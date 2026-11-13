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
| `n` | Choose Timer or Stopwatch, then create it |
| `↑` / `↓` or `k` / `j` | Select a timer |
| `/` then number, `Enter` | Jump to a timer by its displayed list number; `Esc` cancels |
| `Space` / `Enter` | Start, pause, resume, or reuse a completed timer |
| `r` | Reset the selected timer |
| `e` | Edit its name and duration; saving resets its countdown |
| `d` | Delete, with confirmation |
| `f` | Toggle favorite |
| `J` / `K` | Move selected timer down / up in your ranking |
| `s` | Sort favorites first, then by number of starts |
| `c` | Open world clocks |
| `q` / `Ctrl-C` | Save and quit |

In the timer form, use `Tab` to switch fields, `Backspace` to erase,
`Enter` to save, and `Esc` to cancel. New timers start with an empty duration.
Saving a new timer starts its countdown immediately.
For a stopwatch, set **Alert every** to a duration such as `5m` or `30s`,
or leave it blank for no alerts. Stopwatches start counting up immediately.
Space pauses or resumes, and `r` resets elapsed time to zero. Interval alerts
use active elapsed time, excluding pauses, and play the same alarm sound.
If several intervals pass between updates, one alert covers them. The gauge
shows progress toward the next alert. Stopwatch presets and intervals are
saved, and elapsed time starts fresh when you reopen the app.
Names are optional; unnamed timers display a duration-based label like `01:30 timer`.
Each timer also has an optional memo (up to 80 characters). Use `Tab` to move
between name, duration, and memo; `Shift-Tab` moves backward. Memos are saved
with the timer and shown in the list and countdown panel. Press `e` to edit them.
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

Press `c` for world-clock tiles with block digits, local dates, and UTC offsets.
In this view, `n` opens a picker with popular locations. Type a city like
`New York` or `Tokyo`, or an IANA timezone such as `Europe/London`, to search
the full timezone list. Use arrows and `Enter` to add a location. Named zones
automatically follow daylight-saving changes. Arrows select tiles, `d` removes
the selected clock with confirmation, and `t` returns to the main timer
page. `c` and `Esc` also return to timers.
Timers and stopwatch alerts continue in the clock view. Saved locations live
in `timers.clocks.json` beside the timer data file. On smaller terminals,
selecting a tile brings its page into view, and digits shrink to fit.

## Development

For automatic rebuilds and restarts while editing, run:

```sh
./dev.sh
```

This uses `cargo-watch` to watch `src`, `assets`, and `Cargo.toml`. Install it
once with `cargo install cargo-watch` if needed. Source changes restart the
app, so active countdowns and stopwatch elapsed time reset. Use `Ctrl-C` to
stop the watcher after closing the app with `q`.

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
