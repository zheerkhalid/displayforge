//! DisplayForge: monitor control for Windows.
//!
//! The logic lives in this library so that it can be unit-tested; `main.rs`
//! only parses the command line.
pub mod cities;
pub mod cli;
pub mod color;
pub mod control;
pub mod demo;
pub mod engine;
pub mod foreground;
pub mod hotkey;
pub mod hw;
pub mod mock;
pub mod model;
pub mod policy;
pub mod profiles;
pub mod revert;
pub mod schedule;
pub mod selftest;
pub mod service;
pub mod settings;
pub mod solar;
pub mod sys;
pub mod timeutil;
pub mod transition;
pub mod ui;
