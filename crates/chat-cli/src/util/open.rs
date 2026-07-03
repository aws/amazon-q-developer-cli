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

#[cfg(not(target_os = "macos"))]
const OPEN_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Returns bool indicating whether the URL was opened successfully
#[allow(dead_code)]
pub fn open_url(url: impl AsRef<str>) -> Result<(), Error> {
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            open_macos(url)
        } else {
            use std::process::Stdio;

            let mut child = open_command(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let deadline = std::time::Instant::now() + OPEN_EXIT_TIMEOUT;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::trace!(?status, "open_url exited");
                        return if status.success() { Ok(()) } else { Err(Error::Failed) };
                    },
                    Ok(None) if std::time::Instant::now() >= deadline => {
                        tracing::trace!("open_url did not exit within timeout, assuming success");
                        return Ok(());
                    },
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                    Err(err) => return Err(err.into()),
                }
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
            use std::process::Stdio;

            let mut child = tokio::process::Command::from(open_command(url))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            match tokio::time::timeout(OPEN_EXIT_TIMEOUT, child.wait()).await {
                Ok(Ok(status)) => {
                    tracing::trace!(?status, "open_url_async exited");
                    if status.success() { Ok(()) } else { Err(Error::Failed) }
                },
                Ok(Err(err)) => Err(err.into()),
                Err(_) => {
                    tracing::trace!("open_url_async did not exit within timeout, assuming success");
                    Ok(())
                },
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
