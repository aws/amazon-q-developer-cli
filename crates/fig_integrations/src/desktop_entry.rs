use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{
    Path,
    PathBuf,
};
use std::str::FromStr;

use async_trait::async_trait;
use fig_os_shim::{
    EnvProvider,
    Fs,
    FsProvider,
};
use fig_settings::{
    Settings,
    State,
};
use fig_util::PRODUCT_NAME;
use fig_util::consts::APP_PROCESS_NAME;
use fig_util::consts::linux::DESKTOP_ENTRY_NAME;
use fig_util::directories::home_dir_ctx;

use crate::Integration;
use crate::error::{
    Error,
    ErrorExt,
    Result,
};

/// Path to the local [PRODUCT_NAME] desktop entry installed under `~/.local/share/applications`
pub fn local_entry_path<Ctx: FsProvider + EnvProvider>(ctx: &Ctx) -> Result<PathBuf> {
    Ok(home_dir_ctx(ctx)?.join(format!(".local/share/applications/{}", DESKTOP_ENTRY_NAME)))
}

/// Path to the global [PRODUCT_NAME] desktop entry installed under `/usr/share/applications`
pub fn global_entry_path<Ctx: FsProvider>(ctx: &Ctx) -> PathBuf {
    // Using chroot_path for test code.
    ctx.fs()
        .chroot_path(format!("/usr/share/applications/{}", DESKTOP_ENTRY_NAME))
}

/// Path to the local autostart symlink.
pub fn local_autostart_path<Ctx: FsProvider + EnvProvider>(ctx: &Ctx) -> Result<PathBuf> {
    Ok(home_dir_ctx(ctx)?.join(format!(".config/autostart/{}", DESKTOP_ENTRY_NAME)))
}

/// Path to the icon referenced by the desktop entry.
pub fn local_icon_path<Ctx: FsProvider>(ctx: &Ctx) -> Result<PathBuf> {
    Ok(fig_util::directories::fig_data_dir_ctx(ctx)?.join(format!("{APP_PROCESS_NAME}.png")))
}

/// Helper to create the parent directory of `path` if it doesn't already exist.
async fn create_parent(fs: &Fs, path: impl AsRef<Path>) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.is_dir() {
            fs.fs().create_dir_all(parent).await?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct DesktopEntryIntegration<'a, Ctx> {
    ctx: &'a Ctx,

    /// Path to the desktop entry file to be installed. Required to be [Option::Some] for
    /// installing.
    entry_path: Option<PathBuf>,

    /// Path to the desktop entry icon image to be installed. Required to be [Option::Some] for
    /// installing.
    icon_path: Option<PathBuf>,

    /// Path to the executable to be set for the "Exec" field. Required to be [Option::Some] for
    /// installing, and validating the Exec field for [Self::is_installed].
    exec_path: Option<PathBuf>,
}

impl<'a, Ctx> DesktopEntryIntegration<'a, Ctx>
where
    Ctx: FsProvider + EnvProvider,
{
    /// Creates a new [`DesktopEntryIntegration`].
    pub fn new<P>(ctx: &'a Ctx, entry_path: Option<P>, icon_path: Option<P>, exec_path: Option<P>) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            ctx,
            entry_path: entry_path.map(|p| p.as_ref().into()),
            icon_path: icon_path.map(|p| p.as_ref().into()),
            exec_path: exec_path.map(|p| p.as_ref().into()),
        }
    }

    fn validate_field_path(entry_contents: &EntryContents, field: &str, expected_path: &PathBuf) -> Result<()> {
        match entry_contents.get_field(field) {
            Some(path) => {
                let set_path = PathBuf::from_str(path)
                    .map_err(|err| Error::ImproperInstallation(format!("Invalid field {}: {:?}", field, err).into()))?;
                if set_path != *expected_path {
                    return Err(Error::ImproperInstallation(
                        format!(
                            "Invalid field: {}. Expected {}, found {}",
                            field,
                            expected_path.to_string_lossy(),
                            set_path.to_string_lossy(),
                        )
                        .into(),
                    ));
                }
            },
            None => {
                return Err(Error::ImproperInstallation(
                    format!("Field {} is missing", field).into(),
                ));
            },
        }
        Ok(())
    }
}

#[async_trait]
impl<Ctx> Integration for DesktopEntryIntegration<'_, Ctx>
where
    Ctx: FsProvider + EnvProvider + Sync,
{
    fn describe(&self) -> String {
        "Desktop Entry Integration".to_owned()
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }
        let (entry_path, icon_path, exec_path) = match (&self.entry_path, &self.icon_path, &self.exec_path) {
            (Some(entry), Some(icon), Some(exec)) => (entry, icon, exec),
            _ => {
                return Err(Error::Custom(
                    "entry, icon, and exec paths are required for installation".into(),
                ));
            },
        };

        let fs = self.ctx.fs();

        let to_entry_path = local_entry_path(self.ctx)?;
        let to_icon_path = local_icon_path(self.ctx)?;

        // Required in case the user doesn't already have the local directories on their filesystem.
        create_parent(fs, &to_entry_path).await?;
        create_parent(fs, &to_icon_path).await?;

        // Install to the user local paths. Load the current entry if it exists, in case the user
        // adds any additional fields themself.
        let mut entry_contents = if fs.exists(&to_entry_path) {
            EntryContents::from_path(fs, &to_entry_path).await?
        } else {
            EntryContents::from_path(fs, entry_path).await?
        };
        entry_contents.set_field("Exec", &exec_path.to_string_lossy());
        entry_contents.set_field("Name", PRODUCT_NAME);
        entry_contents.set_field("Icon", &to_icon_path.to_string_lossy());
        fs.write(&to_entry_path, entry_contents.to_string()).await?;
        if !fs.exists(&to_icon_path) {
            fs.copy(icon_path, &to_icon_path).await?;
        }

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        let fs = self.ctx.fs();
        let to_entry_path = local_entry_path(self.ctx)?;
        let to_icon_path = local_icon_path(self.ctx)?;
        if fs.exists(&to_entry_path) {
            fs.remove_file(&to_entry_path).await?;
        }
        if fs.exists(&to_icon_path) {
            fs.remove_file(&to_icon_path).await?;
        }
        Ok(())
    }

    async fn is_installed(&self) -> Result<()> {
        let fs = self.ctx.fs();
        let to_entry_path = local_entry_path(self.ctx)?;
        let to_icon_path = local_icon_path(self.ctx)?;

        // Check if the installed entry exists.
        let entry_contents = match fs.read_to_string(&to_entry_path).await.with_path(&to_entry_path) {
            Ok(contents) => contents,
            Err(Error::Io(err)) if err.kind() == ErrorKind::NotFound => {
                return Err(Error::FileDoesNotExist(to_entry_path.clone().into()));
            },
            Err(err) => return Err(err),
        };
        let entry_contents = EntryContents::new(entry_contents);

        if !fs.exists(&to_icon_path) {
            return Err(Error::FileDoesNotExist(to_icon_path.clone().into()));
        }

        if let Some(exec_path) = &self.exec_path {
            Self::validate_field_path(&entry_contents, "Exec", exec_path)?;
        }
        Self::validate_field_path(&entry_contents, "Icon", &to_icon_path)?;

        Ok(())
    }
}

/// Helper struct for parsing and updating a desktop entry.
#[derive(Debug, Clone)]
pub struct EntryContents {
    /// The lines of a desktop entry stored in a vector.
    lines: Vec<String>,

    /// Map of a key name to the line in `lines`.
    fields: HashMap<String, usize>,
}

impl std::fmt::Display for EntryContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.lines.join("\n"))
    }
}

impl EntryContents {
    pub fn new(entry_buf: String) -> Self {
        let lines = entry_buf.lines().map(str::to_owned).collect::<Vec<_>>();
        let fields = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                if !line.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return None;
                }
                if let Some(j) = line.find("=") {
                    let key = &line[..j];
                    Some((key.to_string(), i))
                } else {
                    None
                }
            })
            .collect();
        Self { lines, fields }
    }

    pub async fn from_path<Fs: FsProvider, P: AsRef<Path>>(fs: &Fs, path: P) -> Result<Self> {
        let contents = fs.fs().read_to_string(path).await?;
        Ok(Self::new(contents))
    }

    pub fn from_path_sync<Fs: FsProvider, P: AsRef<Path>>(fs: &Fs, path: P) -> Result<Self> {
        let contents = fs.fs().read_to_string_sync(path)?;
        Ok(Self::new(contents))
    }

    pub fn get_field(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .and_then(|i| self.lines.get(*i))
            .and_then(|line| line.as_str().split_once("="))
            .map(|(_, v)| v)
    }

    pub fn set_field(&mut self, key: &str, value: &str) {
        let to_add = format!("{}={}", key, value);
        match self.fields.get(key) {
            Some(i) => self.lines[*i] = to_add,
            None => {
                self.lines.push(to_add);
                self.fields.insert(key.to_string(), self.lines.len() - 1);
            },
        }
    }
}

/// Represents an XDG Autostart entry that symlinks to a desktop entry.
#[derive(Debug, Clone)]
pub struct AutostartIntegration<'a, Ctx> {
    ctx: &'a Ctx,
    /// Path to the desktop entry to symlink to.
    target: PathBuf,
}

impl<'a, Ctx> AutostartIntegration<'a, Ctx>
where
    Ctx: FsProvider + EnvProvider + Sync,
{
    /// Creates a new [AutostartIntegration] that links to a local desktop entry if in an AppImage,
    /// and a global desktop entry otherwise.
    pub fn new(ctx: &'a Ctx) -> Result<Self> {
        if ctx.env().in_appimage() {
            Self::to_local(ctx)
        } else {
            Ok(Self::to_global(ctx))
        }
    }

    /// Creates a new [AutostartIntegration] that symlinks to a desktop entry installed locally
    /// (see [local_entry_path]).
    pub fn to_local(ctx: &'a Ctx) -> Result<Self> {
        let target = local_entry_path(ctx)?;
        Ok(Self { ctx, target })
    }

    /// Creates a new [AutostartIntegration] that symlinks to a desktop entry installed globally
    /// (see [global_entry_path]).
    pub fn to_global(ctx: &'a Ctx) -> Self {
        let target = global_entry_path(ctx);
        Self { ctx, target }
    }

    /// Helper to uninstall the autostart integration without constructing a local or global
    /// variant.
    pub async fn uninstall(ctx: &'a Ctx) -> Result<()> {
        // local/global doesn't matter
        Self::to_global(ctx).uninstall().await
    }
}

#[async_trait]
impl<Ctx> Integration for AutostartIntegration<'_, Ctx>
where
    Ctx: FsProvider + EnvProvider + Sync,
{
    fn describe(&self) -> String {
        "Desktop Autostart Entry Integration".to_owned()
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        let fs = self.ctx.fs();
        let to_autostart_path = local_autostart_path(self.ctx)?;
        create_parent(fs, &to_autostart_path).await?;
        if fs.symlink_exists(&to_autostart_path).await {
            fs.remove_file(&to_autostart_path).await?;
        }
        fs.symlink(&self.target, &to_autostart_path).await?;
        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        let fs = self.ctx.fs();
        let to_autostart_path = local_autostart_path(self.ctx)?;
        if fs.symlink_exists(&to_autostart_path).await {
            fs.remove_file(&to_autostart_path).await?;
        }
        Ok(())
    }

    async fn is_installed(&self) -> Result<()> {
        let fs = self.ctx.fs();
        let to_autostart_path = local_autostart_path(self.ctx)?;
        if !fs.exists(&to_autostart_path) {
            return Err(Error::FileDoesNotExist(to_autostart_path.clone().into()));
        }
        let read_path = fs.read_link(&to_autostart_path).await?;
        if read_path != self.target {
            Err(Error::ImproperInstallation(
                format!("Unexpected link path: {}", read_path.to_string_lossy()).into(),
            ))
        } else {
            Ok(())
        }
    }
}

/// Whether or not the [`AutostartIntegration`] should be installed according to the user's
/// environment and settings.
pub fn should_install_autostart_entry<Ctx: EnvProvider>(env: &Ctx, settings: &Settings, state: &State) -> bool {
    if env.env().in_appimage() && !state.get_bool_or("appimage.manageDesktopEntry", false) {
        return false;
    }
    settings.get_bool_or("app.launchOnStartup", true)
}

#[cfg(test)]
mod tests {
    use fig_os_shim::{
        Context,
        ContextBuilder,
        Env,
    };

    use super::*;

    const TEST_DESKTOP_ENTRY: &str = r#"[Desktop Entry]
Categories=Development;
Exec=q-desktop
Icon=q-desktop
Name=q_desktop
Terminal=false
Type=Application"#;

    const TEST_EXEC_VALUE: &str = "/app.appimage";

    #[tokio::test]
    async fn test_entry_contents() {
        let mut contents =
            EntryContents::new("[Desktop Entry]\n# Some Comment\nExec=testapp\nIcon=testapp.png".to_string());
        assert_eq!(contents.get_field("Exec"), Some("testapp"));
        assert_eq!(contents.get_field("Icon"), Some("testapp.png"));
        contents.set_field("Icon", "/path/img.png");
        assert_eq!(
            contents.to_string(),
            "[Desktop Entry]\n# Some Comment\nExec=testapp\nIcon=/path/img.png"
        );
    }

    async fn make_test_local_desktop_entry(ctx: &Context) -> DesktopEntryIntegration<'_, Context> {
        let fs = ctx.fs();
        fs.write("/app.desktop", TEST_DESKTOP_ENTRY).await.unwrap();
        fs.write("/app.png", "image").await.unwrap();
        DesktopEntryIntegration::new(ctx, Some("/app.desktop"), Some("/app.png"), Some(TEST_EXEC_VALUE))
    }

    #[tokio::test]
    async fn test_desktop_entry_integration_install_and_uninstall() {
        let ctx = ContextBuilder::new().with_test_home().await.unwrap().build();
        let fs = ctx.fs();
        let integration = make_test_local_desktop_entry(&ctx).await;
        assert!(integration.is_installed().await.is_err());

        // Test install.
        integration.install().await.unwrap();

        // Validating it was installed.
        assert!(integration.is_installed().await.is_ok());
        let installed_entry_path = local_entry_path(&ctx).unwrap();
        let installed_icon_path = local_icon_path(ctx.fs()).unwrap();
        assert!(
            fs.exists(&installed_entry_path),
            "desktop entry should have been created"
        );
        assert_eq!(
            fs.read_to_string(&installed_icon_path).await.unwrap(),
            "image",
            "image should have been created"
        );

        // Validating the content of the desktop entry
        let entry_contents = EntryContents::from_path(fs, &installed_entry_path).await.unwrap();
        let actual_exec = entry_contents.get_field("Exec").unwrap();
        assert_eq!(actual_exec, TEST_EXEC_VALUE, "invalid Exec field");
        let actual_icon = entry_contents.get_field("Icon").unwrap();
        assert_eq!(actual_icon, installed_icon_path.to_string_lossy(), "invalid Icon field");

        // Test uninstall.
        integration.uninstall().await.unwrap();

        // Validating it was uninstalled.
        assert!(integration.is_installed().await.is_err());
        assert!(
            !fs.exists(installed_entry_path),
            "installed desktop entry should have been deleted"
        );
        assert!(
            !fs.exists(installed_icon_path),
            "installed icon should have been deleted"
        );
        assert!(
            integration.uninstall().await.is_ok(),
            "should be able to uninstall repeatedly without erroring"
        );
    }

    #[tokio::test]
    async fn test_new_autostart_integration() {
        let ctx = Context::builder()
            .with_test_home()
            .await
            .unwrap()
            .with_env_var("APPIMAGE", "/app.appimage")
            .build_fake();
        let integration = AutostartIntegration::new(&ctx).unwrap();
        assert_eq!(integration.target, local_entry_path(&ctx).unwrap());

        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let integration = AutostartIntegration::new(&ctx).unwrap();
        assert_eq!(integration.target, global_entry_path(&ctx));
    }

    #[tokio::test]
    async fn test_autostart_integration_install_and_uninstall() {
        let ctx = ContextBuilder::new().with_test_home().await.unwrap().build_fake();

        // Create desktop entries both locally and globally
        {
            let local_path = local_entry_path(&ctx).unwrap();
            let global_path = global_entry_path(&ctx);
            ctx.fs().create_dir_all(local_path.parent().unwrap()).await.unwrap();
            ctx.fs().write(local_path, "[Desktop Entry]").await.unwrap();
            ctx.fs().create_dir_all(global_path.parent().unwrap()).await.unwrap();
            ctx.fs().write(global_path, "[Desktop Entry]").await.unwrap();
        }

        let local_autostart = AutostartIntegration::to_local(&ctx).unwrap();
        let global_autostart = AutostartIntegration::to_global(&ctx);

        // Not installed by default.
        assert!(local_autostart.is_installed().await.is_err());
        assert!(global_autostart.is_installed().await.is_err());

        // Test install.
        local_autostart.install().await.unwrap();
        local_autostart.is_installed().await.unwrap();
        assert!(local_autostart.is_installed().await.is_ok());
        let local_desktop_entry = local_entry_path(&ctx).unwrap();
        let installed_autostart_path = local_autostart_path(&ctx).unwrap();
        assert_eq!(
            ctx.fs().read_link(&installed_autostart_path).await.unwrap(),
            local_desktop_entry
        );

        // Test installing globally will overwrite the local install.
        global_autostart.install().await.unwrap();
        assert_eq!(
            ctx.fs().read_link(&installed_autostart_path).await.unwrap(),
            global_entry_path(&ctx)
        );
        assert!(global_autostart.is_installed().await.is_ok());
        assert!(local_autostart.is_installed().await.is_err());

        // Test uninstall.
        global_autostart.uninstall().await.unwrap();
        assert!(global_autostart.is_installed().await.is_err());
        assert!(!ctx.fs().symlink_exists(&installed_autostart_path).await);
        assert!(
            global_autostart.uninstall().await.is_ok(),
            "should be able to uninstall repeatedly without erroring"
        );
    }

    #[test]
    fn test_should_install_autostart_entry() {
        // Test structure: (env, settings, state, expected_result, test_name)
        let testcases = &[
            (vec![], vec![], vec![], true, "Default should install"),
            (
                vec![],
                vec![("app.launchOnStartup", true.into())],
                vec![],
                true,
                "Install if launchOnStartup is true",
            ),
            (
                vec![],
                vec![("app.launchOnStartup", false.into())],
                vec![],
                false,
                "Don't install if launchOnStartup is false",
            ),
            (
                vec![("APPIMAGE", "/app.appimage")],
                vec![],
                vec![],
                false,
                "AppImage should not install without user permission",
            ),
            (
                vec![("APPIMAGE", "/app.appimage")],
                vec![],
                vec![("appimage.manageDesktopEntry", true.into())],
                true,
                "AppImage should install by default if user has granted permission",
            ),
            (
                vec![("APPIMAGE", "/app.appimage")],
                vec![("app.launchOnStartup", true.into())],
                vec![("appimage.manageDesktopEntry", false.into())],
                false,
                "AppImage should not install if user has removed permission",
            ),
        ];
        for test in testcases {
            let (env, settings, state, expected, message) = test;
            let env = Env::from_slice(env);
            let settings = Settings::from_slice(settings);
            let state = State::from_slice(state);
            assert_eq!(
                should_install_autostart_entry(&env, &settings, &state),
                *expected,
                "{}",
                message
            );
        }
    }
}
