# Opsclock

A mission-control-style terminal world-clock TUI: multiple world clocks (by city
or UTC offset), NASA-style countdown timers and stopwatches, point-in-time
conversion across every clock, four layouts, a red LED dot-matrix display mode,
and real NTP server sync. Keyboard-first — every binding is a single key.

Built with [ratatui](https://ratatui.rs) + crossterm, [jiff](https://docs.rs/jiff)
(bundled tz database, so it works on musl/Windows without system tzdata), clap,
serde/toml, and color-eyre.

## Build & run

```sh
cargo run --release
```

The binary is self-contained; the timezone database is compiled in.

### Cargo features

- `notify` (default **on**) — desktop notification via `notify-rust` when a timer
  hits zero. Build a pure-terminal binary with no desktop-notification dependency:

  ```sh
  cargo build --release --no-default-features
  ```

## CLI

```
opsclock [OPTIONS]

  --layout <grid|split|sidebar|wall>   Start in this layout (overrides config)
  --add <CITY>                         Add a clock on startup (repeatable);
                                       city name or UTC±offset, e.g. --add Tokyo --add UTC+5:30
  --timer <DURATION>                   Start a running countdown (20m, 1h30m, 00:20:00)
  --config <PATH>                      Config file (defaults to the platform config dir)
```

CLI values augment/override the loaded config at startup.

## Keys (normal mode)

| Key | Action |
|-----|--------|
| `← → Tab` (also `↑ ↓`) | select clock |
| `1` `2` `3` `4` | grid / split / sidebar / wall layout |
| `l` | toggle LED matrix on the selected clock |
| `t` | set time on the selected zone clock — converts **all** clocks (e.g. `17:00 tomorrow`) |
| `T` | new countdown timer (`20m`, `1h30m`, `00:20:00`) |
| `s` | new stopwatch (instant, counts up) |
| `space` | run / pause (or restart a timer held at zero) |
| `r` | reset / restart the selected timer or stopwatch |
| `a` | add a clock (city or `UTC+5:30`) |
| `x` | close the selected clock (refuses the last one) |
| `o` | label mode: city names ↔ military zone (`TOKYO` ↔ `INDIA (I)`) |
| `n` | time-server sync panel |
| `?` | key-reference overlay |
| `Esc` | resume live / close overlay |
| `q` / `Ctrl-C` | quit |

In the **input bar**: printable chars append (max 32), `Backspace` deletes,
`Enter` submits, `Esc` cancels. In the **sync** and **help** panels, `↑↓`/`j k`
move and `Enter`/`Esc` act; all other keys are swallowed.

## Behavior notes

- **Timers** display `T-HH:MM:SS` remaining and, on reaching zero, **hold at
  `T-00:00:00` and blink** (~2 Hz) until restarted — they never go negative and
  are reusable. Stopwatches display `T+HH:MM:SS` counting up.
- **Set time** computes the instant of a wall time on that clock's civil date
  (optionally `+1` day) in that clock's zone, using jiff's DST-safe resolution,
  and shows it across every zone clock until `Esc`.
- **Day chips** (`+1 SAT` / `-1 FRI`) appear when a clock's civil date differs
  from the local date. **Military** sub-labels map whole-hour offsets to NATO
  letters (`UTC+09:00 · I`).
- **NTP sync** performs a real SNTP (single UDP exchange) to the selected server
  on a background thread; the measured offset is applied to all displays and
  shown in the header (`SYNC pool.ntp.org +4MS`).
- **Config** (clocks, layout, label mode, NTP server, LED default) is persisted
  as TOML on exit and reloaded on start. Timers/stopwatches load stopped.

## Development

```sh
cargo test        # 39 unit tests (parsers, timer state machine, conversion, resolver, …)
cargo clippy
```

## Layout

```
src/
  main.rs      CLI parse, config load, terminal loop, panic-safe restore
  cli.rs       clap args
  config.rs    TOML load/save, default seed clocks
  model.rs     Clock enum, Source, Layout, timer/stopwatch state machine
  time.rs      duration/offset parsing, military letters, jiff wall math & set-time
  cities.rs    curated city list + tzdb fallback + offset parsing
  led.rs       5×7 dot-matrix font and art
  ntp.rs       real SNTP client (background thread + mpsc)
  app.rs       App state, keymap dispatcher, per-tick update
  ui/
    theme.rs   design-token colors
    layouts.rs per-layout rectangle math
    render.rs  chrome, tiles, overlays
```
