#![no_main]

extern crate fig_ipc;
extern crate libfuzzer_sys;

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::LazyLock;

use fig_ipc::{
    BufferedUnixStream,
    RecvMessage,
    SendMessage,
};
use fig_proto::local::LocalMessage;
use libfuzzer_sys::fuzz_target;
use tokio::net::UnixListener;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

static DIRSOCK: LazyLock<(tempfile::TempDir, PathBuf)> = LazyLock::new(|| {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("test.sock");
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path.parent().unwrap(), Permissions::from_mode(0o700)).unwrap();
    }

    (temp_dir, socket_path)
});

static LISTENER: LazyLock<UnixListener> = LazyLock::new(|| UnixListener::bind(&DIRSOCK.1).unwrap());

fuzz_target!(|input: LocalMessage| {
    RUNTIME.block_on(fuzz(input));
});

async fn fuzz(input: LocalMessage) {
    let _ = LISTENER.deref();
    let join = tokio::spawn(async move {
        let (stream, _) = LISTENER.accept().await.unwrap();
        let mut stream = BufferedUnixStream::new(stream);
        stream.recv_message::<LocalMessage>().await.unwrap();
    });

    let mut stream = fig_ipc::socket_connect(&DIRSOCK.1).await.unwrap();
    stream.send_message(input).await.unwrap();

    join.await.unwrap();
}
