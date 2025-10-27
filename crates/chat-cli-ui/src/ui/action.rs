#![allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Tick,
    Noop,
}
