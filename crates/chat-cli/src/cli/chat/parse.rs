use std::io::Write;

use crossterm::style::{
    Attribute,
    Color,
    Stylize,
};
use crossterm::{
    Command,
    style,
};
use unicode_width::{
    UnicodeWidthChar,
    UnicodeWidthStr,
};
use winnow::Partial;
use winnow::ascii::{
    self,
    digit1,
    space0,
    space1,
    till_line_ending,
};
use winnow::combinator::{
    alt,
    delimited,
    preceded,
    repeat,
    terminated,
};
use winnow::error::{
    ErrMode,
    ErrorKind,
    ParserError,
};
use winnow::prelude::*;
use winnow::stream::{
    AsChar,
    Stream,
};
use winnow::token::{
    any,
    take_till,
    take_until,
    take_while,
};

const CODE_COLOR: Color = Color::Green;
const HEADING_COLOR: Color = Color::Magenta;
const BLOCKQUOTE_COLOR: Color = Color::DarkGrey;
const URL_TEXT_COLOR: Color = Color::Blue;
const URL_LINK_COLOR: Color = Color::DarkGrey;

const DEFAULT_RULE_WIDTH: usize = 40;

#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error(transparent)]
    Stdio(#[from] std::io::Error),
    #[error("parse error {1}, input {0}")]
    Winnow(Partial<&'a str>, ErrorKind),
}

impl<'a> ParserError<Partial<&'a str>> for Error<'a> {
    fn from_error_kind(input: &Partial<&'a str>, kind: ErrorKind) -> Self {
        Self::Winnow(*input, kind)
    }

    fn append(
        self,
        _input: &Partial<&'a str>,
        _checkpoint: &winnow::stream::Checkpoint<
            winnow::stream::Checkpoint<&'a str, &'a str>,
            winnow::Partial<&'a str>,
        >,
        _kind: ErrorKind,
    ) -> Self {
        self
    }
}

#[derive(Debug)]
pub struct ParseState {
    pub terminal_width: Option<usize>,
    pub markdown_disabled: Option<bool>,
    pub column: usize,
    pub in_codeblock: bool,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub set_newline: bool,
    pub newline: bool,
    pub citations: Vec<(String, String)>,
}

impl ParseState {
    pub fn new(terminal_width: Option<usize>, markdown_disabled: Option<bool>) -> Self {
        Self {
            terminal_width,
            markdown_disabled,
            column: 0,
            in_codeblock: false,
            bold: false,
            italic: false,
            strikethrough: false,
            set_newline: false,
            newline: true,
            citations: vec![],
        }
    }
}

pub fn interpret_markdown<'a, 'b>(
    mut i: Partial<&'a str>,
    mut o: impl Write + 'b,
    state: &mut ParseState,
) -> PResult<Partial<&'a str>, Error<'a>> {
    let mut error: Option<Error<'_>> = None;
    let start = i.checkpoint();

    macro_rules! stateful_alt {
        ($($fns:ident),*) => {
            $({
                i.reset(&start);
                match $fns(&mut o, state).parse_next(&mut i) {
                    Err(ErrMode::Backtrack(e)) => {
                        error = match error {
                            Some(error) => Some(error.or(e)),
                            None => Some(e),
                        };
                    },
                    res => {
                        return res.map(|_| i);
                    }
                }
            })*
        };
    }

    match (state.in_codeblock, state.markdown_disabled.unwrap_or(false)) {
        (_, true) => {
            // If markdown is disabled, do not include markdown-related parsers
            stateful_alt!(text, line_ending, fallback);
        },
        (false, false) => {
            stateful_alt!(
                // This pattern acts as a short circuit for alphanumeric plaintext
                // More importantly, it's needed to support manual wordwrapping
                text,
                // multiline patterns
                blockquote,
                // linted_codeblock,
                codeblock_begin,
                // single line patterns
                horizontal_rule,
                heading,
                bulleted_item,
                numbered_item,
                // inline patterns
                code,
                citation,
                url,
                bold,
                italic,
                strikethrough,
                // symbols
                less_than,
                greater_than,
                ampersand,
                quot,
                line_ending,
                // fallback
                fallback
            );
        },
        (true, false) => {
            stateful_alt!(
                codeblock_less_than,
                codeblock_greater_than,
                codeblock_ampersand,
                codeblock_quot,
                codeblock_end,
                codeblock_line_ending,
                codeblock_fallback
            );
        },
    }

    match error {
        Some(e) => Err(ErrMode::Backtrack(e.append(&i, &start, ErrorKind::Alt))),
        None => Err(ErrMode::assert(&i, "no parsers")),
    }
}

fn text<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        let content = take_while(1.., |t| AsChar::is_alphanum(t) || "+,.!?\"".contains(t)).parse_next(i)?;
        queue_newline_or_advance(&mut o, state, content.width())?;
        queue(&mut o, style::Print(content))?;
        Ok(())
    }
}

fn heading<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        if !state.newline {
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }

        let level = terminated(take_while(1.., |c| c == '#'), space1).parse_next(i)?;
        let print = format!("{level} ");

        queue_newline_or_advance(&mut o, state, print.width())?;
        queue(&mut o, style::SetForegroundColor(HEADING_COLOR))?;
        queue(&mut o, style::SetAttribute(Attribute::Bold))?;
        queue(&mut o, style::Print(print))
    }
}

fn bulleted_item<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        if !state.newline {
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }

        let ws = (space0, alt(("-", "*")), space1).parse_next(i)?.0;
        let print = format!("{ws}• ");

        queue_newline_or_advance(&mut o, state, print.width())?;
        queue(&mut o, style::Print(print))
    }
}

fn numbered_item<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        if !state.newline {
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }

        let (ws, digits, _, _) = (space0, digit1, ".", space1).parse_next(i)?;
        let print = format!("{ws}{digits}. ");

        queue_newline_or_advance(&mut o, state, print.width())?;
        queue(&mut o, style::Print(print))
    }
}

fn horizontal_rule<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        if !state.newline {
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }

        (
            space0,
            alt((take_while(3.., '-'), take_while(3.., '*'), take_while(3.., '_'))),
        )
            .parse_next(i)?;

        state.column = 0;
        state.set_newline = true;

        let rule_width = state.terminal_width.unwrap_or(DEFAULT_RULE_WIDTH);
        queue(&mut o, style::Print(format!("{}\n", "━".repeat(rule_width))))
    }
}

fn code<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "`".parse_next(i)?;
        let code = terminated(take_until(0.., "`"), "`").parse_next(i)?;
        let out = code.replace("&amp;", "&").replace("&gt;", ">").replace("&lt;", "<");

        queue_newline_or_advance(&mut o, state, out.width())?;
        queue(&mut o, style::SetForegroundColor(Color::Green))?;
        queue(&mut o, style::Print(out))?;
        queue(&mut o, style::ResetColor)
    }
}

fn blockquote<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        if !state.newline {
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }

        let level = repeat::<_, _, Vec<&'_ str>, _, _>(1.., terminated(">", space0))
            .parse_next(i)?
            .len();
        let print = "│ ".repeat(level);

        queue(&mut o, style::SetForegroundColor(BLOCKQUOTE_COLOR))?;
        queue_newline_or_advance(&mut o, state, print.width())?;
        queue(&mut o, style::Print(print))
    }
}

fn bold<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        match state.newline {
            true => {
                alt(("**", "__")).parse_next(i)?;
                queue(&mut o, style::SetAttribute(Attribute::Bold))?;
            },
            false => match state.bold {
                true => {
                    alt(("**", "__")).parse_next(i)?;
                    queue(&mut o, style::SetAttribute(Attribute::NormalIntensity))?;
                },
                false => {
                    preceded(space1, alt(("**", "__"))).parse_next(i)?;
                    queue(&mut o, style::Print(' '))?;
                    queue(&mut o, style::SetAttribute(Attribute::Bold))?;
                },
            },
        };

        state.bold = !state.bold;

        Ok(())
    }
}

fn italic<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        match state.newline {
            true => {
                alt(("*", "_")).parse_next(i)?;
                queue(&mut o, style::SetAttribute(Attribute::Italic))?;
            },
            false => match state.italic {
                true => {
                    alt(("*", "_")).parse_next(i)?;
                    queue(&mut o, style::SetAttribute(Attribute::NoItalic))?;
                },
                false => {
                    preceded(space1, alt(("*", "_"))).parse_next(i)?;
                    queue(&mut o, style::Print(' '))?;
                    queue(&mut o, style::SetAttribute(Attribute::Italic))?;
                },
            },
        };

        state.italic = !state.italic;

        Ok(())
    }
}

fn strikethrough<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "~~".parse_next(i)?;
        state.strikethrough = !state.strikethrough;
        match state.strikethrough {
            true => queue(&mut o, style::SetAttribute(Attribute::CrossedOut)),
            false => queue(&mut o, style::SetAttribute(Attribute::NotCrossedOut)),
        }
    }
}

fn citation<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        let num = delimited("[[", digit1, "]]").parse_next(i)?;
        let link = delimited("(", take_till(0.., ')'), ")").parse_next(i)?;

        state.citations.push((num.to_owned(), link.to_owned()));

        queue_newline_or_advance(&mut o, state, num.width() + 1)?;
        queue(&mut o, style::SetForegroundColor(URL_TEXT_COLOR))?;
        queue(&mut o, style::Print(format!("[^{num}]")))?;
        queue(&mut o, style::ResetColor)
    }
}

fn url<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        // Save the current input position
        let start = i.checkpoint();

        // Try to match the first part of URL pattern "[text]"
        let display = match delimited::<_, _, _, _, Error<'a>, _, _, _>("[", take_until(1.., "]("), "]").parse_next(i) {
            Ok(display) => display,
            Err(_) => {
                // If it doesn't match, reset position and fail
                i.reset(&start);
                return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
            },
        };

        // Try to match the second part of URL pattern "(url)"
        let link = match delimited::<_, _, _, _, Error<'a>, _, _, _>("(", take_till(0.., ')'), ")").parse_next(i) {
            Ok(link) => link,
            Err(_) => {
                // If it doesn't match, reset position and fail
                i.reset(&start);
                return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
            },
        };

        // Only generate output if the complete URL pattern matches
        queue_newline_or_advance(&mut o, state, display.width() + 1)?;
        queue(&mut o, style::SetForegroundColor(URL_TEXT_COLOR))?;
        queue(&mut o, style::Print(format!("{display} ")))?;
        queue(&mut o, style::SetForegroundColor(URL_LINK_COLOR))?;
        state.column += link.width();
        queue(&mut o, style::Print(link))?;
        queue(&mut o, style::ResetColor)
    }
}

fn less_than<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&lt;".parse_next(i)?;
        queue_newline_or_advance(&mut o, state, 1)?;
        queue(&mut o, style::Print('<'))
    }
}

fn greater_than<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&gt;".parse_next(i)?;
        queue_newline_or_advance(&mut o, state, 1)?;
        queue(&mut o, style::Print('>'))
    }
}

fn ampersand<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&amp;".parse_next(i)?;
        queue_newline_or_advance(&mut o, state, 1)?;
        queue(&mut o, style::Print('&'))
    }
}

fn quot<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&quot;".parse_next(i)?;
        queue_newline_or_advance(&mut o, state, 1)?;
        queue(&mut o, style::Print('"'))
    }
}

fn line_ending<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        ascii::line_ending.parse_next(i)?;

        state.column = 0;
        state.set_newline = true;

        queue(&mut o, style::ResetColor)?;
        queue(&mut o, style::SetAttribute(style::Attribute::Reset))?;
        queue(&mut o, style::Print("\n"))
    }
}

fn fallback<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        let fallback = any.parse_next(i)?;
        if let Some(width) = fallback.width() {
            queue_newline_or_advance(&mut o, state, width)?;
            if fallback != ' ' || state.column != 1 {
                queue(&mut o, style::Print(fallback))?;
            }
        }

        Ok(())
    }
}

fn queue_newline_or_advance<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
    width: usize,
) -> Result<(), ErrMode<Error<'a>>> {
    if let Some(terminal_width) = state.terminal_width {
        if state.column > 0 && state.column + width > terminal_width {
            state.column = width;
            queue(&mut o, style::Print('\n'))?;
            return Ok(());
        }
    }

    // else
    state.column += width;

    Ok(())
}

fn queue<'a>(mut o: impl Write, command: impl Command) -> Result<(), ErrMode<Error<'a>>> {
    use crossterm::QueueableCommand;
    o.queue(command).map_err(|err| ErrMode::Cut(Error::Stdio(err)))?;
    Ok(())
}

fn codeblock_begin<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        if !state.newline {
            return Err(ErrMode::from_error_kind(i, ErrorKind::Fail));
        }

        // We don't want to do anything special to text inside codeblocks so we wait for all of it
        // The alternative is to switch between parse rules at the top level but that's slightly involved
        let language = preceded("```", till_line_ending).parse_next(i)?;
        ascii::line_ending.parse_next(i)?;

        state.in_codeblock = true;

        if !language.is_empty() {
            queue(&mut o, style::Print(format!("{}\n", language).bold()))?;
        }

        queue(&mut o, style::SetForegroundColor(CODE_COLOR))?;

        Ok(())
    }
}

fn codeblock_end<'a, 'b>(
    mut o: impl Write + 'b,
    state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "```".parse_next(i)?;
        state.in_codeblock = false;
        queue(&mut o, style::ResetColor)
    }
}

fn codeblock_less_than<'a, 'b>(
    mut o: impl Write + 'b,
    _state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&lt;".parse_next(i)?;
        queue(&mut o, style::Print('<'))
    }
}

fn codeblock_greater_than<'a, 'b>(
    mut o: impl Write + 'b,
    _state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&gt;".parse_next(i)?;
        queue(&mut o, style::Print('>'))
    }
}

fn codeblock_ampersand<'a, 'b>(
    mut o: impl Write + 'b,
    _state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&amp;".parse_next(i)?;
        queue(&mut o, style::Print('&'))
    }
}

fn codeblock_quot<'a, 'b>(
    mut o: impl Write + 'b,
    _state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        "&quot;".parse_next(i)?;
        queue(&mut o, style::Print('"'))
    }
}

fn codeblock_line_ending<'a, 'b>(
    mut o: impl Write + 'b,
    _state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        ascii::line_ending.parse_next(i)?;
        queue(&mut o, style::Print("\n"))
    }
}

fn codeblock_fallback<'a, 'b>(
    mut o: impl Write + 'b,
    _state: &'b mut ParseState,
) -> impl FnMut(&mut Partial<&'a str>) -> PResult<(), Error<'a>> + 'b {
    move |i| {
        let fallback = any.parse_next(i)?;
        queue(&mut o, style::Print(fallback))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use winnow::stream::Offset;

    use super::*;

    macro_rules! validate {
        ($test:ident, $input:literal, [$($commands:expr),+ $(,)?], $markdown_enabled:expr) => {
            #[test]
            fn $test() -> eyre::Result<()> {
                use crossterm::ExecutableCommand;

                let mut input = $input.trim().to_owned();
                input.push(' ');
                input.push(' ');

                let mut state = ParseState::new(Some(80), Some($markdown_enabled));
                let mut presult = vec![];
                let mut offset = 0;

                loop {
                    let input = Partial::new(&input[offset..]);
                    match interpret_markdown(input, &mut presult, &mut state) {
                        Ok(parsed) => {
                            offset += parsed.offset_from(&input);
                            state.newline = state.set_newline;
                            state.set_newline = false;
                        },
                        Err(err) => match err.into_inner() {
                            Some(err) => panic!("{err}"),
                            None => break, // Data was incomplete
                        },
                    }
                }

                presult.flush()?;
                let presult = String::from_utf8(presult)?;

                let mut wresult: Vec<u8> = vec![];
                $(wresult.execute($commands)?;)+
                let wresult = String::from_utf8(wresult)?;

                assert_eq!(presult.trim(), wresult);

                Ok(())
            }
        };

        ($test:ident, $input:literal, [$($commands:expr),+ $(,)?]) => {
            validate!($test, $input, [$($commands),+], false);
        };
    }

    validate!(text_1, "hello world!", [style::Print("hello world!")]);
    validate!(linted_codeblock_1, "```java\nhello world!```", [
        style::SetAttribute(Attribute::Bold),
        style::Print("java\n"),
        style::SetAttribute(Attribute::Reset),
        style::SetForegroundColor(CODE_COLOR),
        style::Print("hello world!"),
        style::ResetColor,
    ]);
    validate!(code_1, "`print`", [
        style::SetForegroundColor(CODE_COLOR),
        style::Print("print"),
        style::ResetColor,
    ]);
    validate!(url_1, "[google](google.com)", [
        style::SetForegroundColor(URL_TEXT_COLOR),
        style::Print("google "),
        style::SetForegroundColor(URL_LINK_COLOR),
        style::Print("google.com"),
        style::ResetColor,
    ]);
    validate!(citation_1, "[[1]](google.com)", [
        style::SetForegroundColor(URL_TEXT_COLOR),
        style::Print("[^1]"),
        style::ResetColor,
    ]);
    validate!(bold_1, "**hello**", [
        style::SetAttribute(Attribute::Bold),
        style::Print("hello"),
        style::SetAttribute(Attribute::NormalIntensity)
    ]);
    validate!(italic_1, "*hello*", [
        style::SetAttribute(Attribute::Italic),
        style::Print("hello"),
        style::SetAttribute(Attribute::NoItalic)
    ]);
    validate!(strikethrough_1, "~~hello~~", [
        style::SetAttribute(Attribute::CrossedOut),
        style::Print("hello"),
        style::SetAttribute(Attribute::NotCrossedOut)
    ]);
    validate!(less_than_1, "&lt;", [style::Print('<')]);
    validate!(greater_than_1, ".&gt;.", [style::Print(".>.")]);
    validate!(ampersand_1, "&amp;", [style::Print('&')]);
    validate!(quote_1, "&quot;", [style::Print('"')]);
    validate!(fallback_1, "+ % @ . ? ", [style::Print("+ % @ . ?")]);
    validate!(horizontal_rule_1, "---", [style::Print("━".repeat(80))]);
    validate!(heading_1, "# Hello World", [
        style::SetForegroundColor(HEADING_COLOR),
        style::SetAttribute(Attribute::Bold),
        style::Print("# Hello World"),
    ]);
    validate!(bulleted_item_1, "- bullet", [style::Print("• bullet")]);
    validate!(bulleted_item_2, "* bullet", [style::Print("• bullet")]);
    validate!(numbered_item_1, "1. number", [style::Print("1. number")]);
    validate!(blockquote_1, "> hello", [
        style::SetForegroundColor(BLOCKQUOTE_COLOR),
        style::Print("│ hello"),
    ]);
    validate!(square_bracket_1, "[test]", [style::Print("[test]")]);
    validate!(square_bracket_2, "Text with [brackets]", [style::Print(
        "Text with [brackets]"
    )]);
    validate!(square_bracket_empty, "[]", [style::Print("[]")]);
    validate!(square_bracket_array, "a[i]", [style::Print("a[i]")]);
    validate!(square_bracket_url_like_1, "[text] without url part", [style::Print(
        "[text] without url part"
    )]);
    validate!(square_bracket_url_like_2, "[text](without url part", [style::Print(
        "[text](without url part"
    )]);

    validate!(markdown_disabled_bold, "**hello**", [style::Print("**hello**")], true);
    validate!(markdown_disabled_italic, "*hello*", [style::Print("*hello*")], true);
    validate!(markdown_disabled_code, "`print`", [style::Print("`print`")], true);
    validate!(
        markdown_disabled_heading,
        "# Hello World",
        [style::Print("# Hello World")],
        true
    );
    validate!(markdown_disabled_bullet, "- bullet", [style::Print("- bullet")], true);
    validate!(markdown_disabled_number, "1. number", [style::Print("1. number")], true);
    validate!(markdown_disabled_blockquote, "> hello", [style::Print("> hello")], true);
    validate!(
        markdown_disabled_url,
        "[amazon](amazon.com)",
        [style::Print("[amazon](amazon.com)")],
        true
    );
    validate!(
        markdown_disabled_codeblock,
        "```java hello world!```",
        [style::Print("```java hello world!```")],
        true
    );
    validate!(
        markdown_disabled_text,
        "hello world!",
        [style::Print("hello world!")],
        true
    );
    validate!(
        markdown_disabled_line_ending,
        "line one\nline two",
        [
            style::Print("line one"),
            style::ResetColor,
            style::SetAttribute(style::Attribute::Reset),
            style::Print("\n"),
            style::Print("line two")
        ],
        true
    );
    validate!(
        markdown_disabled_fallback,
        "+ % @ . ?",
        [style::Print("+ % @ . ?")],
        true
    );
}
