//! Command-line arguments (clap derive). CLI values augment/override the loaded
//! config at startup.

use crate::model::Layout;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum LayoutArg {
    Grid,
    Split,
    Sidebar,
    Wall,
}

impl From<LayoutArg> for Layout {
    fn from(a: LayoutArg) -> Self {
        match a {
            LayoutArg::Grid => Layout::Grid,
            LayoutArg::Split => Layout::Split,
            LayoutArg::Sidebar => Layout::Sidebar,
            LayoutArg::Wall => Layout::Wall,
        }
    }
}

/// Mission-control-style terminal world-clock TUI.
#[derive(Parser, Debug)]
#[command(name = "opsclock", version, about)]
pub struct Args {
    /// Start in this layout (overrides the saved config).
    #[arg(long, value_enum)]
    pub layout: Option<LayoutArg>,

    /// Add a clock on startup (city name or UTC±offset). Repeatable.
    #[arg(long, value_name = "CITY")]
    pub add: Vec<String>,

    /// Start a running countdown timer (e.g. 20m, 1h30m, 00:20:00).
    #[arg(long, value_name = "DURATION")]
    pub timer: Option<String>,

    /// Config file path (defaults to the platform config dir).
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}
