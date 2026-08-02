#![deny(missing_docs)]

//! Graphical editor for [`simple_particles`] effects.
//!
//! Runs standalone through the `particle-editor` binary, or embedded in a host
//! window as an [`editor_panels::Panel`] via [`main_panel`].
//!
//! An embedded editor never opens a file dialog: the host routes a clicked path
//! to [`EditorApp::open`] and saves through [`EditorApp::save`].

mod app;
mod axes;
mod locale;
mod textures;
mod ui;
mod wind;

pub use app::{EditorApp, Main, handles, main_panel};
