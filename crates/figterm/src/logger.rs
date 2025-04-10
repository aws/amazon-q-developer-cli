use std::fmt::Display;

use fig_log::get_log_level_max;
use tracing::Level;

pub fn stdio_debug_log(s: impl Display) {
    if get_log_level_max() >= Level::DEBUG {
        println!("{s}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_debug_log() {
        stdio_debug_log("test");
    }
}
