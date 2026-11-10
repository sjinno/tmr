# tmr

A small terminal home for reusable timers, built with Rust and Ratatui.
Run multiple countdowns, keep your favorites close, and reuse timers without recreating them.

## Run

Install Rust, then from this directory:

```sh
cargo run
```

To install the `tmr` command (ensure `~/.cargo/bin` is on your PATH):

```sh
cargo install --path .
tmr
```

## Keys

| Key | Action |
| --- | --- |
| `n` | Create a timer |
| `↑` / `↓` or `k` / `j` | Select a timer |
| `Space` / `Enter` | Start, pause, resume, or reuse a completed timer |
| `r` | Reset the selected timer |
| `e` | Edit its name and duration; saving resets its countdown |
| `d` | Delete, with confirmation |
| `f` | Toggle favorite |
| `J` / `K` | Move selected timer down / up in your ranking |
| `s` | Sort favorites first, then by number of starts |
| `q` / `Ctrl-C` | Save and quit |

In the timer form, use `Tab` to switch fields, `Backspace` to erase,
`Enter` to save, and `Esc` to cancel. New timers start with an empty duration.
Saving a new timer starts its countdown immediately.
Names are optional; unnamed timers display a duration-based label like `01:30 timer`.
Enter durations with units: `1h`, `6m 10s`, `100s`, or `1h30m`.
Units are case-insensitive; spaces between values and units are optional.
Use whole numbers with a positive total, up to 999 hours.
Plain seconds (`10`), minutes and seconds (`10 10`), hours/minutes/seconds
(`1 30 0`), and colon-separated durations (`1:30` or `1:00:00`) also work.

Each fresh start counts as one use; resuming a paused timer does not.
Multiple timers can run at once. Completion displays a message and plays the
bundled rooster alarm (`assets/alarm.mp3`). On macOS this uses the built-in
`afplay`; other platforms need `ffplay` on PATH. Playback runs without blocking
the UI, and simultaneous completions share the current sound. If playback is
unavailable or fails, tmr sends a terminal bell instead (audibility depends on
terminal settings). Quitting stops any playing alarm. The MP3 is embedded in
the binary, so the original Downloads file is no longer needed. Keep the app open
for countdowns and alerts; it does not run in the background after quitting.

Timer presets, favorites, usage counts, and manual order are saved automatically
in the platform's application data directory (`tmr/timers.json`). On macOS this
is `~/Library/Application Support/tmr/timers.json`; on Linux it is usually
`~/.local/share/tmr/timers.json`. Set `TMR_DATA_FILE` to override the full file
path. Countdowns reopen at their full duration. Invalid saved files produce an
error without overwriting the original. Run one instance per data file.

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
