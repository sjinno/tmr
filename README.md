# tmr

A little terminal app for timers, stopwatches, pomodoros, and world clocks. Built with
Rust and Ratatui, with block digits, rounded panels, and your terminal's
background color.

## run

With Rust installed, run from the project directory:

```sh
cargo run
```

To install the `tmr` command:

```sh
cargo install --path .
tmr
```

Make sure `~/.cargo/bin` is on your PATH. After updating the source, reinstall
with `cargo install --path . --force` to update the installed command.

Use a terminal at least 44 columns wide and 16 rows tall. Wider terminals show
the timer list and **on the clock** panel side by side.

## timers and stopwatches

Press `n`, choose **timer**, **stopwatch**, or **pomodoro**, and press `Enter` to continue.
Names and memos are optional. Names keep the casing you enter. New timers and
stopwatches start immediately when saved.

Use `Tab` or `Shift+Tab` to move between fields, `Backspace` to erase,
`Enter` to save, and `Esc` to cancel. Editing an existing timer or stopwatch
resets its current run and leaves it ready to start.
While a form is open, `Ctrl+N` cycles timer → stopwatch → pomodoro, and `Ctrl+P`
cycles backward. Draft values are preserved, including the Pomodoro rest
interval when you switch away and back. The selected type is applied on save.

### durations

Use whole numbers with `h`, `m`, and `s`, with or without spaces.

| input | duration |
| --- | --- |
| `1h` | 1 hour |
| `6m 10s` | 6 minutes, 10 seconds |
| `100s` | 100 seconds |
| `1h30m` | 1 hour, 30 minutes |
| `10` | 10 seconds |
| `10 10` or `10:10` | 10 minutes, 10 seconds |
| `1 30 0` or `1:30:00` | 1 hour, 30 minutes |

Units are case-insensitive. Timer durations must be positive and at most
999 hours. Unnamed timers use a duration-based label, such as `01:30 timer`.

### stopwatch alerts

A stopwatch counts up from zero. Set **alert every** to an interval such as
`5m` or `30s`, or leave it blank to disable alerts. Its gauge tracks progress
toward the next alert. Paused time does not count toward intervals.

Stopwatches continue running after an interval alert. If several intervals
pass between updates, one alert covers them. Resetting starts elapsed time
and the alert schedule over from zero.
The details table counts completed alert intervals under `laps`. Paused time
is excluded, and resetting clears the count. With alerts disabled it shows `--`.

### pomodoro

Choose **pomodoro** to alternate work and rest intervals automatically. Both
**interval** (work) and **rest interval** are required. For example, enter `25m`
and `5m`. Names and memos are optional, and saving starts the run immediately.

The display counts down the remaining time in the current work or rest phase.
The list names the phase, and the gauge tracks progress through it.
An alarm plays at each transition. Pausing freezes elapsed time and the phase
schedule. Resetting returns to the start of a work interval. Intervals and
memos are saved with the preset, and reopening starts elapsed time at zero.
The details grid shows completed work intervals under `intervals` (`done` on
narrow panels). This count increases when work ends, stays fixed during a
pause, and returns to zero on reset.

Press `Esc` to silence an alarm while the pomodoro or stopwatch keeps running.

### controls

| key | action |
| --- | --- |
| `c` | open world clocks |
| `d` | delete the selected item, with confirmation |
| `e` | edit name, duration or alert interval, and memo |
| `f` | toggle favorite |
| `n` | create a timer or stopwatch |
| `q` / `Ctrl+C` | save and quit |
| `r` | reset the selected item |
| `s` | sort favorites first, then by usage count |
| `/` then number, `Enter` | jump to the displayed list number; `Esc` cancels |
| `↑` / `↓` or `k` / `j` | select an item |
| `Space` / `Enter` | start, pause, resume, or reuse |
| `Alt+↑` / `Alt+↓` | reorder the selected item |

List numbers follow the current order. Each fresh start counts as one use;
resuming a paused run does not. Reordering and favorites are saved automatically.

The timer list shows block-digit IDs and green countdowns and `run` labels for
active items. **on the clock** shows the selected item's centered title,
countdown, gauge, memo, and a details grid with start time, duration or alert
interval, and usage count. Start times use local hours and minutes, with UTC
as a fallback. Resetting clears the start time.

When multiple items are running, the panel shows their individual cards.
Each card keeps its details inline, wrapping as needed, including usage and
lap or completed work-interval counts. A memo appears in its own labeled box
below the gauge. Cards grow to fit their contents.
Selecting a running item brings its card into view if the cards overflow.

## world clocks

Press `c` to open compact clock tiles with block digits, local dates, timezone
abbreviations, and UTC offsets. Timers and stopwatch alerts continue running
while you browse clocks.

Press `n` to choose from popular locations, or type a city such as `Tokyo` or
`New York` to search the timezone list. Full IANA names such as `Europe/London`
also work. Use `↑` / `↓` and `Enter` to add a location, or `Esc` to cancel.
Named timezones follow daylight-saving changes automatically.

| key | action |
| --- | --- |
| `d` | remove the selected clock, with confirmation |
| `n` or `/` | search for a city or timezone |
| `q` / `Ctrl+C` | save and quit |
| `t` | return to timers; `c` or `Esc` also works |
| arrows or `j` / `k` | select a clock |
| `Alt+←` / `Alt+→` | reorder clocks; `Alt+↑` / `Alt+↓` also works |

Locations and their order are saved. On smaller terminals, selecting a clock
brings its page into view, and digits shrink to fit.

## alarms and keeping awake

Press `,` on either page to open settings. Set **alarm duration** to `5s`,
`30s`, or another duration to repeat the sound for that long, trimming playback
when the duration ends. Leave it blank to play the sound once (the default).
The setting applies to timer completions and stopwatch interval alerts.
Press `Esc` to silence an alarm. Overlapping alerts share the current playback
and its original deadline. Settings are saved beside your timer data in
`timers.settings.json` by default.

Timer completion and stopwatch interval alerts play the bundled rooster sound
in `assets/alarm.mp3`. Playback uses macOS's built-in `afplay`; other platforms
need `ffplay` on PATH. If playback fails, the app uses a terminal bell instead.
Simultaneous alerts share the current sound. Quitting stops playback.

On macOS, tmr uses `caffeinate` to prevent idle system and display sleep while
any timer or stopwatch is running. The hold is released when all runs pause or
finish, or when the app exits. Keep the app open for countdowns and alerts.

## saved data

Presets, memos, favorites, usage counts, and list order are saved in the
platform's application data directory.

| platform | default timer data file |
| --- | --- |
| macOS | `~/Library/Application Support/tmr/timers.json` |
| Linux | `~/.local/share/tmr/timers.json` (unless `XDG_DATA_HOME` is set) |

Set `TMR_DATA_FILE` to override the full timer data path. Clock locations are
stored beside it in `timers.clocks.json` by default. An overridden filename
gets the same `.clocks.json` extension.

Countdowns reopen at their full duration. Stopwatch elapsed time reopens at
zero. Invalid saved files produce an error without overwriting the original.
Use one app instance per data file.
