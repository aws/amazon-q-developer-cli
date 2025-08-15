use std::fmt::Display;
use std::io::SeekFrom;

use crossterm::style::Color;
use fd_lock::RwLock;
use regex::Regex;
use serde_json::{
    Map,
    Value,
};
use tokio::fs::File;
use tokio::io::{
    AsyncReadExt,
    AsyncSeekExt,
    AsyncWriteExt,
};

use super::DatabaseError;

/// Setting key that can be either static (enum-based) or dynamic (namespace-based)
#[derive(Clone, Debug)]
pub enum SettingKey {
    Static(Setting),
    ThemeColor { theme: String, category: String },
}

impl SettingKey {
    pub fn as_string(&self) -> String {
        match self {
            Self::Static(setting) => setting.as_ref().to_string(),
            Self::ThemeColor { theme, category } => {
                format!("chat.theme.{}.{}", theme, category)
            }
        }
    }
}

impl TryFrom<&str> for SettingKey {
    type Error = DatabaseError;
    
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Try static settings first
        if let Ok(setting) = Setting::try_from(value) {
            return Ok(Self::Static(setting));
        }
        
        // Check for theme color pattern: chat.theme.{theme}.{color}
        static THEME_COLOR_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let regex = THEME_COLOR_REGEX.get_or_init(|| {
            Regex::new(r"^chat\.theme\.([^.]+)\.([^.]+)$").unwrap()
        });
        
        if let Some(captures) = regex.captures(value) {
            let theme = captures.get(1).unwrap().as_str().to_string();
            let category = captures.get(2).unwrap().as_str().to_string();
            
            // Validate color category
            if !["success", "error", "warning", "info", "secondary", "primary", "action", "data"]
                .contains(&category.as_str()) {
                return Err(DatabaseError::InvalidSetting(value.to_string()));
            }
            
            return Ok(Self::ThemeColor { theme, category });
        }
        
        Err(DatabaseError::InvalidSetting(value.to_string()))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Setting {
    TelemetryEnabled,
    OldClientId,
    ShareCodeWhispererContent,
    EnabledThinking,
    EnabledKnowledge,
    SkimCommandKey,
    ChatGreetingEnabled,
    ApiTimeout,
    ChatEditMode,
    ChatEnableNotifications,
    ApiCodeWhispererService,
    ApiQService,
    McpInitTimeout,
    McpNoInteractiveTimeout,
    McpLoadedBefore,
    ChatDefaultModel,
    ChatDisableMarkdownRendering,
    ChatDefaultAgent,
    ChatDisableAutoCompaction,
    ChatEnableHistoryHints,

    // Color theme settings
    ChatTheme,
    ChatThemeSuccess,
    ChatThemeError,
    ChatThemeWarning,
    ChatThemeInfo,
    ChatThemeSecondary,
    ChatThemePrimary,
    ChatThemeAction,
    ChatThemeData,
}

/// Semantic color categories for consistent theming
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorCategory {
    /// Success operations, completions, positive feedback
    Success,
    /// Errors, failures, critical issues
    Error,
    /// Warnings, cautions, informational alerts
    Warning,
    /// Informational content, references, system responses
    Info,
    /// Secondary information, help text, less prominent elements
    Secondary,
    /// Primary UI elements, branding, important system messages
    Primary,
    /// Tool usage, actions, user interactions
    Action,
    /// Context files, data visualization
    Data,
}

/// Predefined theme names
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeName {
    Default,
    HighContrast,
    Light,
    Nord,
}

impl ThemeName {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default" => Some(Self::Default),
            "high-contrast" | "high_contrast" => Some(Self::HighContrast),
            "light" => Some(Self::Light),
            "nord" => Some(Self::Nord),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::HighContrast => "high-contrast",
            Self::Light => "light",
            Self::Nord => "nord",
        }
    }
}

/// Color theme definitions
#[derive(Clone, Debug, PartialEq)]
pub struct ColorTheme {
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub secondary: Color,
    pub primary: Color,
    pub action: Color,
    pub data: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self::default_theme()
    }
}

impl ColorTheme {
    /// Default color theme (current Amazon Q CLI colors)
    pub fn default_theme() -> Self {
        Self {
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,
            secondary: Color::DarkGrey,
            primary: Color::Cyan,
            action: Color::Magenta,
            data: Color::DarkCyan,
        }
    }

    /// High contrast theme for better accessibility
    pub fn high_contrast_theme() -> Self {
        Self {
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,
            secondary: Color::White,  // Changed from DarkGrey for better visibility
            primary: Color::Cyan,
            action: Color::Magenta,
            data: Color::DarkCyan,
        }
    }

    /// Terminal-friendly theme for terminals with light backgrounds
    pub fn light_theme() -> Self {
        Self {
            success: Color::DarkGreen,
            error: Color::DarkRed,
            warning: Color::DarkYellow,
            info: Color::DarkBlue,
            secondary: Color::DarkGrey,
            primary: Color::DarkCyan,
            action: Color::DarkMagenta,
            data: Color::DarkCyan,
        }
    }

    /// Nord theme - Arctic, north-bluish color palette
    pub fn nord_theme() -> Self {
        Self {
            success: Color::Rgb { r: 163, g: 190, b: 140 }, // Nord14
            error: Color::Rgb { r: 191, g: 97, b: 106 },    // Nord11
            warning: Color::Rgb { r: 235, g: 203, b: 139 }, // Nord13
            info: Color::Rgb { r: 129, g: 161, b: 193 },    // Nord10
            secondary: Color::Rgb { r: 76, g: 86, b: 106 }, // Nord3
            primary: Color::Rgb { r: 136, g: 192, b: 208 }, // Nord8
            action: Color::Rgb { r: 180, g: 142, b: 173 },  // Nord15
            data: Color::Rgb { r: 94, g: 129, b: 172 },     // Nord9
        }
    }

    /// Get color for a specific category
    pub fn get_color(&self, category: ColorCategory) -> Color {
        match category {
            ColorCategory::Success => self.success,
            ColorCategory::Error => self.error,
            ColorCategory::Warning => self.warning,
            ColorCategory::Info => self.info,
            ColorCategory::Secondary => self.secondary,
            ColorCategory::Primary => self.primary,
            ColorCategory::Action => self.action,
            ColorCategory::Data => self.data,
        }
    }

    /// Create theme from settings
    pub fn from_settings(settings: &Settings) -> Self {
        // First check if a predefined theme is selected
        if let Some(theme_name) = settings.get_string(Setting::ChatTheme) {
            if let Some(theme) = ThemeName::from_str(&theme_name) {
                let base_theme = match theme {
                    ThemeName::Default => Self::default_theme(),
                    ThemeName::HighContrast => Self::high_contrast_theme(),
                    ThemeName::Light => Self::light_theme(),
                    ThemeName::Nord => Self::nord_theme(),
                };
                
                // Apply any theme-specific color overrides on top of the base theme
                let theme_prefix = format!("chat.theme.{}", theme.as_str());
                return Self {
                    success: settings.get_color_by_key(&format!("{}.success", theme_prefix)).unwrap_or(base_theme.success),
                    error: settings.get_color_by_key(&format!("{}.error", theme_prefix)).unwrap_or(base_theme.error),
                    warning: settings.get_color_by_key(&format!("{}.warning", theme_prefix)).unwrap_or(base_theme.warning),
                    info: settings.get_color_by_key(&format!("{}.info", theme_prefix)).unwrap_or(base_theme.info),
                    secondary: settings.get_color_by_key(&format!("{}.secondary", theme_prefix)).unwrap_or(base_theme.secondary),
                    primary: settings.get_color_by_key(&format!("{}.primary", theme_prefix)).unwrap_or(base_theme.primary),
                    action: settings.get_color_by_key(&format!("{}.action", theme_prefix)).unwrap_or(base_theme.action),
                    data: settings.get_color_by_key(&format!("{}.data", theme_prefix)).unwrap_or(base_theme.data),
                };
            }
        }
        
        // Fallback to individual color settings (current behavior)
        Self {
            success: settings.get_color(Setting::ChatThemeSuccess).unwrap_or(Color::Green),
            error: settings.get_color(Setting::ChatThemeError).unwrap_or(Color::Red),
            warning: settings.get_color(Setting::ChatThemeWarning).unwrap_or(Color::Yellow),
            info: settings.get_color(Setting::ChatThemeInfo).unwrap_or(Color::Blue),
            secondary: settings.get_color(Setting::ChatThemeSecondary).unwrap_or(Color::DarkGrey),
            primary: settings.get_color(Setting::ChatThemePrimary).unwrap_or(Color::Cyan),
            action: settings.get_color(Setting::ChatThemeAction).unwrap_or(Color::Magenta),
            data: settings.get_color(Setting::ChatThemeData).unwrap_or(Color::DarkCyan),
        }
    }
}

impl AsRef<str> for Setting {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::TelemetryEnabled => "telemetry.enabled",
            Self::OldClientId => "telemetryClientId",
            Self::ShareCodeWhispererContent => "codeWhisperer.shareCodeWhispererContentWithAWS",
            Self::EnabledThinking => "chat.enableThinking",
            Self::EnabledKnowledge => "chat.enableKnowledge",
            Self::SkimCommandKey => "chat.skimCommandKey",
            Self::ChatGreetingEnabled => "chat.greeting.enabled",
            Self::ApiTimeout => "api.timeout",
            Self::ChatEditMode => "chat.editMode",
            Self::ChatEnableNotifications => "chat.enableNotifications",
            Self::ApiCodeWhispererService => "api.codewhisperer.service",
            Self::ApiQService => "api.q.service",
            Self::McpInitTimeout => "mcp.initTimeout",
            Self::McpNoInteractiveTimeout => "mcp.noInteractiveTimeout",
            Self::McpLoadedBefore => "mcp.loadedBefore",
            Self::ChatDefaultModel => "chat.defaultModel",
            Self::ChatDisableMarkdownRendering => "chat.disableMarkdownRendering",
            Self::ChatDefaultAgent => "chat.defaultAgent",
            Self::ChatDisableAutoCompaction => "chat.disableAutoCompaction",
            Self::ChatEnableHistoryHints => "chat.enableHistoryHints",
            Self::ChatTheme => "chat.theme",
            Self::ChatThemeSuccess => "chat.theme.success",
            Self::ChatThemeError => "chat.theme.error",
            Self::ChatThemeWarning => "chat.theme.warning",
            Self::ChatThemeInfo => "chat.theme.info",
            Self::ChatThemeSecondary => "chat.theme.secondary",
            Self::ChatThemePrimary => "chat.theme.primary",
            Self::ChatThemeAction => "chat.theme.action",
            Self::ChatThemeData => "chat.theme.data",
        }
    }
}

impl Display for Setting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl TryFrom<&str> for Setting {
    type Error = DatabaseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "telemetry.enabled" => Ok(Self::TelemetryEnabled),
            "telemetryClientId" => Ok(Self::OldClientId),
            "codeWhisperer.shareCodeWhispererContentWithAWS" => Ok(Self::ShareCodeWhispererContent),
            "chat.enableThinking" => Ok(Self::EnabledThinking),
            "chat.enableKnowledge" => Ok(Self::EnabledKnowledge),
            "chat.skimCommandKey" => Ok(Self::SkimCommandKey),
            "chat.greeting.enabled" => Ok(Self::ChatGreetingEnabled),
            "api.timeout" => Ok(Self::ApiTimeout),
            "chat.editMode" => Ok(Self::ChatEditMode),
            "chat.enableNotifications" => Ok(Self::ChatEnableNotifications),
            "api.codewhisperer.service" => Ok(Self::ApiCodeWhispererService),
            "api.q.service" => Ok(Self::ApiQService),
            "mcp.initTimeout" => Ok(Self::McpInitTimeout),
            "mcp.noInteractiveTimeout" => Ok(Self::McpNoInteractiveTimeout),
            "mcp.loadedBefore" => Ok(Self::McpLoadedBefore),
            "chat.defaultModel" => Ok(Self::ChatDefaultModel),
            "chat.disableMarkdownRendering" => Ok(Self::ChatDisableMarkdownRendering),
            "chat.defaultAgent" => Ok(Self::ChatDefaultAgent),
            "chat.disableAutoCompaction" => Ok(Self::ChatDisableAutoCompaction),
            "chat.enableHistoryHints" => Ok(Self::ChatEnableHistoryHints),
            "chat.theme" => Ok(Self::ChatTheme),
            "chat.theme.success" => Ok(Self::ChatThemeSuccess),
            "chat.theme.error" => Ok(Self::ChatThemeError),
            "chat.theme.warning" => Ok(Self::ChatThemeWarning),
            "chat.theme.info" => Ok(Self::ChatThemeInfo),
            "chat.theme.secondary" => Ok(Self::ChatThemeSecondary),
            "chat.theme.primary" => Ok(Self::ChatThemePrimary),
            "chat.theme.action" => Ok(Self::ChatThemeAction),
            "chat.theme.data" => Ok(Self::ChatThemeData),
            _ => Err(DatabaseError::InvalidSetting(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Settings(Map<String, Value>);

impl Settings {
    pub async fn new() -> Result<Self, DatabaseError> {
        if cfg!(test) {
            return Ok(Self::default());
        }

        let path = crate::util::directories::settings_path()?;

        // If the folder doesn't exist, create it.
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        Ok(Self(match path.exists() {
            true => {
                let mut file = RwLock::new(File::open(&path).await?);
                let mut buf = Vec::new();
                file.write()?.read_to_end(&mut buf).await?;
                serde_json::from_slice(&buf)?
            },
            false => {
                let mut file = RwLock::new(File::create(path).await?);
                file.write()?.write_all(b"{}").await?;
                serde_json::Map::new()
            },
        }))
    }

    pub fn map(&self) -> &'_ Map<String, Value> {
        &self.0
    }

    pub fn get(&self, key: Setting) -> Option<&Value> {
        self.0.get(key.as_ref())
    }

    /// Get value by string key (for dynamic settings)
    pub fn get_by_key(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub async fn set(&mut self, key: Setting, value: impl Into<serde_json::Value>) -> Result<(), DatabaseError> {
        self.0.insert(key.to_string(), value.into());
        self.save_to_file().await
    }

    pub async fn remove(&mut self, key: Setting) -> Result<Option<Value>, DatabaseError> {
        let key = self.0.remove(key.as_ref());
        self.save_to_file().await?;
        Ok(key)
    }

    pub fn get_bool(&self, key: Setting) -> Option<bool> {
        self.get(key).and_then(|value| value.as_bool())
    }

    pub fn get_string(&self, key: Setting) -> Option<String> {
        self.get(key).and_then(|value| value.as_str().map(|s| s.into()))
    }

    pub fn get_int(&self, key: Setting) -> Option<i64> {
        self.get(key).and_then(|value| value.as_i64())
    }

    /// Get a color setting, parsing from string representation
    pub fn get_color(&self, key: Setting) -> Option<Color> {
        self.get_string(key).and_then(|color_str| parse_color(&color_str))
    }

    /// Get color by string key (for theme-specific settings)
    pub fn get_color_by_key(&self, key: &str) -> Option<Color> {
        self.get_string_by_key(key).and_then(|color_str| parse_color(&color_str))
    }

    /// Get string value by string key
    pub fn get_string_by_key(&self, key: &str) -> Option<String> {
        self.0.get(key).and_then(|value| value.as_str().map(|s| s.to_string()))
    }

    /// Get the current color theme
    pub fn get_color_theme(&self) -> ColorTheme {
        ColorTheme::from_settings(self)
    }

    /// Set the theme by name
    pub async fn set_theme(&mut self, theme: ThemeName) -> Result<(), DatabaseError> {
        self.set(Setting::ChatTheme, theme.as_str()).await
    }

    /// Get the current theme name
    pub fn get_theme(&self) -> Option<ThemeName> {
        self.get_string(Setting::ChatTheme)
            .and_then(|s| ThemeName::from_str(&s))
    }

    /// Set value by string key (for dynamic settings)
    pub async fn set_by_key(&mut self, key: &str, value: impl Into<serde_json::Value>) -> Result<(), DatabaseError> {
        self.0.insert(key.to_string(), value.into());
        self.save_to_file().await
    }

    /// Remove value by string key (for dynamic settings)
    pub async fn remove_by_key(&mut self, key: &str) -> Result<Option<Value>, DatabaseError> {
        let value = self.0.remove(key);
        self.save_to_file().await?;
        Ok(value)
    }

    pub async fn save_to_file(&self) -> Result<(), DatabaseError> {
        if cfg!(test) {
            return Ok(());
        }

        let path = crate::util::directories::settings_path()?;

        // If the folder doesn't exist, create it.
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let mut file_opts = File::options();
        file_opts.create(true).write(true).truncate(true);

        #[cfg(unix)]
        file_opts.mode(0o600);
        let mut file = RwLock::new(file_opts.open(&path).await?);
        let mut lock = file.write()?;

        match serde_json::to_string_pretty(&self.0) {
            Ok(json) => lock.write_all(json.as_bytes()).await?,
            Err(_err) => {
                lock.seek(SeekFrom::Start(0)).await?;
                lock.set_len(0).await?;
                lock.write_all(b"{}").await?;
            },
        }
        lock.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// General read/write settings test
    #[tokio::test]
    async fn test_settings() {
        let mut settings = Settings::new().await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDefaultModel), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);

        settings.set(Setting::TelemetryEnabled, true).await.unwrap();
        settings.set(Setting::OldClientId, "test").await.unwrap();
        settings.set(Setting::ShareCodeWhispererContent, false).await.unwrap();
        settings.set(Setting::McpLoadedBefore, true).await.unwrap();
        settings.set(Setting::ChatDefaultModel, "model 1").await.unwrap();
        settings
            .set(Setting::ChatDisableMarkdownRendering, false)
            .await
            .unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get(Setting::OldClientId),
            Some(&Value::String("test".to_string()))
        );
        assert_eq!(
            settings.get(Setting::ShareCodeWhispererContent),
            Some(&Value::Bool(false))
        );
        assert_eq!(settings.get(Setting::McpLoadedBefore), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get(Setting::ChatDefaultModel),
            Some(&Value::String("model 1".to_string()))
        );
        assert_eq!(
            settings.get(Setting::ChatDisableMarkdownRendering),
            Some(&Value::Bool(false))
        );

        settings.remove(Setting::TelemetryEnabled).await.unwrap();
        settings.remove(Setting::OldClientId).await.unwrap();
        settings.remove(Setting::ShareCodeWhispererContent).await.unwrap();
        settings.remove(Setting::McpLoadedBefore).await.unwrap();
        settings.remove(Setting::ChatDisableMarkdownRendering).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);
    }

    #[tokio::test]
    async fn test_theme_name_parsing() {
        assert_eq!(ThemeName::from_str("default"), Some(ThemeName::Default));
        assert_eq!(ThemeName::from_str("Default"), Some(ThemeName::Default));
        assert_eq!(ThemeName::from_str("DEFAULT"), Some(ThemeName::Default));
        
        assert_eq!(ThemeName::from_str("high-contrast"), Some(ThemeName::HighContrast));
        assert_eq!(ThemeName::from_str("high_contrast"), Some(ThemeName::HighContrast));
        assert_eq!(ThemeName::from_str("HIGH-CONTRAST"), Some(ThemeName::HighContrast));
        
        assert_eq!(ThemeName::from_str("light"), Some(ThemeName::Light));
        assert_eq!(ThemeName::from_str("Light"), Some(ThemeName::Light));
        
        assert_eq!(ThemeName::from_str("invalid"), None);
        assert_eq!(ThemeName::from_str(""), None);
    }

    #[tokio::test]
    async fn test_theme_name_as_str() {
        assert_eq!(ThemeName::Default.as_str(), "default");
        assert_eq!(ThemeName::HighContrast.as_str(), "high-contrast");
        assert_eq!(ThemeName::Light.as_str(), "light");
    }

    #[tokio::test]
    async fn test_color_theme_from_settings_with_theme_name() {
        let mut settings = Settings::new().await.unwrap();
        
        // Test default theme selection
        settings.set(Setting::ChatTheme, "default").await.unwrap();
        let theme = settings.get_color_theme();
        assert_eq!(theme, ColorTheme::default_theme());
        
        // Test high contrast theme selection
        settings.set(Setting::ChatTheme, "high-contrast").await.unwrap();
        let theme = settings.get_color_theme();
        assert_eq!(theme, ColorTheme::high_contrast_theme());
        
        // Test light theme selection
        settings.set(Setting::ChatTheme, "light").await.unwrap();
        let theme = settings.get_color_theme();
        assert_eq!(theme, ColorTheme::light_theme());
    }

    #[tokio::test]
    async fn test_color_theme_from_settings_with_overrides() {
        let mut settings = Settings::new().await.unwrap();
        
        // Set a base theme
        settings.set(Setting::ChatTheme, "default").await.unwrap();
        
        // Override the primary color
        settings.set_color(Setting::ChatThemePrimary, Color::Blue).await.unwrap();
        
        let theme = settings.get_color_theme();
        let default_theme = ColorTheme::default_theme();
        
        // Should use the base theme for most colors
        assert_eq!(theme.success, default_theme.success);
        assert_eq!(theme.error, default_theme.error);
        
        // But use the override for primary
        assert_eq!(theme.primary, Color::Blue);
    }

    #[tokio::test]
    async fn test_color_theme_from_settings_fallback() {
        let mut settings = Settings::new().await.unwrap();
        
        // No theme set, should use individual colors or defaults
        settings.set_color(Setting::ChatThemePrimary, Color::Blue).await.unwrap();
        
        let theme = settings.get_color_theme();
        
        // Should use default colors for unset values
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.error, Color::Red);
        
        // Should use the individual setting
        assert_eq!(theme.primary, Color::Blue);
    }

    #[tokio::test]
    async fn test_theme_helper_methods() {
        let mut settings = Settings::new().await.unwrap();
        
        // Test setting theme by name
        settings.set_theme(ThemeName::HighContrast).await.unwrap();
        assert_eq!(settings.get_theme(), Some(ThemeName::HighContrast));
        assert_eq!(settings.get_string(Setting::ChatTheme), Some("high-contrast".to_string()));
        
        // Test getting theme
        settings.set(Setting::ChatTheme, "light").await.unwrap();
        assert_eq!(settings.get_theme(), Some(ThemeName::Light));
        
        // Test clearing theme
        settings.clear_theme().await.unwrap();
        assert_eq!(settings.get_theme(), None);
        assert_eq!(settings.get(Setting::ChatTheme), None);
    }
}

/// Parse a color from string representation
fn parse_color(color_str: &str) -> Option<Color> {
    match color_str.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "darkgrey" | "dark_grey" => Some(Color::DarkGrey),
        "red" => Some(Color::Red),
        "darkred" | "dark_red" => Some(Color::DarkRed),
        "green" => Some(Color::Green),
        "darkgreen" | "dark_green" => Some(Color::DarkGreen),
        "yellow" => Some(Color::Yellow),
        "darkyellow" | "dark_yellow" => Some(Color::DarkYellow),
        "blue" => Some(Color::Blue),
        "darkblue" | "dark_blue" => Some(Color::DarkBlue),
        "magenta" => Some(Color::Magenta),
        "darkmagenta" | "dark_magenta" => Some(Color::DarkMagenta),
        "cyan" => Some(Color::Cyan),
        "darkcyan" | "dark_cyan" => Some(Color::DarkCyan),
        "white" => Some(Color::White),
        "grey" | "gray" => Some(Color::Grey),
        "reset" => Some(Color::Reset),
        _ => {
            // Try to parse RGB format: "rgb(r,g,b)" or "#rrggbb"
            if color_str.starts_with("rgb(") && color_str.ends_with(')') {
                let rgb_str = &color_str[4..color_str.len() - 1];
                let parts: Vec<&str> = rgb_str.split(',').collect();
                if parts.len() == 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse::<u8>(),
                        parts[1].trim().parse::<u8>(),
                        parts[2].trim().parse::<u8>(),
                    ) {
                        return Some(Color::Rgb { r, g, b });
                    }
                }
            } else if color_str.starts_with('#') && color_str.len() == 7 {
                if let Ok(hex) = u32::from_str_radix(&color_str[1..], 16) {
                    let r = ((hex >> 16) & 0xFF) as u8;
                    let g = ((hex >> 8) & 0xFF) as u8;
                    let b = (hex & 0xFF) as u8;
                    return Some(Color::Rgb { r, g, b });
                }
            }
            None
        }
    }
}
