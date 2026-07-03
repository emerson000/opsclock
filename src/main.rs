mod app;
mod cities;
mod cli;
mod config;
mod led;
mod model;
mod ntp;
mod time;
mod ui;

use app::{App, Flow};
use clap::Parser;
use cli::Args;
use config::Config;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::{execute, terminal};
use model::Clock;
use std::io::{self, Stdout};
use std::time::Duration;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let args = Args::parse();

    let config_path = args.config.clone().unwrap_or_else(Config::default_path);
    let cfg = Config::load(&config_path);
    let mut app = App::new(cfg, config_path);
    apply_cli_overrides(&mut app, &args);

    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app);
    restore_terminal()?;
    app.save();
    result
}

/// CLI values augment/override the loaded config at startup.
fn apply_cli_overrides(app: &mut App, args: &Args) {
    if let Some(layout) = args.layout {
        app.layout = layout.into();
    }
    for query in &args.add {
        if let Some((name, source)) = cities::resolve(query) {
            app.clocks.push(Clock::Tz {
                name,
                source,
                led: app.led_default,
            });
        } else {
            eprintln!("opsclock: unknown clock '{query}'");
        }
    }
    if let Some(spec) = &args.timer {
        match time::parse_duration(spec) {
            Some(dur) if dur > 0 => {
                let n = app.clocks.iter().filter(|c| matches!(c, Clock::Timer { .. })).count();
                let name = if n == 0 {
                    "TIMER".to_string()
                } else {
                    format!("TIMER {}", n + 1)
                };
                app.clocks.push(Clock::Timer {
                    name,
                    duration_ms: dur,
                    elapsed_ms: 0,
                    running: true,
                    last_start: app.now(),
                    led: app.led_default,
                    notified: false,
                });
            }
            _ => eprintln!("opsclock: bad --timer duration '{spec}'"),
        }
    }
    app.sel = app.sel.min(app.clocks.len().saturating_sub(1));
}

type Term = ratatui::Terminal<ratatui::backend::CrosstermBackend<Stdout>>;

fn setup_terminal() -> color_eyre::Result<Term> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen)?;
    // Restore the terminal even if we panic mid-draw.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    Ok(ratatui::Terminal::new(backend)?)
}

fn restore_terminal() -> color_eyre::Result<()> {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), terminal::LeaveAlternateScreen);
    Ok(())
}

fn run(terminal: &mut Term, app: &mut App) -> color_eyre::Result<()> {
    loop {
        terminal.draw(|f| ui::render::draw(f, app))?;
        // Poll at ~100ms so the display refreshes ≥5 Hz for blink/tick.
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.on_key(key) == Flow::Quit {
                    return Ok(());
                }
            }
        }
        app.tick();
    }
}
