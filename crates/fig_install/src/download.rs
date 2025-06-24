use std::borrow::BorrowMut as _;
use std::path::Path;
use std::time::Duration;

use hex::encode;
use reqwest::IntoUrl;
use tokio::io::AsyncWriteExt as _;
use tokio::sync::mpsc::Sender;

use crate::{
    Error,
    UpdateStatus,
};

#[allow(dead_code)]
pub(crate) async fn download_file(
    src: impl IntoUrl,
    dst: impl AsRef<Path>,
    size: u64,
    tx: Option<Sender<UpdateStatus>>,
) -> Result<String, Error> {
    let client = fig_request::client().expect("fig_request client must be instantiated on first request");
    let mut response = client.get(src).timeout(Duration::from_secs(30 * 60)).send().await?;

    let mut bytes_downloaded = 0;
    let mut file = tokio::fs::File::create(&dst).await?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);

    while let Some(mut bytes) = response.chunk().await? {
        bytes_downloaded += bytes.len() as u64;

        ctx.update(&bytes);

        if let Some(tx) = &tx {
            tx.send(UpdateStatus::Percent(bytes_downloaded as f32 / size as f32 * 100.0))
                .await
                .ok();

            tx.send(UpdateStatus::Message(format!(
                "Downloading ({:.2}/{:.2} MB)",
                bytes_downloaded as f32 / 1_000_000.0,
                size as f32 / 1_000_000.0
            )))
            .await
            .ok();
        }

        file.write_all_buf(bytes.borrow_mut()).await?;
    }

    if let Some(tx) = &tx {
        tx.send(UpdateStatus::Percent(100.0)).await.ok();
    }

    let hex_digest = encode(ctx.finish());
    Ok(hex_digest)
}
