use crossterm::style::{
    ResetColor,
    SetAttribute,
    SetForegroundColor,
};

/// This trait is purely here to facilitate a smooth transition from the old event loop to a new
/// event loop. It is a way to achieve inversion of control to delegate the implementation of
/// themes to the consumer of this crate. Without this, we would be running into a circular
/// dependency.
pub trait ThemeSource {
    fn error(&self, text: &str) -> String;
    fn info(&self, text: &str) -> String;
    fn emphasis(&self, text: &str) -> String;
    fn command(&self, text: &str) -> String;
    fn prompt(&self, text: &str) -> String;
    fn profile(&self, text: &str) -> String;
    fn tangent(&self, text: &str) -> String;
    fn usage_low(&self, text: &str) -> String;
    fn usage_medium(&self, text: &str) -> String;
    fn usage_high(&self, text: &str) -> String;
    fn brand(&self, text: &str) -> String;
    fn primary(&self, text: &str) -> String;
    fn secondary(&self, text: &str) -> String;
    fn success(&self, text: &str) -> String;
    fn error_fg(&self) -> SetForegroundColor;
    fn warning_fg(&self) -> SetForegroundColor;
    fn success_fg(&self) -> SetForegroundColor;
    fn info_fg(&self) -> SetForegroundColor;
    fn brand_fg(&self) -> SetForegroundColor;
    fn secondary_fg(&self) -> SetForegroundColor;
    fn emphasis_fg(&self) -> SetForegroundColor;
    fn reset(&self) -> ResetColor;
    fn reset_attributes(&self) -> SetAttribute;
}
