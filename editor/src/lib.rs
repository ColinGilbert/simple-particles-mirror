#![deny(missing_docs)]

//! Graphical editor for [`simple_particles`] effects.
//!
//! Runs standalone through the `particle-editor` binary, or embedded in a host
//! window as an [`editor_panels::Panel`] via [`main_panel`].
//!
//! An embedded editor never opens a file dialog and never touches the host's
//! style: the host routes a clicked path to [`EditorApp::open`], saves through
//! [`EditorApp::save`], and dresses its own context. A standalone window calls
//! [`configure_style`] once to wear the same look.

mod app;
mod axes;
mod locale;
mod textures;
mod ui;
mod wind;

pub use app::{EditorApp, Main, handles, main_panel};

/// Applies the shared editor-suite look to a whole window.
///
/// For a standalone window only. A panel embedded in a host must leave the style
/// alone: one window is one [`egui::Context`] and one style, so a panel that
/// overrode it locally would still get its menus, combo popups and tooltips
/// drawn from the global one.
pub fn configure_style(context: &egui::Context) {
    editor_panels::dress(context, editor_panels::Theme::default());
}
