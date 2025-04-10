#![warn(rust_2018_idioms, future_incompatible)]

pub mod ansi;
pub mod event;
pub mod grid;
pub mod index;
pub mod term;

pub use crate::grid::Grid;
pub use crate::term::Term;
