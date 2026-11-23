//! The egui user interface: the main window with its pages, and the quick
//! panel (flyout) next to the notification area.
use crate::engine::{Control, Shared};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;

mod app;
mod core;
mod display_view;
mod flyout;
mod info_view;
mod monitors;
mod profiles_view;
mod schedule_view;
mod settings_view;
mod theme;
mod tools;
mod widgets;

/// Which window to open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The main window.
    #[default]
    Main,
    /// The small quick panel next to the notification area; closes when it loses focus.
    Flyout,
}

#[derive(Clone, Default)]
pub struct Options {
    pub mode: Mode,
    /// Render one frame of the chosen view to this PNG (1280 x 800), then exit.
    pub screenshot: Option<PathBuf>,
    /// Which page / dialog to open first.
    pub view: Option<String>,
    /// Force the light theme (screenshots).
    pub light: bool,
    pub shared: Option<Arc<Shared>>,
    pub control: Option<Arc<dyn Control>>,
}

/// Opens the window and returns when it is closed.
pub fn run(options: Options, icon: egui::IconData) -> eframe::Result<()> {
    match options.mode {
        Mode::Main => app::run_main(options, icon),
        Mode::Flyout => flyout::run_flyout(options, icon),
    }
}
