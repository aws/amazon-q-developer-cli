#![allow(dead_code)]
use serde::{
    Deserialize,
    Serialize,
};

use crate::protocol::InputEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Quit,
    ClearScreen,
    Error(String),
    Help,
    Input(InputEvent),
    Scroll(Scroll),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scroll {
    Up(ScrollDistance),
    Down(ScrollDistance),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollDistance {
    Message,
    Line(u16),
}
