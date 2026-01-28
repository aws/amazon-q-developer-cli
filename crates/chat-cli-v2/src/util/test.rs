macro_rules! assert_parse {
    (
        [ $($args:expr),+ ],
        $subcommand:expr
    ) => {
        assert_eq!(
            <crate::cli::Cli as clap::Parser>::parse_from([crate::util::CHAT_BINARY_NAME, $($args),*]),
            crate::cli::Cli {
                subcommand: Some($subcommand),
                ..Default::default()
            }
        );
    };
}

pub(crate) use assert_parse;
