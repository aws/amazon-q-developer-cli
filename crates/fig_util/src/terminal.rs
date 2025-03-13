use std::borrow::Cow;
use std::fmt;
use std::sync::OnceLock;

use fig_os_shim::Context;
use serde::{
    Deserialize,
    Serialize,
};

/// Terminals that macOS supports
pub const MACOS_TERMINALS: &[Terminal] = &[
    Terminal::Alacritty,
    Terminal::Iterm,
    Terminal::Kitty,
    Terminal::Tabby,
    Terminal::TerminalApp,
    Terminal::VSCodeInsiders,
    Terminal::VSCode,
    Terminal::VSCodium,
    Terminal::WezTerm,
    Terminal::Zed,
    Terminal::Cursor,
    Terminal::CursorNightly,
    Terminal::Rio,
    Terminal::Windsurf,
    Terminal::WindsurfNext,
    Terminal::Ghostty,
    Terminal::Positron,
    Terminal::Trae,
];

/// Terminals that Linux supports
pub const LINUX_TERMINALS: &[Terminal] = &[
    Terminal::Alacritty,
    Terminal::Kitty,
    Terminal::GnomeConsole,
    Terminal::GnomeTerminal,
    Terminal::Guake,
    Terminal::Hyper,
    Terminal::Konsole,
    Terminal::XfceTerminal,
    Terminal::WezTerm,
    Terminal::Tilix,
    Terminal::Terminator,
    Terminal::VSCode,
    Terminal::VSCodeInsiders,
    Terminal::VSCodium,
    Terminal::IntelliJ(None),
    Terminal::Positron,
];

/// Other terminals that figterm should launch within that are not full terminal emulators
pub const SPECIAL_TERMINALS: &[Terminal] = &[
    Terminal::Ssh,
    Terminal::Tmux,
    Terminal::Nvim,
    Terminal::Vim,
    Terminal::Zellij,
];

pub fn current_terminal() -> Option<&'static Terminal> {
    static CURRENT_TERMINAL: OnceLock<Option<Terminal>> = OnceLock::new();
    CURRENT_TERMINAL
        .get_or_init(|| Terminal::parent_terminal(&Context::new()))
        .as_ref()
}

pub fn current_terminal_version() -> Option<&'static str> {
    static CURRENT_TERMINAL_VERSION: OnceLock<Option<String>> = OnceLock::new();
    CURRENT_TERMINAL_VERSION.get_or_init(Terminal::version).as_deref()
}

/// Checks if the current process is inside of one of the pseudoterminals listed under
/// [`SPECIAL_TERMINALS`], returning the terminal if true.
pub fn in_special_terminal(ctx: &Context) -> Option<Terminal> {
    Terminal::from_process_info(ctx, &SPECIAL_TERMINALS.to_vec())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomTerminalMacos {
    /// The macOS bundle ID
    pub bundle_id: Option<String>,

    #[serde(default)]
    pub input_method: bool,
    #[serde(default)]
    pub accessibility: bool,
    #[serde(default)]
    pub xterm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomTerminal {
    pub id: String,
    pub name: String,
    pub macos: CustomTerminalMacos,
}

/// All supported terminals
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Terminal {
    /// iTerm 2
    Iterm,
    /// Native macOS terminal
    TerminalApp,
    /// Hyper terminal
    Hyper,
    /// Alacritty terminal
    Alacritty,
    /// Kitty terminal
    Kitty,
    /// VSCode terminal
    VSCode,
    /// VSCode Insiders
    VSCodeInsiders,
    /// VSCodium
    VSCodium,
    /// Tabby
    Tabby,
    /// Nova
    Nova,
    /// Wezterm
    WezTerm,
    /// Gnome Console
    GnomeConsole,
    /// Gnome Terminal
    GnomeTerminal,
    /// KDE Konsole
    Konsole,
    /// Tilix
    Tilix,
    /// Xfce Terminal
    XfceTerminal,
    /// Terminator
    Terminator,
    /// Terminology
    Terminology,
    /// IntelliJ
    IntelliJ(Option<IntelliJVariant>),
    // Zed
    Zed,
    /// Cursor
    Cursor,
    /// Cursor Nightly
    CursorNightly,
    /// Rio <https://github.com/raphamorim/rio>
    Rio,
    /// Guake
    Guake,

    // Other pseudoterminal that we want to launch within
    /// SSH
    Ssh,
    /// Tmux
    Tmux,
    /// Vim
    Vim,
    /// Nvim
    Nvim,
    /// Zellij
    Zellij,
    /// Windsurf
    Windsurf,
    /// Windsurf Next
    WindsurfNext,
    /// Ghostty
    Ghostty,
    /// Positron
    Positron,
    /// Trae
    Trae,

    /// Custom terminal to support user/custom entries
    Custom(CustomTerminal),
}

impl fmt::Display for Terminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminal::Iterm => write!(f, "iTerm 2"),
            Terminal::TerminalApp => write!(f, "macOS"),
            Terminal::Hyper => write!(f, "Hyper"),
            Terminal::Alacritty => write!(f, "Alacritty"),
            Terminal::Kitty => write!(f, "Kitty"),
            Terminal::VSCode => write!(f, "VSCode"),
            Terminal::VSCodeInsiders => write!(f, "VSCode Insiders"),
            Terminal::VSCodium => write!(f, "VSCodium"),
            Terminal::Tabby => write!(f, "Tabby"),
            Terminal::Nova => write!(f, "Nova"),
            Terminal::WezTerm => write!(f, "Wezterm"),
            Terminal::GnomeConsole => write!(f, "Gnome Console"),
            Terminal::GnomeTerminal => write!(f, "Gnome Terminal"),
            Terminal::Konsole => write!(f, "Konsole"),
            Terminal::Tilix => write!(f, "Tilix"),
            Terminal::XfceTerminal => write!(f, "Xfce Terminal"),
            Terminal::Terminator => write!(f, "Terminator"),
            Terminal::Terminology => write!(f, "Terminology"),
            Terminal::Ssh => write!(f, "SSH"),
            Terminal::Tmux => write!(f, "Tmux"),
            Terminal::Vim => write!(f, "Vim"),
            Terminal::Nvim => write!(f, "Nvim"),
            Terminal::Zellij => write!(f, "Zellij"),
            Terminal::IntelliJ(Some(variant)) => write!(f, "{}", variant.application_name()),
            Terminal::IntelliJ(None) => write!(f, "IntelliJ"),
            Terminal::Zed => write!(f, "Zed"),
            Terminal::Cursor => write!(f, "Cursor"),
            Terminal::CursorNightly => write!(f, "Cursor Nightly"),
            Terminal::Rio => write!(f, "Rio"),
            Terminal::Windsurf => write!(f, "Windsurf"),
            Terminal::WindsurfNext => write!(f, "Windsurf Next"),
            Terminal::Guake => write!(f, "Guake"),
            Terminal::Ghostty => write!(f, "Ghostty"),
            Terminal::Positron => write!(f, "Positron"),
            Terminal::Trae => write!(f, "Trae"),
            Terminal::Custom(custom_terminal) => write!(f, "{}", custom_terminal.name),
        }
    }
}

impl Terminal {
    /// Attempts to return the suspected terminal emulator for the current process.
    ///
    /// Note that "special" pseudoterminals like tmux or ssh will not be returned.
    pub fn parent_terminal(ctx: &Context) -> Option<Self> {
        let env = ctx.env();

        #[cfg(target_os = "macos")]
        {
            if let Ok(bundle_id) = env.get("__CFBundleIdentifier") {
                if let Some(term) = Self::from_bundle_id(bundle_id) {
                    return Some(term);
                }
            }
        }

        match env.get("TERM_PROGRAM").ok().as_deref() {
            Some("iTerm.app") => return Some(Terminal::Iterm),
            Some("Apple_Terminal") => return Some(Terminal::TerminalApp),
            Some("Hyper") => return Some(Terminal::Hyper),
            Some("vscode") => match std::env::var("TERM_PROGRAM_VERSION").ok().as_deref() {
                Some(v) if v.contains("insiders") => return Some(Terminal::VSCodeInsiders),
                _ => return Some(Terminal::VSCode),
            },
            Some("Tabby") => return Some(Terminal::Tabby),
            Some("Nova") => return Some(Terminal::Nova),
            Some("WezTerm") => return Some(Terminal::WezTerm),
            Some("guake") => return Some(Terminal::Guake),
            Some("ghostty") => return Some(Terminal::Ghostty),
            _ => (),
        };

        let terminals = match ctx.platform().os() {
            fig_os_shim::Os::Mac => MACOS_TERMINALS,
            fig_os_shim::Os::Linux => LINUX_TERMINALS,
            _ => return None,
        };
        Self::from_process_info(ctx, &terminals.to_vec())
    }

    /// Attempts to return the suspected terminal emulator for the current process according to the
    /// process hierarchy. Only the list provided in `terminals` will be searched for.
    pub fn from_process_info(ctx: &Context, terminals: &Vec<Terminal>) -> Option<Self> {
        let mut option_pid = Some(Box::new(ctx.process_info().current_pid()));
        let (mut curr_depth, max_depth) = (0, 5);
        while curr_depth < max_depth {
            if let Some(pid) = option_pid {
                if let Some(exe) = pid.exe() {
                    if let Some(name) = exe.file_name().and_then(|s| s.to_str()) {
                        for terminal in terminals {
                            if terminal.executable_names().contains(&name) {
                                return Some(terminal.clone());
                            }
                        }
                    }
                }
                if let Some(cmdline) = pid.cmdline() {
                    if let Some(terminal) = Self::try_from_cmdline(&cmdline, terminals) {
                        return Some(terminal.clone());
                    }
                }
                option_pid = pid.parent();
                curr_depth += 1;
            } else {
                break;
            }
        }
        None
    }

    /// Attempts to find the suspected terminal according to the provided `cmdline` - ie,
    /// the value from /proc/pid/cmdline except with null bytes replaced with space.
    ///
    /// Only the list provided in `terminals` will be searched for.
    pub fn try_from_cmdline(cmdline: &str, terminals: &Vec<Terminal>) -> Option<Self> {
        // Special cases for terminals that launch as a script, e.g.
        // `/usr/bin/python3 /usr/bin/terminator`
        let second_arg_terms = &[Terminal::Terminator, Terminal::Guake];
        if second_arg_terms.iter().any(|t| terminals.contains(t)) {
            let second_arg_name = cmdline
                .split(' ')
                .skip(1)
                .take(1)
                .next()
                .and_then(|cmd| cmd.split('/').last());
            if let Some(second_arg_name) = second_arg_name {
                if let Some(term) = second_arg_terms
                    .iter()
                    .find(|t| t.executable_names().contains(&second_arg_name))
                {
                    return Some(term.clone());
                }
            }
        }

        // Default logic that checks the final path element of the first argument.
        let first_arg_name = cmdline
            .split(' ')
            .take(1)
            .next()
            .and_then(|cmd| cmd.split('/').last())
            .map(str::to_string);
        if let Some(first_arg_name) = first_arg_name {
            for terminal in terminals {
                if terminal.executable_names().contains(&first_arg_name.as_str()) {
                    return Some(terminal.clone());
                }
            }
        }

        None
    }

    pub fn version() -> Option<String> {
        static RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
        let re = RE.get_or_init(|| regex::Regex::new("[0-9\\-\\._]+").ok()).as_ref()?;
        let version = std::env::var("TERM_PROGRAM_VERSION").ok()?;
        match re.captures(&version).is_some() {
            true => Some(version),
            false => None,
        }
    }

    pub fn internal_id(&self) -> Cow<'static, str> {
        match self {
            Terminal::Iterm => "iterm".into(),
            Terminal::TerminalApp => "terminal".into(),
            Terminal::Hyper => "hyper".into(),
            Terminal::Alacritty => "alacritty".into(),
            Terminal::Kitty => "kitty".into(),
            Terminal::VSCode => "vscode".into(),
            Terminal::VSCodeInsiders => "vscode-insiders".into(),
            Terminal::VSCodium => "vscodium".into(),
            Terminal::Tabby => "tabby".into(),
            Terminal::Nova => "nova".into(),
            Terminal::WezTerm => "wezterm".into(),
            Terminal::GnomeTerminal => "gnome-terminal".into(),
            Terminal::GnomeConsole => "gnome-console".into(),
            Terminal::Konsole => "konsole".into(),
            Terminal::Tilix => "tilix".into(),
            Terminal::XfceTerminal => "xfce-terminal".into(),
            Terminal::Terminator => "terminator".into(),
            Terminal::Terminology => "terminology".into(),
            Terminal::Ssh => "ssh".into(),
            Terminal::Tmux => "tmux".into(),
            Terminal::Vim => "vim".into(),
            Terminal::Nvim => "nvim".into(),
            Terminal::Zellij => "zellij".into(),
            Terminal::Zed => "zed".into(),
            Terminal::IntelliJ(ide) => match ide {
                Some(variant) => format!("intellij-{}", variant.internal_id()).into(),
                None => "intellij".into(),
            },
            Terminal::Cursor => "cursor".into(),
            Terminal::CursorNightly => "cursor-nightly".into(),
            Terminal::Rio => "rio".into(),
            Terminal::Windsurf => "windsurf".into(),
            Terminal::WindsurfNext => "windsurf-next".into(),
            Terminal::Guake => "guake".into(),
            Terminal::Ghostty => "ghostty".into(),
            Terminal::Positron => "positron".into(),
            Terminal::Trae => "trae".into(),
            Terminal::Custom(custom_terminal) => custom_terminal.id.clone().into(),
        }
    }

    /// Get the bundle identifier for the terminal
    /// Note: this does not gracefully handle terminals that have changed bundle identifiers
    /// recently such as VSCodium & Alacritty. We default to the current identifier.
    pub fn to_bundle_id(&self) -> Option<Cow<'static, str>> {
        match self {
            Terminal::Iterm => Some("com.googlecode.iterm2".into()),
            Terminal::TerminalApp => Some("com.apple.Terminal".into()),
            Terminal::Hyper => Some("co.zeit.hyper".into()),
            Terminal::Alacritty => Some("org.alacritty".into()),
            Terminal::Kitty => Some("net.kovidgoyal.kitty".into()),
            Terminal::VSCode => Some("com.microsoft.VSCode".into()),
            Terminal::VSCodeInsiders => Some("com.microsoft.VSCodeInsiders".into()),
            Terminal::VSCodium => Some("com.vscodium".into()),
            Terminal::Tabby => Some("org.tabby".into()),
            Terminal::Nova => Some("com.panic.Nova".into()),
            Terminal::WezTerm => Some("com.github.wez.wezterm".into()),
            Terminal::IntelliJ(Some(variant)) => Some(variant.bundle_identifier().into()),
            Terminal::Zed => Some("dev.zed.Zed".into()),
            Terminal::Cursor => Some("com.todesktop.230313mzl4w4u92".into()),
            Terminal::CursorNightly => Some("com.todesktop.23052492jqa5xjo".into()),
            Terminal::Rio => Some("com.raphaelamorim.rio".into()),
            Terminal::Windsurf => Some("com.exafunction.windsurf".into()),
            Terminal::WindsurfNext => Some("com.exafunction.windsurf-next".into()),
            Terminal::Ghostty => Some("com.mitchellh.ghostty".into()),
            Terminal::Positron => Some("co.posit.positron".into()),
            Terminal::Trae => Some("com.trae.app".into()),
            Terminal::Custom(custom_terminal) => custom_terminal.macos.bundle_id.clone().map(Cow::Owned),
            _ => None,
        }
    }

    pub fn from_bundle_id(bundle: impl AsRef<str>) -> Option<Self> {
        let bundle = bundle.as_ref();
        let res = match bundle {
            "com.googlecode.iterm2" => Terminal::Iterm,
            "com.apple.Terminal" => Terminal::TerminalApp,
            "co.zeit.hyper" => Terminal::Hyper,
            "io.alacritty" | "org.alacritty" => Terminal::Alacritty,
            "net.kovidgoyal.kitty" => Terminal::Kitty,
            "com.microsoft.VSCode" => Terminal::VSCode,
            "com.microsoft.VSCodeInsiders" => Terminal::VSCodeInsiders,
            "com.vscodium" | "com.visualstudio.code.oss" => Terminal::VSCodium,
            "org.tabby" => Terminal::Tabby,
            "com.panic.Nova" => Terminal::Nova,
            "com.github.wez.wezterm" => Terminal::WezTerm,
            "dev.zed.Zed" => Terminal::Zed,
            "com.todesktop.230313mzl4w4u92" => Terminal::Cursor,
            "com.todesktop.23052492jqa5xjo" => Terminal::CursorNightly,
            "com.raphaelamorim.rio" => Terminal::Rio,
            "com.exafunction.windsurf" => Terminal::Windsurf,
            "com.exafunction.windsurf-next" => Terminal::WindsurfNext,
            "com.mitchellh.ghostty" => Terminal::Ghostty,
            "co.posit.positron" => Terminal::Positron,
            "com.trae.app" => Terminal::Trae,
            // TODO: the following line does not account for Android Studio
            _ if bundle.starts_with("com.jetbrains.") | bundle.starts_with("com.google.") => {
                Terminal::IntelliJ(IntelliJVariant::from_bundle_id(bundle))
            },
            _ => return None,
        };

        Some(res)
    }

    pub fn supports_macos_input_method(&self) -> bool {
        matches!(
            self,
            Terminal::Alacritty
                | Terminal::Kitty
                | Terminal::Nova
                | Terminal::WezTerm
                | Terminal::IntelliJ(_)
                | Terminal::Zed
                | Terminal::Rio
                | Terminal::Ghostty
        ) || self.as_custom().is_some_and(|c| c.macos.input_method)
    }

    pub fn supports_macos_accessibility(&self) -> bool {
        matches!(
            self,
            Terminal::Iterm
                | Terminal::TerminalApp
                | Terminal::VSCode
                | Terminal::VSCodeInsiders
                | Terminal::VSCodium
                | Terminal::Hyper
                | Terminal::Tabby
        ) || self.as_custom().is_some_and(|c| c.macos.accessibility)
    }

    pub fn is_xterm(&self) -> bool {
        matches!(
            self,
            Terminal::VSCode
                | Terminal::VSCodeInsiders
                | Terminal::VSCodium
                | Terminal::Hyper
                | Terminal::Tabby
                | Terminal::Cursor
                | Terminal::CursorNightly
                | Terminal::Windsurf
                | Terminal::WindsurfNext
                | Terminal::Trae
        ) || self.as_custom().is_some_and(|c| c.macos.xterm)
    }

    pub fn executable_names(&self) -> &'static [&'static str] {
        match self {
            Terminal::VSCode => &["code"],
            Terminal::VSCodeInsiders => &["code-insiders"],
            Terminal::Alacritty => &["alacritty"],
            Terminal::Kitty => &["kitty"],
            Terminal::GnomeConsole => &["kgx"],
            Terminal::GnomeTerminal => &["gnome-terminal-server"],
            Terminal::Konsole => &["konsole"],
            Terminal::Tilix => &["tilix"],
            Terminal::XfceTerminal => &["xfce4-terminal"],
            Terminal::Terminology => &["terminology"],
            Terminal::WezTerm => &["wezterm", "wezterm-gui"],
            Terminal::Hyper => &["hyper"],
            Terminal::Tabby => &["tabby"],
            Terminal::Terminator => &["terminator"],
            Terminal::Zed => &["zed"],
            Terminal::Cursor => &["Cursor", "cursor"],
            Terminal::CursorNightly => &["Cursor Nightly", "cursor-nightly"],
            Terminal::Rio => &["rio"],
            Terminal::Windsurf => &["windsurf"],
            Terminal::WindsurfNext => &["windsurf-next"],
            Terminal::Guake => &["guake"],
            Terminal::Ghostty => &["ghostty"],
            Terminal::Positron => &["positron"],
            Terminal::Trae => &["trae"],

            Terminal::Ssh => &["sshd"],
            Terminal::Tmux => &["tmux", "tmux: server"],
            Terminal::Vim => &["vim"],
            Terminal::Nvim => &["nvim"],
            Terminal::Zellij => &["zellij"],

            _ => &[],
        }
    }

    /// Returns the "Class" part of the WM_CLASS property.
    pub fn wm_class(&self) -> Option<&'static str> {
        match self {
            Terminal::VSCode => Some("Code"),
            Terminal::VSCodeInsiders => Some("Vscode-insiders"),
            Terminal::GnomeConsole => Some("Kgx"),
            Terminal::GnomeTerminal => Some("Gnome-terminal"),
            Terminal::Guake => Some("Guake"),
            Terminal::Hyper => Some("Hyper"),
            Terminal::Konsole => Some("konsole"),
            Terminal::Tilix => Some("Tilix"),
            Terminal::Alacritty => Some("Alacritty"),
            Terminal::Kitty => Some("kitty"),
            Terminal::XfceTerminal => Some("Xfce4-terminal"),
            Terminal::Terminator => Some("Terminator"),
            Terminal::Terminology => Some("terminology"),
            Terminal::WezTerm => Some("org.wezfurlong.wezterm"),
            Terminal::Tabby => Some("tabby"),
            Terminal::IntelliJ(Some(IntelliJVariant::IdeaCE)) => Some("jetbrains-idea-ce"),
            _ => None,
        }
    }

    /// Returns the "Instance" part of the WM_CLASS property.
    pub fn wm_class_instance(&self) -> Option<&'static str> {
        match self {
            Terminal::GnomeConsole => Some("org.gnome.Console"),
            Terminal::GnomeTerminal => Some("gnome-terminal-server"),
            Terminal::Guake => Some("guake"),
            Terminal::Hyper => Some("hyper"),
            Terminal::Terminator => Some("terminator"),
            Terminal::Tilix => Some("tilix"),
            // Many terminals seem to use the same name for both, falling back to Class name
            // as a default.
            _ => self.wm_class(),
        }
    }

    pub fn is_jetbrains_terminal() -> bool {
        // Handles all official JetBrain IDEs + Android Studio
        match std::env::var("TERMINAL_EMULATOR") {
            Ok(v) => v == "JetBrains-JediTerm",
            Err(_) => false,
        }
    }

    pub fn supports_fancy_boxes(&self) -> bool {
        !matches!(
            self,
            Terminal::VSCode
                | Terminal::VSCodeInsiders
                | Terminal::VSCodium
                | Terminal::Cursor
                | Terminal::CursorNightly
                | Terminal::Windsurf
                | Terminal::WindsurfNext
                | Terminal::Trae
        )
    }

    pub fn positioning_kind(&self) -> PositioningKind {
        match self {
            Terminal::Konsole => PositioningKind::Logical,
            _ => PositioningKind::Physical,
        }
    }

    /// Other pseudoterminal that we want to launch within
    pub fn is_special(&self) -> bool {
        matches!(
            self,
            Terminal::Ssh | Terminal::Tmux | Terminal::Vim | Terminal::Nvim | Terminal::Zellij
        )
    }

    pub fn as_custom(&self) -> Option<&CustomTerminal> {
        match self {
            Terminal::Custom(custom) => Some(custom),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum PositioningKind {
    Logical,
    Physical,
}

macro_rules! intellij_variants {
    ($($name:ident { org: $organization:expr, internal_id: $internal_id:expr, name: $application_name:expr, bundle: $bundle_identifier:expr },)*) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum IntelliJVariant {
            $(
                $name,
            )*
        }

        impl IntelliJVariant {
            pub const fn all() -> &'static [IntelliJVariant] {
                &[$(IntelliJVariant::$name,)*]
            }

            pub fn application_name(&self) -> &'static str {
                match self {
                    $(
                        IntelliJVariant::$name => $application_name,
                    )*
                }
            }

            pub fn organization(&self) -> &'static str {
                match self {
                    $(
                        IntelliJVariant::$name => $organization,
                    )*
                }
            }

            pub fn bundle_identifier(&self) -> &'static str {
                match self {
                    $(
                        IntelliJVariant::$name => $bundle_identifier,
                    )*
                }
            }

            pub fn internal_id(&self) -> &'static str {
                match self {
                    $(
                        IntelliJVariant::$name => $internal_id,
                    )*
                }
            }

            pub fn from_bundle_id(bundle_id: &str) -> Option<IntelliJVariant> {
                match bundle_id {
                    $(
                        $bundle_identifier => Some(IntelliJVariant::$name),
                    )*
                    _ => None,
                }
            }
        }
    };
}

intellij_variants! {
    IdeaUltimate {
        org: "JetBrains",
        internal_id: "idea-ultimate",
        name: "IDEA Ultimate",
        bundle: "com.jetbrains.intellij"
    },
    IdeaUltimateEap {
        org: "JetBrains",
        internal_id: "idea-ultimate-eap",
        name: "IDEA Ultimate EAP",
        bundle: "com.jetbrains.intellij-EAP"
    },
    IdeaCE {
        org: "JetBrains",
        internal_id: "idea-ce",
        name: "IDEA Community",
        bundle: "com.jetbrains.intellij.ce"
    },
    WebStorm {
        org: "JetBrains",
        internal_id: "webstorm",
        name: "WebStorm",
        bundle: "com.jetbrains.WebStorm"
    },
    GoLand {
        org: "JetBrains",
        internal_id: "goland",
        name: "GoLand",
        bundle: "com.jetbrains.goland"
    },
    PhpStorm {
        org: "JetBrains",
        internal_id: "phpstorm",
        name: "PhpStorm",
        bundle: "com.jetbrains.PhpStorm"
    },
    PyCharm {
        org: "JetBrains",
        internal_id: "pycharm",
        name: "PyCharm Professional",
        bundle: "com.jetbrains.pycharm"
    },
    PyCharmCE {
        org: "JetBrains",
        internal_id: "pycharm-ce",
        name: "PyCharm Community",
        bundle: "com.jetbrains.pycharm.ce"
    },
    AppCode {
        org: "JetBrains",
        internal_id: "appcode",
        name: "AppCode",
        bundle: "com.jetbrains.AppCode"
    },
    CLion {
        org: "JetBrains",
        internal_id: "clion",
        name: "CLion",
        bundle: "com.jetbrains.CLion"
    },
    Rider {
        org: "JetBrains",
        internal_id: "rider",
        name: "Rider",
        bundle: "com.jetbrains.rider"
    },
    RubyMine {
        org: "JetBrains",
        internal_id: "rubymine",
        name: "RubyMine",
        bundle: "com.jetbrains.rubymine"
    },
    DataSpell {
        org: "JetBrains",
        internal_id: "dataspell",
        name: "DataSpell",
        bundle: "com.jetbrains.dataspell"
    },
    DataGrip {
        org: "JetBrains",
        internal_id: "datagrip",
        name: "DataGrip",
        bundle: "com.jetbrains.datagrip"
    },
    RustRover {
        org: "JetBrains",
        internal_id: "rustrover",
        name: "Rust Rover",
        bundle: "com.jetbrains.rustrover"
    },
    RustRoverEap {
        org: "JetBrains",
        internal_id: "rustrover-EAP",
        name: "Rust Rover EAP",
        bundle: "com.jetbrains.rustrover-EAP"
    },
    AndroidStudio {
        org: "Google",
        internal_id: "android-studio",
        name: "Android Studio",
        bundle: "com.google.android.studio"
    },
}

impl IntelliJVariant {
    pub fn from_product_code(from: &str) -> Option<Self> {
        Some(match from {
            "IU" => IntelliJVariant::IdeaUltimate,
            "IC" => IntelliJVariant::IdeaCE,
            "PC" => IntelliJVariant::PyCharmCE,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fig_os_shim::process_info::TestExe;
    use fig_os_shim::{
        Os,
        ProcessInfo,
    };

    use super::*;

    fn make_context<T: Into<TestExe>>(os: Os, processes: Vec<T>) -> Arc<Context> {
        Context::builder()
            .with_os(os)
            .with_process_info(ProcessInfo::from_exes(processes))
            .build()
    }

    #[test]
    fn test_from_process_info() {
        Terminal::from_process_info(&Context::new(), &MACOS_TERMINALS.to_vec());

        let ctx = make_context(Os::Linux, vec!["q", "fish", "wezterm"]);
        assert_eq!(
            Terminal::from_process_info(&ctx, &LINUX_TERMINALS.to_vec()),
            Some(Terminal::WezTerm)
        );

        let ctx = make_context(Os::Linux, vec!["q", "bash", "tmux"]);
        assert_eq!(
            Terminal::from_process_info(&ctx, &LINUX_TERMINALS.to_vec()),
            None,
            "Special terminals should return None"
        );

        let ctx = make_context(Os::Linux, vec!["cargo", "cargo", "q", "bash", "tmux", "wezterm"]);
        assert_eq!(
            Terminal::from_process_info(&ctx, &LINUX_TERMINALS.to_vec()),
            None,
            "Max search depth reached should return None"
        );

        let ctx = make_context(Os::Linux, vec![
            (Some("q"), None),
            (Some("python3"), Some("/usr/bin/python3 /usr/bin/terminator")),
        ]);
        assert_eq!(
            Terminal::from_process_info(&ctx, &LINUX_TERMINALS.to_vec()),
            Some(Terminal::Terminator),
            "should return terminator"
        );

        let ctx = make_context(Os::Linux, vec![
            (Some("q"), None),
            (Some("python3"), Some("/usr/bin/python3 /usr/bin/guake")),
        ]);
        assert_eq!(
            Terminal::from_process_info(&ctx, &LINUX_TERMINALS.to_vec()),
            Some(Terminal::Guake),
            "should return guake"
        );
    }
}
