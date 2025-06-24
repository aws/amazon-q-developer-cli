use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use dbus::gnome_shell::ShellExtensions;
use fig_integrations::Integration;
use fig_integrations::desktop_entry::{
    AutostartIntegration,
    DesktopEntryIntegration,
};
use fig_integrations::gnome_extension::GnomeExtensionIntegration;
use fig_os_shim::Context;
use fig_util::directories::{
    fig_data_dir_ctx,
    local_webview_data_dir,
};
use fig_util::manifest::manifest;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use tokio::sync::mpsc::Sender;
use tracing::{
    debug,
    error,
    warn,
};
use url::Url;

use crate::download::download_file;
use crate::index::UpdatePackage;
use crate::{
    Error,
    UpdateStatus,
};

macro_rules! bail {
    ($($arg:tt)*) => {
        return Err(Error::UpdateFailed(format!($($arg)*)))
    };
}

#[derive(Debug, PartialEq, Eq)]
struct ArchiveParser<'a> {
    /// The name of the archive file, e.g. `foo-x86_64-linux.tar.zst`
    file_name: &'a str,
    /// The prefix of the archive file, e.g. `foo-x86_64-linux`
    file_prefix: &'a str,
    /// The name of the binary, e.g. `foo`
    name: &'a str,
    /// The architecture of the binary, e.g. `x86_64`
    arch: Option<&'a str>,
    /// The operating system of the binary, e.g. `linux`
    os: Option<&'a str>,
}

impl<'a> ArchiveParser<'a> {
    fn from_url(url: &'a Url) -> Result<Self, Error> {
        let url = url.path();

        let Some((_, file_name)) = url.rsplit_once('/') else {
            return Err(Error::UpdateFailed(format!("Invalid archive URL: {url}")));
        };

        let Some((file_prefix, _)) = file_name.split_once('.') else {
            return Err(Error::UpdateFailed(format!("Invalid archive name: {file_name}")));
        };

        let mut components = file_prefix.split('-');

        let Some(name) = components.next() else {
            return Err(Error::UpdateFailed(format!("Invalid archive name: {file_name}")));
        };

        let arch = components.next();
        let os = components.next();

        Ok(Self {
            file_name,
            file_prefix,
            name,
            arch,
            os,
        })
    }
}

fn extract_archive(archive_path: &Path, tempdir: &Path) -> Result<(), Error> {
    let file = std::fs::File::open(archive_path)?;
    let mut decoder = zstd::Decoder::new(file)?;
    let mut tar = tar::Archive::new(&mut decoder);
    debug!("unpacking tar to directory: {:?}", tempdir);
    tar.unpack(tempdir)?;
    Ok(())
}

/// find binaries in the `bin_dir`` and move them to `$HOME/.local/bin`
async fn replace_bins(bin_dir: &Path) -> Result<(), Error> {
    let local_bin = fig_util::directories::home_local_bin()?;

    let mut res = Ok(());

    let mut read_bin_dir = tokio::fs::read_dir(bin_dir).await?;
    while let Ok(Some(bin)) = read_bin_dir.next_entry().await {
        let installed_bin_path = local_bin.join(bin.file_name());

        let _ = tokio::fs::remove_file(&installed_bin_path).await;
        if let Err(err) = tokio::fs::copy(bin.path(), installed_bin_path).await {
            if res.is_ok() {
                res = Err(err.into());
            }
        }
    }
    res
}

pub(crate) async fn update(
    update_package: UpdatePackage,
    tx: Sender<UpdateStatus>,
    interactive: bool,
    relaunch_dashboard: bool,
) -> Result<(), Error> {
    match &manifest().variant {
        fig_util::manifest::Variant::Full => update_full(update_package, tx, interactive, relaunch_dashboard).await,
        fig_util::manifest::Variant::Minimal => {
            update_minimal(update_package, tx, interactive, relaunch_dashboard).await
        },
        fig_util::manifest::Variant::Other(other) => Err(Error::UnsupportedVariant(other.clone())),
    }
}

pub(crate) async fn update_minimal(
    UpdatePackage {
        download_url,
        sha256,
        size,
        ..
    }: UpdatePackage,
    tx: Sender<UpdateStatus>,
    _interactive: bool,
    _relaunch_dashboard: bool,
) -> Result<(), Error> {
    // check if the current exe can update
    let exe_path = std::env::current_exe()?.canonicalize()?;
    let Some(exe_name) = exe_path.file_name().and_then(|s| s.to_str()) else {
        bail!("Failed to get name of current executable: {exe_path:?}")
    };
    let Some(exe_parent) = exe_path.parent() else {
        bail!("Failed to get parent of current executable: {exe_path:?}")
    };
    // canonicalize to handle if the home dir is a symlink (like on Dev Desktops)
    let local_bin = fig_util::directories::home_local_bin()?.canonicalize()?;

    if exe_parent != local_bin {
        bail!(
            "Update is only supported for binaries installed in {local_bin:?}, the current executable is in {exe_parent:?}"
        );
    }

    if exe_name != CLI_BINARY_NAME {
        bail!("Update is only supported for {CLI_BINARY_NAME:?}, the current executable is {exe_name:?}");
    }

    let tempdir = tempfile::tempdir()?;

    let archive = ArchiveParser::from_url(&download_url)?;

    let archive_path = tempdir.path().join(archive.file_name);

    debug!("downloading file: {:?} to path: {:?}", download_url, archive_path);
    let real_hash = download_file(download_url.clone(), &archive_path, size, Some(tx.clone())).await?;
    if sha256 != real_hash {
        return Err(Error::UpdateFailed(format!(
            "Hash mismatch for {}: expected {sha256}, got {real_hash}",
            archive.file_name
        )));
    }

    let tempdir_path = tempdir.path().to_owned();
    tokio::task::spawn_blocking(move || extract_archive(&archive_path, &tempdir_path))
        .await
        .map_err(|err| Error::UpdateFailed(format!("Failed to extract {}: {err}", archive.file_name)))??;

    let bin_dir = tempdir.path().join(archive.name).join("bin");
    replace_bins(&bin_dir).await?;

    Ok(())
}

pub(crate) async fn update_full(
    update_package: UpdatePackage,
    tx: Sender<UpdateStatus>,
    interactive: bool,
    relaunch_dashboard: bool,
) -> Result<(), Error> {
    update_full_ctx(&Context::new(), update_package, tx, interactive, relaunch_dashboard).await?;
    #[allow(clippy::exit)]
    std::process::exit(0);
}

async fn update_full_ctx(
    ctx: &Context,
    UpdatePackage {
        download_url,
        sha256: expected_hash,
        size,
        ..
    }: UpdatePackage,
    tx: Sender<UpdateStatus>,
    _interactive: bool,
    _relaunch_dashboard: bool,
) -> Result<(), Error> {
    if !ctx.env().in_appimage() {
        return Err(Error::UpdateFailed(
            "Updating is only supported from the AppImage".into(),
        ));
    }
    let current_appimage_path = ctx
        .env()
        .get("APPIMAGE")
        .map_err(|err| Error::UpdateFailed(format!("Unable to determine the path to the appimage: {:?}", err)))?;

    debug!("starting update");
    tx.send(UpdateStatus::Message("Downloading...".into())).await.ok();

    // Download the updated AppImage to a temporary location.

    let temp_dir = ctx.fs().create_tempdir().await?;
    let file_name = download_url
        .path_segments()
        .and_then(|path| path.last())
        .unwrap_or(PRODUCT_NAME);
    let download_path = temp_dir.path().join(file_name);

    // Security: set the permissions to 700 so that only the user can read and write
    ctx.fs()
        .set_permissions(temp_dir.path(), std::fs::Permissions::from_mode(0o700))
        .await?;

    debug!(?file_name, "Downloading update file");
    let real_hash = download_file(download_url, &download_path, size, Some(tx.clone())).await?;

    if real_hash != expected_hash {
        return Err(Error::UpdateFailed(format!(
            "file hash mismatch. Expected: {expected_hash}, Actual: {real_hash}"
        )));
    }

    tx.send(UpdateStatus::Message("Installing update...".into())).await.ok();

    ctx.fs()
        .set_permissions(&download_path, std::fs::Permissions::from_mode(0o755))
        .await?;

    debug!(?download_path, ?current_appimage_path, "Replacing the current AppImage");
    ctx.fs().rename(&download_path, &current_appimage_path).await?;
    debug!("Successfully swapped the AppImage");

    tx.send(UpdateStatus::Message("Relaunching...".into())).await.ok();

    std::process::Command::new(current_appimage_path).spawn()?;

    let lock_file_path = fig_util::directories::update_lock_path(ctx)?;
    debug!(?lock_file_path, "Removing lock file");
    if ctx.fs().exists(&lock_file_path) {
        ctx.fs()
            .remove_file(&lock_file_path)
            .await
            .map_err(|err| error!(?err, "Unable to remove the lock file"))
            .ok();
    }

    tx.send(UpdateStatus::Exit).await.ok();
    Ok(())
}

pub(crate) async fn uninstall_gnome_extension(
    ctx: &Context,
    shell_extensions: &ShellExtensions<Context>,
) -> Result<(), Error> {
    Ok(
        GnomeExtensionIntegration::new(ctx, shell_extensions, None::<&str>, None)
            .uninstall()
            .await?,
    )
}

pub(crate) async fn uninstall_desktop_entries(ctx: &Context) -> Result<(), Error> {
    DesktopEntryIntegration::new(ctx, None::<&str>, None, None)
        .uninstall()
        .await?;
    Ok(AutostartIntegration::uninstall(ctx).await?)
}

pub(crate) async fn uninstall_desktop(ctx: &Context) -> Result<(), Error> {
    let fs = ctx.fs();
    let data_dir_path = fig_data_dir_ctx(fs)?;
    if fs.exists(&data_dir_path) {
        fs.remove_dir_all(&data_dir_path)
            .await
            .map_err(|err| warn!(?err, "Failed to remove data dir"))
            .ok();
    }
    let webview_dir_path = local_webview_data_dir(ctx)?;
    if fs.exists(&webview_dir_path) {
        fs.remove_dir_all(&webview_dir_path)
            .await
            .map_err(|err| warn!(?err, "Failed to remove webview data dir"))
            .ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use fig_os_shim::Os;
    use fig_test_utils::TestServer;
    use fig_test_utils::http::Method;
    use fig_util::CLI_BINARY_NAME;
    use fig_util::directories::home_dir;
    use hex::ToHex;

    use super::*;

    #[test]
    fn test_archive_def_from_url() {
        let url = Url::parse("https://example.com/path/q-x86_64-linux.tar.zst").unwrap();
        let archive_name = ArchiveParser::from_url(&url).unwrap();
        assert_eq!(archive_name, ArchiveParser {
            file_name: "q-x86_64-linux.tar.zst",
            file_prefix: "q-x86_64-linux",
            name: "q",
            arch: Some("x86_64"),
            os: Some("linux"),
        });
    }

    fn print_tree(p: &Path) {
        for entry in std::fs::read_dir(p).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = std::fs::metadata(&path).unwrap();
            if metadata.is_dir() {
                print_tree(&path);
            } else {
                println!("{}", path.display());
            }
        }
    }

    #[tokio::test]
    #[ignore = "needs archive to test with"]
    async fn test_extract_archive() {
        let archive_path = home_dir().unwrap().join(format!("{CLI_BINARY_NAME}.tar.zst"));
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = tempdir.path();

        extract_archive(&archive_path, tempdir.path()).unwrap();
        print_tree(tempdir_path);

        let bin_dir = tempdir.path().join(CLI_BINARY_NAME).join("bin");
        replace_bins(&bin_dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_uninstall_desktop_removes_data_dir() {
        let ctx = Context::builder()
            .with_test_home()
            .await
            .unwrap()
            .with_os(Os::Linux)
            .build_fake();
        let fs = ctx.fs();
        let data_dir_path = fig_data_dir_ctx(fs).unwrap();
        fs.create_dir_all(&data_dir_path).await.unwrap();

        uninstall_desktop(&ctx).await.unwrap();

        assert!(!fs.exists(&data_dir_path));
    }

    #[tokio::test]
    async fn test_appimage_updates_successfully() {
        tracing_subscriber::fmt::try_init().ok();

        // Given
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let current_appimage_path = ctx.fs().chroot_path("/app.appimage");
        unsafe { ctx.env().set_var("APPIMAGE", &current_appimage_path) };

        let test_version = "9.9.9"; // update version
        let test_fname = "new.exe"; // file name to be downloaded
        let test_download_path = format!("/{}/{}", test_version, test_fname);
        let test_script_output_path = ctx.fs().chroot_path("/version");
        let test_file = format!(
            "#!/usr/bin/env sh\necho -n '{}' > '{}'",
            test_version,
            test_script_output_path.to_string_lossy()
        );
        // Create a test server that returns a test script that writes the expected version to a file when
        // executed.
        let test_server_addr = TestServer::new()
            .await
            .with_mock_response(Method::GET, test_download_path.clone(), test_file.clone())
            .spawn_listener();

        // When
        update_full_ctx(
            &ctx,
            UpdatePackage {
                version: semver::Version::from_str(test_version).unwrap(),
                download_url: Url::from_str(&format!("http://{}{}", test_server_addr, test_download_path)).unwrap(),
                sha256: ring::digest::digest(&ring::digest::SHA256, test_file.as_bytes()).encode_hex(),
                size: 0, // size not checked
                cli_path: None,
            },
            tokio::sync::mpsc::channel(999).0,
            false,
            true,
        )
        .await
        .unwrap();

        // Then
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            ctx.fs().read_to_string(&current_appimage_path).await.unwrap(),
            test_file,
            "The path to the current app image should have been replaced with the update file"
        );
        assert_eq!(
            ctx.fs().read_to_string(&test_script_output_path).await.unwrap(),
            test_version,
            "Expected the downloaded file to be executed"
        );
        assert!(
            !ctx.fs().exists(fig_util::directories::update_lock_path(&ctx).unwrap()),
            "Lock file should have been deleted"
        );
    }
}
