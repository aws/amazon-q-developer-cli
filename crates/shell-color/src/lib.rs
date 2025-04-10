//! Parsing for terminal color

use std::fmt::Debug;

#[derive(Clone, PartialEq, Eq)]
pub struct SuggestionColor {
    pub fg: Option<VTermColor>,
    pub bg: Option<VTermColor>,
}

impl SuggestionColor {
    pub fn fg(&self) -> Option<VTermColor> {
        self.fg.clone()
    }

    pub fn bg(&self) -> Option<VTermColor> {
        self.bg.clone()
    }
}

impl Debug for SuggestionColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuggestionColor")
            .field("fg", &self.fg())
            .field("bg", &self.bg())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VTermColor {
    Rgb { red: u8, green: u8, blue: u8 },
    Indexed { idx: u8 },
}

impl VTermColor {
    const fn from_idx(idx: u8) -> Self {
        VTermColor::Indexed { idx }
    }

    const fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        VTermColor::Rgb { red, green, blue }
    }
}

impl From<nu_ansi_term::Color> for VTermColor {
    fn from(color: nu_ansi_term::Color) -> Self {
        use nu_ansi_term::Color;
        match color {
            Color::Black => VTermColor::from_idx(0),
            Color::Red => VTermColor::from_idx(1),
            Color::Green => VTermColor::from_idx(2),
            Color::Yellow => VTermColor::from_idx(3),
            Color::Blue => VTermColor::from_idx(4),
            Color::Purple => VTermColor::from_idx(5),
            Color::Magenta => VTermColor::from_idx(5),
            Color::Cyan => VTermColor::from_idx(6),
            Color::White => VTermColor::from_idx(7),
            Color::DarkGray => VTermColor::from_idx(8),
            Color::LightRed => VTermColor::from_idx(9),
            Color::LightGreen => VTermColor::from_idx(10),
            Color::LightYellow => VTermColor::from_idx(11),
            Color::LightBlue => VTermColor::from_idx(12),
            Color::LightPurple => VTermColor::from_idx(13),
            Color::LightMagenta => VTermColor::from_idx(13),
            Color::LightCyan => VTermColor::from_idx(14),
            Color::LightGray => VTermColor::from_idx(16),
            Color::Fixed(i) => VTermColor::from_idx(i),
            Color::Rgb(r, g, b) => VTermColor::from_rgb(r, g, b),
            Color::Default => VTermColor::from_idx(7),
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ColorSupport: u32 {
        const TERM256 = 1 << 1;
        const TERM24BIT = 1 << 2;
    }
}

#[allow(clippy::if_same_then_else)]
// Updates our idea of whether we support term256 and term24bit (see issue #10222).
pub fn get_color_support() -> ColorSupport {
    // Detect or infer term256 support. If fish_term256 is set, we respect it;
    // otherwise infer it from the TERM variable or use terminfo.
    let mut support_term256 = false;
    let mut support_term24bit = false;

    let term = std::env::var("TERM").ok();
    let fish_term256 = std::env::var("fish_term256").ok();

    if let Some(fish_term256) = fish_term256 {
        support_term256 = bool_from_string(&fish_term256);
    } else if term.is_some() && term.as_ref().unwrap().contains("256color") {
        support_term256 = true;
    } else if term.is_some() && term.as_ref().unwrap().contains("xterm") {
        // Assume that all 'xterm's can handle 256, except for Terminal.app from Snow Leopard
        let term_program = std::env::var("TERM_PROGRAM").ok();
        if term_program.is_some() && term_program.unwrap() == "Apple_Terminal" {
            let tpv = std::env::var("TERM_PROGRAM_VERSION").ok();
            if tpv.is_some() && tpv.unwrap().parse::<i32>().unwrap_or(0) > 299 {
                support_term256 = true;
            }
        } else {
            support_term256 = true;
        }
    }

    let ct = std::env::var("COLORTERM").ok();
    let it = std::env::var("ITERM_SESSION_ID").ok();
    let vte = std::env::var("VTE_VERSION").ok();
    let fish_term24bit = std::env::var("fish_term24bit").ok();
    // Handle $fish_term24bit
    if let Some(fish_term24bit) = fish_term24bit {
        support_term24bit = bool_from_string(&fish_term24bit);
    } else if std::env::var("STY").is_ok() || (term.is_some() && term.as_ref().unwrap().starts_with("eterm")) {
        // Screen and emacs' ansi-term swallow truecolor sequences,
        // so we ignore them unless force-enabled.
        support_term24bit = false;
    } else if let Some(ct) = ct {
        // If someone set $COLORTERM, that's the sort of color they want.
        if ct == "truecolor" || ct == "24bit" {
            support_term24bit = true;
        }
    } else if std::env::var("KONSOLE_VERSION").is_ok() || std::env::var("KONSOLE_PROFILE_NAME").is_ok() {
        // All konsole versions that use $KONSOLE_VERSION are new enough to support this,
        // so no check is necessary.
        support_term24bit = true;
    } else if it.is_some() {
        // Supporting versions of iTerm include a colon here.
        // We assume that if this is iTerm, it can't also be st, so having this check
        // inside is okay.
        if it.unwrap().contains(':') {
            support_term24bit = true;
        }
    } else if term.as_ref().is_some() && term.unwrap().starts_with("st-") {
        support_term24bit = true;
    } else if vte.is_some() && vte.unwrap().parse::<i32>().unwrap_or(0) > 3600 {
        support_term24bit = true;
    }

    let mut support = ColorSupport::empty();

    if support_term256 {
        support |= ColorSupport::TERM256;
    }

    if support_term24bit {
        support |= ColorSupport::TERM24BIT;
    }

    support
}

pub fn parse_suggestion_color_zsh_autosuggest(suggestion_str: &str, color_support: ColorSupport) -> SuggestionColor {
    let mut sc = SuggestionColor { fg: None, bg: None };

    for mut color_name in suggestion_str.split(',') {
        let is_fg = color_name.starts_with("fg=");
        let is_bg = color_name.starts_with("bg=");
        if is_fg || is_bg {
            (_, color_name) = color_name.split_at(3);
            // TODO: currently using fish's parsing logic for named colors.
            // This can fail in two cases:
            // 1. false positives from fish colors that aren't supported in zsh (e.g. brblack)
            // 2. false negatives that aren't supported in fish (e.g. abbreviations like bl for black)
            // note: this todo was in the old c code - maybe it isn't needed anymore?
            let mut color = try_parse_named(color_name);
            if color.is_none() && color_name.starts_with('#') {
                color = try_parse_rgb(color_name);
            }
            if color.is_none() {
                // custom zsh logic - try 256 indexed colors first.
                let index = color_name.parse::<u64>().unwrap_or(0);
                let index_supported = if color_support.is_empty() {
                    index < 16
                } else {
                    index < 256
                };
                if index_supported {
                    let vc = VTermColor::Indexed { idx: index as u8 };
                    if is_fg {
                        sc.fg = Some(vc);
                    } else {
                        sc.bg = Some(vc);
                    }
                }
            } else {
                let vc = color_to_vterm_color(color, color_support);
                if is_fg {
                    sc.fg = vc;
                } else {
                    sc.bg = vc;
                }
            }
        }
    }

    sc
}

pub fn parse_suggestion_color_fish(suggestion_str: &str, color_support: ColorSupport) -> Option<SuggestionColor> {
    let c = parse_fish_color_from_string(suggestion_str, color_support);
    let vc = color_to_vterm_color(c, color_support)?;
    Some(SuggestionColor { fg: Some(vc), bg: None })
}

pub fn parse_hint_color_nu(suggestion_str: impl AsRef<str>) -> SuggestionColor {
    let color = nu_color_config::lookup_ansi_color_style(suggestion_str.as_ref());
    SuggestionColor {
        fg: color.foreground.map(VTermColor::from),
        bg: color.background.map(VTermColor::from),
    }
}

#[derive(PartialEq, Eq, Debug)]
#[repr(u8)]
enum ColorType {
    Named = 1,
    Rgb   = 2,
}

#[derive(Debug, PartialEq, Eq)]
struct Color {
    kind: ColorType,
    name_idx: u8,
    rgb: [u8; 3],
}

fn bool_from_string(x: &str) -> bool {
    match x.chars().next() {
        Some(first) => "YTyt1".contains(first),
        None => false,
    }
}

const fn squared_difference(p1: i64, p2: i64) -> u64 {
    let diff = (p1 - p2).unsigned_abs();
    diff * diff
}

const fn convert_color(rgb: [u8; 3], colors: &[u32]) -> u8 {
    let r = rgb[0] as i64;
    let g = rgb[1] as i64;
    let b = rgb[2] as i64;

    let mut best_distance = u64::MAX;
    let mut best_index = u8::MAX;

    let mut i = 0;
    while i < colors.len() {
        let color = colors[i];
        let test_r = ((color >> 16) & 0xff) as i64;
        let test_g = ((color >> 8) & 0xff) as i64;
        let test_b = (color & 0xff) as i64;
        let distance = squared_difference(r, test_r) + squared_difference(g, test_g) + squared_difference(b, test_b);
        if distance <= best_distance {
            best_index = i as u8;
            best_distance = distance;
        }
        i += 1;
    }

    best_index
}

/// We support the following style of rgb formats (case insensitive):
/// `#FA3`, `#F3A035`, `FA3`, `F3A035`
fn try_parse_rgb(name: &str) -> Option<Color> {
    // Skip any leading #.
    let name = match name.strip_prefix('#') {
        Some(name) => name,
        None => name,
    };

    let mut color = Color {
        kind: ColorType::Rgb,
        name_idx: 0,
        rgb: [0, 0, 0],
    };

    let name = name.as_bytes();

    match name.len() {
        // Format: FA3
        3 => {
            for (i, c) in name.iter().enumerate().take(3) {
                let val = char::from(*c).to_digit(16)? as u8;
                color.rgb[i] = val * 16 + val;
            }
            Some(color)
        },
        // Format: F3A035
        6 => {
            for i in 0..3 {
                let val_hi = char::from(name[i * 2]).to_digit(16)? as u8;
                let val_low = char::from(name[i * 2 + 1]).to_digit(16)? as u8;
                color.rgb[i] = val_hi * 16 + val_low;
            }
            Some(color)
        },
        _ => None,
    }
}

struct NamedColor {
    name: &'static str,
    idx: u8,
    _rgb: [u8; 3],
}

macro_rules! decl_named_colors {
    ($({$name: expr, $idx: expr, { $r: expr, $g: expr, $b: expr }}),*,) => {
        &[
            $(
                NamedColor {
                    name: $name,
                    idx: $idx,
                    _rgb: [$r, $g, $b],
                },
            )*
        ]
    };
}

// Keep this sorted alphabetically
static NAMED_COLORS: &[NamedColor] = decl_named_colors! {
    {"black", 0, {0x00, 0x00, 0x00}},      {"blue", 4, {0x00, 0x00, 0x80}},
    {"brblack", 8, {0x80, 0x80, 0x80}},    {"brblue", 12, {0x00, 0x00, 0xFF}},
    {"brbrown", 11, {0xFF, 0xFF, 0x00}},    {"brcyan", 14, {0x00, 0xFF, 0xFF}},
    {"brgreen", 10, {0x00, 0xFF, 0x00}},   {"brgrey", 8, {0x55, 0x55, 0x55}},
    {"brmagenta", 13, {0xFF, 0x00, 0xFF}}, {"brown", 3, {0x72, 0x50, 0x00}},
    {"brpurple", 13, {0xFF, 0x00, 0xFF}},   {"brred", 9, {0xFF, 0x00, 0x00}},
    {"brwhite", 15, {0xFF, 0xFF, 0xFF}},   {"bryellow", 11, {0xFF, 0xFF, 0x00}},
    {"cyan", 6, {0x00, 0x80, 0x80}},       {"green", 2, {0x00, 0x80, 0x00}},
    {"grey", 7, {0xE5, 0xE5, 0xE5}},        {"magenta", 5, {0x80, 0x00, 0x80}},
    {"purple", 5, {0x80, 0x00, 0x80}},      {"red", 1, {0x80, 0x00, 0x00}},
    {"white", 7, {0xC0, 0xC0, 0xC0}},      {"yellow", 3, {0x80, 0x80, 0x00}},
};

fn try_parse_named(s: &str) -> Option<Color> {
    let idx_res = NAMED_COLORS.binary_search_by(|elem| elem.name.cmp(&s.to_ascii_lowercase()));
    if let Ok(idx) = idx_res {
        return Some(Color {
            kind: ColorType::Named,
            name_idx: NAMED_COLORS[idx].idx,
            rgb: [0, 0, 0],
        });
    }
    None
}

const fn term16_color_for_rgb(rgb: [u8; 3]) -> u8 {
    const K_COLORS: &[u32] = &[
        0x000000, // Black
        0x800000, // Red
        0x008000, // Green
        0x808000, // Yellow
        0x000080, // Blue
        0x800080, // Magenta
        0x008080, // Cyan
        0xc0c0c0, // White
        0x808080, // Bright Black
        0xff0000, // Bright Red
        0x00ff00, // Bright Green
        0xffff00, // Bright Yellow
        0x0000ff, // Bright Blue
        0xff00ff, // Bright Magenta
        0x00ffff, // Bright Cyan
        0xffffff, // Bright White
    ];
    convert_color(rgb, K_COLORS)
}

const fn term256_color_for_rgb(rgb: [u8; 3]) -> u8 {
    const K_COLORS: &[u32] = &[
        0x000000, 0x00005f, 0x000087, 0x0000af, 0x0000d7, 0x0000ff, 0x005f00, 0x005f5f, 0x005f87, 0x005faf, 0x005fd7,
        0x005fff, 0x008700, 0x00875f, 0x008787, 0x0087af, 0x0087d7, 0x0087ff, 0x00af00, 0x00af5f, 0x00af87, 0x00afaf,
        0x00afd7, 0x00afff, 0x00d700, 0x00d75f, 0x00d787, 0x00d7af, 0x00d7d7, 0x00d7ff, 0x00ff00, 0x00ff5f, 0x00ff87,
        0x00ffaf, 0x00ffd7, 0x00ffff, 0x5f0000, 0x5f005f, 0x5f0087, 0x5f00af, 0x5f00d7, 0x5f00ff, 0x5f5f00, 0x5f5f5f,
        0x5f5f87, 0x5f5faf, 0x5f5fd7, 0x5f5fff, 0x5f8700, 0x5f875f, 0x5f8787, 0x5f87af, 0x5f87d7, 0x5f87ff, 0x5faf00,
        0x5faf5f, 0x5faf87, 0x5fafaf, 0x5fafd7, 0x5fafff, 0x5fd700, 0x5fd75f, 0x5fd787, 0x5fd7af, 0x5fd7d7, 0x5fd7ff,
        0x5fff00, 0x5fff5f, 0x5fff87, 0x5fffaf, 0x5fffd7, 0x5fffff, 0x870000, 0x87005f, 0x870087, 0x8700af, 0x8700d7,
        0x8700ff, 0x875f00, 0x875f5f, 0x875f87, 0x875faf, 0x875fd7, 0x875fff, 0x878700, 0x87875f, 0x878787, 0x8787af,
        0x8787d7, 0x8787ff, 0x87af00, 0x87af5f, 0x87af87, 0x87afaf, 0x87afd7, 0x87afff, 0x87d700, 0x87d75f, 0x87d787,
        0x87d7af, 0x87d7d7, 0x87d7ff, 0x87ff00, 0x87ff5f, 0x87ff87, 0x87ffaf, 0x87ffd7, 0x87ffff, 0xaf0000, 0xaf005f,
        0xaf0087, 0xaf00af, 0xaf00d7, 0xaf00ff, 0xaf5f00, 0xaf5f5f, 0xaf5f87, 0xaf5faf, 0xaf5fd7, 0xaf5fff, 0xaf8700,
        0xaf875f, 0xaf8787, 0xaf87af, 0xaf87d7, 0xaf87ff, 0xafaf00, 0xafaf5f, 0xafaf87, 0xafafaf, 0xafafd7, 0xafafff,
        0xafd700, 0xafd75f, 0xafd787, 0xafd7af, 0xafd7d7, 0xafd7ff, 0xafff00, 0xafff5f, 0xafff87, 0xafffaf, 0xafffd7,
        0xafffff, 0xd70000, 0xd7005f, 0xd70087, 0xd700af, 0xd700d7, 0xd700ff, 0xd75f00, 0xd75f5f, 0xd75f87, 0xd75faf,
        0xd75fd7, 0xd75fff, 0xd78700, 0xd7875f, 0xd78787, 0xd787af, 0xd787d7, 0xd787ff, 0xd7af00, 0xd7af5f, 0xd7af87,
        0xd7afaf, 0xd7afd7, 0xd7afff, 0xd7d700, 0xd7d75f, 0xd7d787, 0xd7d7af, 0xd7d7d7, 0xd7d7ff, 0xd7ff00, 0xd7ff5f,
        0xd7ff87, 0xd7ffaf, 0xd7ffd7, 0xd7ffff, 0xff0000, 0xff005f, 0xff0087, 0xff00af, 0xff00d7, 0xff00ff, 0xff5f00,
        0xff5f5f, 0xff5f87, 0xff5faf, 0xff5fd7, 0xff5fff, 0xff8700, 0xff875f, 0xff8787, 0xff87af, 0xff87d7, 0xff87ff,
        0xffaf00, 0xffaf5f, 0xffaf87, 0xffafaf, 0xffafd7, 0xffafff, 0xffd700, 0xffd75f, 0xffd787, 0xffd7af, 0xffd7d7,
        0xffd7ff, 0xffff00, 0xffff5f, 0xffff87, 0xffffaf, 0xffffd7, 0xffffff, 0x080808, 0x121212, 0x1c1c1c, 0x262626,
        0x303030, 0x3a3a3a, 0x444444, 0x4e4e4e, 0x585858, 0x626262, 0x6c6c6c, 0x767676, 0x808080, 0x8a8a8a, 0x949494,
        0x9e9e9e, 0xa8a8a8, 0xb2b2b2, 0xbcbcbc, 0xc6c6c6, 0xd0d0d0, 0xdadada, 0xe4e4e4, 0xeeeeee,
    ];
    16 + convert_color(rgb, K_COLORS)
}

fn parse_fish_color_from_string(s: &str, color_support: ColorSupport) -> Option<Color> {
    let mut first_rgb = None;
    let mut first_named = None;

    for color_name in s.split([' ', '\t']) {
        if !color_name.starts_with('-') {
            let mut color = try_parse_named(color_name);
            if color.is_none() {
                color = try_parse_rgb(color_name);
            }
            if let Some(color) = color {
                if first_rgb.is_none() && color.kind == ColorType::Rgb {
                    first_rgb = Some(color);
                } else if first_named.is_none() && color.kind == ColorType::Named {
                    first_named = Some(color);
                }
            }
        }
    }

    if (first_rgb.is_some() && color_support.contains(ColorSupport::TERM24BIT)) || first_named.is_none() {
        return first_rgb;
    }

    first_named
}

fn color_to_vterm_color(c: Option<Color>, color_support: ColorSupport) -> Option<VTermColor> {
    let c = c?;
    if c.kind == ColorType::Rgb {
        if color_support.contains(ColorSupport::TERM24BIT) {
            Some(VTermColor::from_rgb(c.rgb[0], c.rgb[1], c.rgb[2]))
        } else if color_support.contains(ColorSupport::TERM256) {
            Some(VTermColor::from_idx(term256_color_for_rgb(c.rgb)))
        } else {
            Some(VTermColor::from_idx(term16_color_for_rgb(c.rgb)))
        }
    } else {
        Some(VTermColor::from_idx(c.name_idx))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[fig_test::test]
    fn color_support() {
        // make sure it doesn't panic
        get_color_support();

        for (key, _) in std::env::vars() {
            std::env::remove_var(key);
        }

        let assert_supports = |vars: &[(&str, &str)], expected: ColorSupport| {
            for (key, value) in vars {
                std::env::set_var(key, value);
            }

            assert_eq!(get_color_support(), expected);

            for (key, _) in vars {
                std::env::remove_var(key);
            }
        };

        // no env
        assert_supports(&[], ColorSupport::empty());

        // TERM256
        // fish_term256
        assert_supports(&[("fish_term256", "y")], ColorSupport::TERM256);
        assert_supports(&[("fish_term256", "n")], ColorSupport::empty());
        // TERM=*256color*
        assert_supports(&[("TERM", "foo_256color_bar")], ColorSupport::TERM256);
        // xterm
        assert_supports(&[("TERM", "xterm")], ColorSupport::TERM256);
        // recent Terminal.app
        assert_supports(
            &[
                ("TERM", "xterm"),
                ("TERM_PROGRAM", "Apple_Terminal"),
                ("TERM_PROGRAM_VERSION", "300"),
            ],
            ColorSupport::TERM256,
        );
        // old Terminal.app
        assert_supports(
            &[
                ("TERM", "xterm"),
                ("TERM_PROGRAM", "Apple_Terminal"),
                ("TERM_PROGRAM_VERSION", "200"),
            ],
            ColorSupport::empty(),
        );

        // TERM24BIT
        // fish_term24bit
        assert_supports(&[("fish_term24bit", "y")], ColorSupport::TERM24BIT);
        assert_supports(&[("fish_term24bit", "n")], ColorSupport::empty());
        // screen/emacs
        assert_supports(&[("TERM", "eterm"), ("STY", "foo")], ColorSupport::empty());
        // colorterm
        assert_supports(&[("COLORTERM", "truecolor")], ColorSupport::TERM24BIT);
        assert_supports(&[("COLORTERM", "24bit")], ColorSupport::TERM24BIT);
        assert_supports(&[("COLORTERM", "foo")], ColorSupport::empty());
        // konsole
        assert_supports(&[("KONSOLE_VERSION", "foo")], ColorSupport::TERM24BIT);
        // iterm
        assert_supports(&[("ITERM_SESSION_ID", "1:2")], ColorSupport::TERM24BIT);
        // st
        assert_supports(&[("TERM", "st-foo")], ColorSupport::TERM24BIT);
        // vte
        assert_supports(&[("VTE_VERSION", "3500")], ColorSupport::empty());
        assert_supports(&[("VTE_VERSION", "3700")], ColorSupport::TERM24BIT);
    }

    #[test]
    fn assert_named_colors_sort() {
        NAMED_COLORS
            .windows(2)
            .for_each(|elems| assert!(elems[0].name.cmp(elems[1].name).is_lt()));
    }

    #[test]
    fn parse_color() {
        // parse_rgb
        // Should parse
        assert!(try_parse_rgb("#ffffff").is_some());
        assert!(try_parse_rgb("#000000").is_some());
        assert!(try_parse_rgb("#ababab").is_some());
        assert!(try_parse_rgb("000000").is_some());
        assert!(try_parse_rgb("ffffff").is_some());
        assert!(try_parse_rgb("abcabc").is_some());
        assert!(try_parse_rgb("#123").is_some());
        assert!(try_parse_rgb("#fff").is_some());
        assert!(try_parse_rgb("abc").is_some());
        assert!(try_parse_rgb("123").is_some());
        assert!(try_parse_rgb("fff").is_some());
        assert!(try_parse_rgb("000").is_some());

        // Should not parse
        assert!(try_parse_rgb("#xyz").is_none());
        assert!(try_parse_rgb("12").is_none());
        assert!(try_parse_rgb("abcdeh").is_none());
        assert!(try_parse_rgb("#ffff").is_none());
        assert!(try_parse_rgb("12345").is_none());
        assert!(try_parse_rgb("1234567").is_none());

        // parse_named
        // Should parse
        assert!(try_parse_named("blue").is_some());
        assert!(try_parse_named("white").is_some());
        assert!(try_parse_named("yellow").is_some());
        assert!(try_parse_named("brblack").is_some());
        assert!(try_parse_named("BrBlue").is_some());
        assert!(try_parse_named("bRYelLow").is_some());

        // Should not parse
        assert!(try_parse_named("aaa").is_none());
        assert!(try_parse_named("blu").is_none());
        assert!(try_parse_named("other").is_none());
    }

    #[test]
    fn parse_fish_autosuggest() {
        assert_eq!(
            parse_fish_color_from_string("cyan", ColorSupport::TERM256),
            Some(Color {
                kind: ColorType::Named,
                name_idx: 6,
                rgb: [0, 0, 0]
            })
        );
        assert_eq!(
            parse_fish_color_from_string("#123", ColorSupport::TERM256),
            Some(Color {
                kind: ColorType::Rgb,
                name_idx: 0,
                rgb: [0x11, 0x22, 0x33]
            })
        );
        assert_eq!(
            parse_fish_color_from_string("-ignore\t-white\t-#123\tcyan", ColorSupport::TERM256),
            Some(Color {
                kind: ColorType::Named,
                name_idx: 6,
                rgb: [0, 0, 0]
            })
        );
        assert_eq!(
            parse_fish_color_from_string("555 brblack", ColorSupport::TERM256),
            Some(Color {
                kind: ColorType::Named,
                name_idx: 8,
                rgb: [0, 0, 0]
            })
        );
        assert_eq!(
            parse_fish_color_from_string("555 brblack", ColorSupport::TERM24BIT),
            Some(Color {
                kind: ColorType::Rgb,
                name_idx: 0,
                rgb: [0x55, 0x55, 0x55]
            })
        );
        assert_eq!(
            parse_fish_color_from_string("-ignore -all", ColorSupport::TERM256),
            None
        );
    }

    #[test]
    fn parse_zsh_autosuggest() {
        assert_eq!(
            // color support supports rgb
            parse_suggestion_color_zsh_autosuggest("fg=#123,bg=#456", ColorSupport::TERM24BIT),
            SuggestionColor {
                fg: Some(VTermColor::from_rgb(0x11, 0x22, 0x33)),
                bg: Some(VTermColor::from_rgb(0x44, 0x55, 0x66)),
            }
        );
        assert_eq!(
            // color support doesn't support rgb
            parse_suggestion_color_zsh_autosuggest("fg=#123,bg=#456", ColorSupport::empty()),
            SuggestionColor {
                fg: Some(VTermColor::from_idx(0)),
                bg: Some(VTermColor::from_idx(8)),
            }
        );
        assert_eq!(
            // default
            parse_suggestion_color_zsh_autosuggest("fg=8", ColorSupport::empty()),
            SuggestionColor {
                fg: Some(VTermColor::from_idx(8)),
                bg: None,
            }
        );
        assert_eq!(
            // ignore and recover from invalid data
            parse_suggestion_color_zsh_autosuggest("invalid=!,,=,bg=cyan", ColorSupport::empty()),
            SuggestionColor {
                fg: None,
                bg: Some(VTermColor::from_idx(6))
            }
        );
    }
}
