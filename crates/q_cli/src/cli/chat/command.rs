use eyre::Result;

pub enum Command {
    Ask { prompt: String },
    Execute { command: String },
    Clear,
    Help,
    Quit,
}

impl Command {
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();

        if let Some(command) = input.strip_prefix("/") {
            return Ok(match command.to_lowercase().as_str() {
                "clear" => Self::Clear,
                "help" => Self::Help,
                "q" | "exit" | "quit" => Self::Quit,
                _ => return Err(format!("Unknown command: {}", input)),
            });
        }

        if let Some(command) = input.strip_prefix("!") {
            return Ok(Self::Execute {
                command: command.to_string(),
            });
        }

        Ok(Self::Ask {
            prompt: input.to_string(),
        })
    }
}
