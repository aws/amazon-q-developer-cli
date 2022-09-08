use anyhow::anyhow;
use camino::Utf8PathBuf;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    AppendToFileRequest,
    ContentsOfDirectoryRequest,
    ContentsOfDirectoryResponse,
    DestinationOfSymbolicLinkRequest,
    DestinationOfSymbolicLinkResponse,
    ReadFileRequest,
    ReadFileResponse,
    WriteFileRequest,
};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use super::{
    RequestResult,
    RequestResultImpl,
};
use crate::utils::{
    build_filepath,
    resolve_filepath,
};

pub async fn read_file(request: ReadFileRequest) -> RequestResult {
    use fig_proto::fig::read_file_response::Type;
    let path = request.path.as_ref().ok_or_else(|| anyhow!("No path provided"))?;
    let resolved_path = resolve_filepath(path);
    let kind = if request.is_binary_file() {
        Type::Data(
            tokio::fs::read(&*resolved_path)
                .await
                .map_err(|err| anyhow!("Failed reading file at {resolved_path}: {err}"))?
                .into(),
        )
    } else {
        Type::Text(
            tokio::fs::read_to_string(&*resolved_path)
                .await
                .map_err(|err| anyhow!("Failed reading file at {resolved_path}: {err}"))?,
        )
    };
    let response = ServerOriginatedSubMessage::ReadFileResponse(ReadFileResponse { r#type: Some(kind) });

    Ok(response.into())
}

pub async fn write_file(request: WriteFileRequest) -> RequestResult {
    use fig_proto::fig::write_file_request::Data;
    let path = request.path.as_ref().ok_or_else(|| anyhow!("No path provided"))?;
    let resolved_path = resolve_filepath(path);
    match request.data.unwrap() {
        Data::Binary(data) => tokio::fs::write(&*resolved_path, data)
            .await
            .map_err(|err| anyhow!("Failed writing to file at {resolved_path}: {err}"))?,
        Data::Text(data) => tokio::fs::write(&*resolved_path, data.as_bytes())
            .await
            .map_err(|err| anyhow!("Failed writing to file at {resolved_path}: {err}"))?,
    }

    RequestResult::success()
}

pub async fn append_to_file(request: AppendToFileRequest) -> RequestResult {
    use fig_proto::fig::append_to_file_request::Data;
    let path = request.path.as_ref().ok_or_else(|| anyhow!("No path provided"))?;
    let resolved_path = resolve_filepath(path);
    let mut file = OpenOptions::new()
        .append(true)
        .open(&*resolved_path)
        .await
        .map_err(|err| anyhow!("Failed opening file at {resolved_path}: {err}"))?;

    match request.data.unwrap() {
        Data::Binary(data) => file
            .write(&data)
            .await
            .map_err(|err| anyhow!("Failed writing to file at {resolved_path}: {err}"))?,
        Data::Text(data) => file
            .write(data.as_bytes())
            .await
            .map_err(|err| anyhow!("Failed writing to file at {resolved_path}: {err}"))?,
    };

    RequestResult::success()
}

pub async fn destination_of_symbolic_link(request: DestinationOfSymbolicLinkRequest) -> RequestResult {
    let path = request.path.as_ref().ok_or_else(|| anyhow!("No path provided"))?;
    let resolved_path = resolve_filepath(path);
    let real_path: Utf8PathBuf = tokio::fs::canonicalize(&*resolved_path)
        .await
        .map_err(|err| anyhow!("Failed resolving symlink at {resolved_path}: {err}"))?
        .try_into()?;

    let response = ServerOriginatedSubMessage::DestinationOfSymbolicLinkResponse(DestinationOfSymbolicLinkResponse {
        destination: Some(build_filepath(real_path)),
    });

    Ok(response.into())
}

pub async fn contents_of_directory(request: ContentsOfDirectoryRequest) -> RequestResult {
    let path = request.directory.as_ref().ok_or_else(|| anyhow!("No path provided"))?;
    let resolved_path = resolve_filepath(path);
    let mut stream = tokio::fs::read_dir(&*resolved_path)
        .await
        .map_err(|err| anyhow!("Failed listing directory in {resolved_path}: {err}"))?;

    let mut contents = Vec::new();
    while let Some(item) = stream
        .next_entry()
        .await
        .map_err(|err| anyhow!("Failed listing directory entries in {resolved_path}: {err}"))?
    {
        contents.push(item.file_name().to_string_lossy().to_string());
    }

    let response =
        ServerOriginatedSubMessage::ContentsOfDirectoryResponse(ContentsOfDirectoryResponse { file_names: contents });

    Ok(response.into())
}

pub async fn create_directory_request(request: fig_proto::fig::CreateDirectoryRequest) -> RequestResult {
    let path = request.path.as_ref().ok_or_else(|| anyhow!("No path provided"))?;
    let resolved_path = resolve_filepath(path);
    if request.recursive() {
        tokio::fs::create_dir_all(&*resolved_path).await?;
    } else {
        tokio::fs::create_dir(&*resolved_path).await?;
    }

    RequestResult::success()
}
