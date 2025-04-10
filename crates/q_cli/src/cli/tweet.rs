use anstream::println;
use crossterm::style::Stylize;
use eyre::Result;
use rand::prelude::*;
use url::Url;

const TWEET_OPTIONS: &[(&str, bool)] = &[
    ("I've added autocomplete to my terminal using @fig!\n\n🛠🆕👉️", true),
    (
        "I've added autocomplete to my terminal using @fig! It's super fast and integrates with my existing \
         terminal.\n\n🛠🆕👉️",
        true,
    ),
    (
        "I just added autocomplete to my terminal using @fig! It supports 500+ CLI tools and fits into my workflow \
         seamlessly!\n\n🛠🆕👉️",
        true,
    ),
    (
        "I just added IDE-style autocomplete to my terminal using @fig. It supports 500+ CLI tools and works with my \
         existing terminal! Try it out\n\n🛠🆕🔥",
        false,
    ),
];

fn tweet_url() -> Result<Url> {
    let mut rng = rand::rng();
    let (tweet, with_link) = TWEET_OPTIONS.choose(&mut rng).unwrap_or(&TWEET_OPTIONS[0]);

    let mut params = vec![("text", *tweet), ("related", "codewhisperer")];

    if *with_link {
        params.push(("url", "https://fig.io"));
    }

    Ok(Url::parse_with_params("https://twitter.com/intent/tweet", &params)?)
}

pub fn tweet_cli() -> Result<()> {
    println!();
    println!("→ Opening Twitter...");
    println!();

    let url = tweet_url()?;

    // Open the default browser to the tweet URL
    if fig_util::open_url(url.as_str()).is_err() {
        println!("{}", url.as_str().underlined());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tweet_url() {
        let url = tweet_url().unwrap();
        assert!(url.as_str().starts_with("https://twitter.com/intent/tweet"));
    }
}
