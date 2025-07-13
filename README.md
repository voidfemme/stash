### `README.md`

````markdown
# stash

`stash` is a small command-line utility that runs any given command, “tees” its output into a timestamped log file, and keeps only the last *N* logs (default 20). You can also specify certain programs (e.g. curses/TUI apps) to be run directly without logging.

## Features

- Rolling log directory (default `~/.cache/stash`)
- Retain only the *N* most recent log files
- Configurable ignore-list via `~/.config/stash/stash.toml` or `--ignore`
- Simple install script to build and copy the binary into your `PATH`
- MIT/CC0 license (public domain)

## Installation

```bash
git clone https://github.com/yourusername/stash.git
cd stash
./install.sh
````

This will:

1. Build `stash` in release mode.
2. Copy the resulting `stash` binary into `~/.local/bin` (creates it if needed).

Make sure `~/.local/bin` is in your `PATH`.

## Usage

```bash
stash --retain 10 --ignore vim --ignore tmux -- echo "hello world"
```

* `--log-dir`: where logs are kept (default `~/.cache/stash`)
* `--retain`: how many logs to keep (default 20)
* `--ignore`: one or more program names to run without logging
* `-- <cmd>…`: the command (and its args) to execute and log

## Configuration

Create `~/.config/stash/stash.toml` with:

```toml
ignore = ["vim", "htop"]
```

Entries here are merged with any `--ignore` flags you pass.

## Development

```bash
# run tests or try out
cargo test
cargo run -- echo "test"
```

## License

This software is dedicated to the public domain under CC0. See [LICENSE](LICENSE) for details.

````

---

### `LICENSE` (CC0 1.0 Universal)

```text
CC0 1.0 Universal

Copyright (c) 2025 Your Name

To the extent possible under law, the person who associated CC0 with this work has waived all copyright and related or neighboring rights to this work.

You can copy, modify, distribute and perform the work, even for commercial purposes, all without asking permission. 

See https://creativecommons.org/publicdomain/zero/1.0/ for full details.
````
