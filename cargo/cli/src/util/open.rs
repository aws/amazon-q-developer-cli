use cfg_if::cfg_if;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Failed to open URL")]
    Failed,
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn open_macos(url_str: impl AsRef<str>) -> Result<(), Error> {
    use objc2::ClassType;
    use objc2_foundation::{
        NSString,
        NSURL,
    };

    let url_nsstring = NSString::from_str(url_str.as_ref());
    let nsurl = unsafe { NSURL::initWithString(NSURL::alloc(), &url_nsstring) }.ok_or(Error::Failed)?;
    let res = unsafe { objc2_app_kit::NSWorkspace::sharedWorkspace().openURL(&nsurl) };
    res.then_some(()).ok_or(Error::Failed)
}

#[cfg(target_os = "windows")]
fn open_command(url: impl AsRef<str>) -> std::process::Command {
    use std::os::windows::process::CommandExt;

    let detached = 0x8;
    let mut command = std::process::Command::new("cmd");
    command.creation_flags(detached);
    command.args(["/c", "start", url.as_ref()]);
    command
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn open_command(url: impl AsRef<str>) -> std::process::Command {
    let executable = if super::system_info::in_wsl() {
        "wslview"
    } else {
        "xdg-open"
    };

    let mut command = std::process::Command::new(executable);
    command.arg(url.as_ref());
    command
}

/// Returns bool indicating whether the URL was opened successfully
#[allow(dead_code)]
pub fn open_url(url: impl AsRef<str>) -> Result<(), Error> {
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            open_macos(url)
        } else {
            match open_command(url).output() {
                Ok(output) => {
                    tracing::trace!(?output, "open_url output");
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(Error::Failed)
                    }
                },
                Err(err) => Err(err.into()),
            }
        }
    }
}

/// Returns bool indicating whether the URL was opened successfully
pub async fn open_url_async(url: impl AsRef<str>) -> Result<(), Error> {
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            open_macos(url)
        } else {
            match tokio::process::Command::from(open_command(url)).output().await {
                Ok(output) => {
                    tracing::trace!(?output, "open_url_async output");
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(Error::Failed)
                    }
                },
                Err(err) => Err(err.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn test_open_url() {
        open_url("https://fig.io").unwrap();
    }
}
