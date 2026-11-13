#!/bin/sh
# Restore the terminal even if the watcher force-stops the TUI.
restore_terminal() {
    stty sane < /dev/tty 2>/dev/null || true
    printf '\033[?1049l\033[?25h\033[0m' > /dev/tty
}
trap restore_terminal EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
cargo watch --watch src --watch assets --watch Cargo.toml --exec run
