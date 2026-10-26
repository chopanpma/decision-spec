//! DecisionSpec TUI — a "spec cockpit" over the projects registered in
//! `~/.config/decispec/registry.toml`.
//!
//! Pure by design (`AGENT_TEAMS.md` F31/F32): this crate holds app state,
//! input transitions, the spec editor, and widget rendering into a ratatui
//! [`ratatui::buffer::Buffer`] (testable with `TestBackend`). The `cli` crate
//! owns the terminal (crossterm raw mode + event pump), the registry file,
//! all workspace loading — and the two explicit write actions: Ctrl+S saves
//! an edited spec file into its project, and `p` opens a draft PR there.
//! Everything else never writes; generated artifacts are rendered in memory
//! via `codegen`'s pure render functions and shown on screen only.

pub mod app;
pub mod editor;
pub mod registry;
pub mod snapshot;
pub mod views;
