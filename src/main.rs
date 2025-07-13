// src/main.rs

// --------------------------------------------------------------------------------
// External crates for:
//  1) CLI parsing (Clap)
//  2) Timestamping (Chrono)
//  3) Expanding “~” in paths (Dirs)
// --------------------------------------------------------------------------------
use chrono::Local;
use clap::Parser;
use dirs::home_dir;
use serde::Deserialize;
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

#[derive(Deserialize, Debug)]
struct ConfigFile {
    /// list of program names for which we skip logging entirely
    ignore: Option<Vec<String>>,
}

fn load_config_file() -> ConfigFile {
    // find $XDG_CONFIG_HOME/stash/stash.toml (or fallback to ~/.config)
    let config_path = dirs::config_dir()
        .expect("no config dir")
        .join("stash")
        .join("stash.toml");

    if let Ok(toml_str) = std::fs::read_to_string(&config_path) {
        toml::from_str(&toml_str)
            .unwrap_or_else(|e| panic!("invalid TOML in {:?}: {}", config_path, e))
    } else {
        ConfigFile { ignore: None }
    }
}

/// Command‐line options, parsed via Clap
#[derive(Parser)]
#[clap(
    name = "stash",
    version = "0.1",
    about = "Run any command, tee its output to a timestamped log, and keep only the last N logs."
)]
struct Opts {
    /// Directory in which to keep per‐command logs
    #[clap(
        long,
        default_value = "~/.cache/stash",
        help = "Where to store rolling logs of past commands"
    )]
    log_dir: PathBuf,

    /// How many logs to keep around before pruning
    #[clap(long, default_value = "20", help = "Max number of log files to retain")]
    retain: usize,

    /// Override or add to the list of commands we ignore (e.g. tui apps)
    /// (space separated)
    #[clap(long, value_name = "PROG", num_args = 1..)]
    ignore: Vec<String>,

    /// The actual command (and its args) to run; everything after `--`
    #[clap(required = true, last = true, help = "The command to run and log")]
    cmd: Vec<String>,
}

fn main() -> io::Result<()> {
    // 1. Parse CLI args
    let mut opts = Opts::parse();

    // 2. Expand "~" to the user’s home directory, if present
    if let Some(home) = home_dir() {
        if opts.log_dir.starts_with("~") {
            // Strip the "~" and join with the real home path
            opts.log_dir = home.join(opts.log_dir.strip_prefix("~").unwrap());
        }
    }

    // 3. Ensure the log directory exists
    fs::create_dir_all(&opts.log_dir)?;

    // 4. Prune old logs so we never exceed `opts.retain`
    rotate_old(&opts.log_dir, opts.retain)?;

    // 5. Compute a fresh logfile name, e.g. "20250712-153045.123.log"
    let logfile = opts
        .log_dir
        .join(format!("{}.log", Local::now().format("%Y%m%d-%H%M%S%.3f")));
    let log = fs::File::create(&logfile)?;

    // 6. Load defaults from stash.toml
    let file_cfg = load_config_file();
    let mut ignore_list = file_cfg.ignore.unwrap_or_default();

    // 7. Append any --ignore entries (CLI wins / extends)
    ignore_list.extend(opts.ignore.clone());

    // 8. Deduplicate so I don't accidentally run twice
    ignore_list.sort();
    ignore_list.dedup();

    // 9. Grab the program name
    let prog = &opts.cmd[0];

    // 10. If it's in our ignore_list, exec it *directly*, inheriting stdio,
    //      so the user sees a normal interactive curses session- and we never log
    if ignore_list.iter().any(|p| p == prog) {
        let status = std::process::Command::new(prog)
            .args(&opts.cmd[1..])
            // inherit all stdio so the TUI app can take over your terminal
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?;
        std::process::exit(status.code().unwrap_or(1));
    }

    // 10. Launch the real child process, capturing both stdout and stderr pipes
    let mut child = Command::new(&opts.cmd[0])
        .args(&opts.cmd[1..])
        // Tell Rust to give us handles to stdout/stderr so we can read them
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // 11. Take the pipes out of the child and spawn two tee‐threads
    let stdout_pipe = child.stdout.take().unwrap();
    let stderr_pipe = child.stderr.take().unwrap();

    // 12. We clone the File handle so stdout and stderr threads can each write to it
    let log_clone = log.try_clone()?;
    let handle_out = spawn_tee(stdout_pipe, log_clone, false);
    let handle_err = spawn_tee(stderr_pipe, log, true);

    // 13. Wait for the child to exit, then join both threads so they've finished writing
    let status = child.wait()?;
    handle_out.join().unwrap();
    handle_err.join().unwrap();

    // 14. Propagate the child’s exit code as our own
    std::process::exit(status.code().unwrap_or(1));
}

/// Spawn a thread that "tees" everything from `pipe` into both
/// 1) the real terminal (stdout or stderr), and
/// 2) our logfile (`writer`).
///
/// Take `pipe` as any `impl Read + Send + 'static`.
fn spawn_tee<P>(pipe: P, mut writer: fs::File, is_err: bool) -> JoinHandle<()>
where
    P: Read + Send + 'static,
{
    // Wrap the incoming pip in a buffered reader so we can read line-by-line
    let mut reader = BufReader::new(pipe);

    // Box-up either stdout or stderr behind the same trait object:
    let term: Box<dyn Write + Send> = if is_err {
        Box::new(io::stderr())
    } else {
        Box::new(io::stdout())
    };
    let term = Arc::new(Mutex::new(term));

    // Spawn a thread that:
    //      - loops on reader.read_line()
    //      - writes each line to the real terminal AND to my logfile
    thread::spawn(move || {
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            // a) Write to the terminal
            {
                let mut out = term.lock().unwrap();
                write!(out, "{}", line).unwrap();
            }
            // b) append to the logfile
            writer.write_all(line.as_bytes()).unwrap();
            line.clear();
        }
    })
}

/// Deletes oldest `.log` files so that only `retain` newest remain
fn rotate_old(dir: &PathBuf, retain: usize) -> io::Result<()> {
    // 1. Collect all ".log" entries
    let mut logs: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("log"))
        .collect();

    // 2. Sort by filename (we named them with timestamps, so this is chronological)
    logs.sort_by_key(|e| e.path());

    // 3. While we have more than `retain`, delete the oldest (front of the vec)
    while logs.len() > retain {
        let old = logs.remove(0);
        // ignore any error deleting old logs
        let _ = fs::remove_file(old.path());
    }
    Ok(())
}
