use std::io::{
    Write,
    stdout,
};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{
    Context,
    Result,
};
use fig_proto::local::{
    EditBufferHook,
    InterceptedKeyHook,
    PostExecHook,
    PreExecHook,
    PromptHook,
    ShellContext,
};
use fig_proto::remote::clientbound;
use fig_remote_ipc::RemoteHookHandler;
use fig_remote_ipc::figterm::FigtermState;
use fig_util::RUNTIME_DIR_NAME;
use portable_pty::{
    Child,
    CommandBuilder,
    PtyPair,
    PtySize,
    native_pty_system,
};
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct RemoteHook {
    buffer: Arc<Mutex<Option<String>>>,
    shell_context: Arc<Mutex<Option<ShellContext>>>,
}

#[async_trait::async_trait]
impl RemoteHookHandler for RemoteHook {
    type Error = anyhow::Error;

    async fn edit_buffer(
        &mut self,
        edit_buffer_hook: &EditBufferHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>, Self::Error> {
        *self.buffer.lock().await = Some(edit_buffer_hook.text.clone());
        Ok(None)
    }

    async fn prompt(
        &mut self,
        _prompt_hook: &PromptHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>, Self::Error> {
        Ok(None)
    }

    async fn pre_exec(
        &mut self,
        _pre_exec_hook: &PreExecHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>, Self::Error> {
        Ok(None)
    }

    async fn post_exec(
        &mut self,
        _post_exec_hook: &PostExecHook,
        _session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>, Self::Error> {
        Ok(None)
    }

    async fn intercepted_key(
        &mut self,
        _intercepted_key: InterceptedKeyHook,
        _session_id: Uuid,
    ) -> Result<Option<clientbound::response::Response>, Self::Error> {
        Ok(None)
    }

    async fn shell_context(&mut self, context: &ShellContext, _session_id: Uuid) {
        *self.shell_context.lock().await = Some(context.clone());
    }
}

pub struct Shell {
    // pub remote_socket: UnixListener,
    pub desktop_socket: UnixListener,

    pub pty_pair: PtyPair,
    pub writer: Box<dyn std::io::Write + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    pub tempdir: TempDir,

    pub buffer: Arc<Mutex<Option<String>>>,
    pub shell_context: Arc<Mutex<Option<ShellContext>>>,
}

impl Shell {
    pub async fn init(shell: &str) -> Result<Shell> {
        let tempdir = TempDir::new()?;
        println!("{tempdir:?}");

        let figterm_state = Arc::new(FigtermState::new());

        let runtime_dir = tempdir.path().join(RUNTIME_DIR_NAME);
        tokio::fs::create_dir_all(&runtime_dir).await?;
        tokio::fs::set_permissions(&runtime_dir, std::fs::Permissions::from_mode(0o700)).await?;

        println!("{runtime_dir:?} {}", runtime_dir.to_str().unwrap().len());

        let path = runtime_dir.join("remote.sock");
        let buffer = Arc::new(Mutex::new(None));
        let shell_context = Arc::new(Mutex::new(None));
        tokio::spawn({
            let buffer = buffer.clone();
            let shell_context = shell_context.clone();
            async move {
                fig_remote_ipc::remote::start_remote_ipc(path, figterm_state.clone(), RemoteHook {
                    buffer,
                    shell_context,
                })
                .await
                .unwrap();
            }
        });

        let desktop_socket =
            UnixListener::bind(runtime_dir.join("desktop.sock")).context("Failed to make desktop.socket")?;

        let pty_system = native_pty_system();

        // Create a new pty
        let pty_pair = pty_system.openpty(PtySize {
            cols: 80,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Spawn a shell into the pty
        let mut cmd = CommandBuilder::new(shell);

        let session_id = "1234";
        cmd.env("Q_NEW_SESSION", "1");
        cmd.env("MOCK_QTERM_SESSION_ID", session_id);
        cmd.env("TMPDIR", tempdir.path());
        cmd.env("XDG_RUNTIME_DIR", tempdir.path());

        let child = pty_pair.slave.spawn_command(cmd)?;
        let writer = pty_pair.master.take_writer()?;

        let mut res = pty_pair.master.try_clone_reader().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0; 1024];
            while let Ok(a) = res.read(&mut buf) {
                if a == 0 {
                    break;
                }
                stdout().write_all(&buf[..a]).unwrap();
                stdout().flush().unwrap();
            }
        });

        // give time for shell to spawn
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(Shell {
            desktop_socket,

            pty_pair,
            writer,
            child,
            tempdir,

            buffer,
            shell_context,
        })
    }

    pub fn write(&mut self, data: &str) -> Result<()> {
        self.writer.write_all(data.as_bytes())?;
        self.writer.flush()?;

        Ok(())
    }

    pub async fn typed(&mut self, text: &str) -> Result<()> {
        println!("tying: {text}");
        for c in text.chars() {
            self.writer.write_all(c.to_string().as_bytes())?;
            self.writer.flush()?;

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.pty_pair.master.resize(PtySize {
            cols,
            rows,
            ..Default::default()
        })?;
        Ok(())
    }

    pub async fn reset(&mut self) -> Result<()> {
        self.typed("\n").await?;
        self.resize(80, 24)?;
        Ok(())
    }

    pub async fn buffer(&mut self) -> Option<String> {
        self.buffer.lock().await.clone()
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}
