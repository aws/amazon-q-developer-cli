use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Stage {
    Prod,
    Gamma,
    Alpha,
    Beta,
}

impl Stage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::Prod => "prod",
            Stage::Gamma => "gamma",
            Stage::Alpha => "alpha",
            Stage::Beta => "beta",
        }
    }
}

impl FromStr for Stage {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().trim() {
            "prod" | "production" => Ok(Stage::Prod),
            "gamma" => Ok(Stage::Gamma),
            "alpha" => Ok(Stage::Alpha),
            "beta" => Ok(Stage::Beta),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
