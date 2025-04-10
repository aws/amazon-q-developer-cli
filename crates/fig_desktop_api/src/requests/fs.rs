use camino::Utf8PathBuf;
use fig_os_shim::{
    Env,
    Fs,
};
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
use crate::util::{
    build_filepath,
    resolve_filepath,
};

pub async fn read_file(request: ReadFileRequest, env: &Env, fs: &Fs) -> RequestResult {
    use fig_proto::fig::read_file_response::Type;
    let path = request.path.as_ref().ok_or("No path provided")?;
    let resolved_path = resolve_filepath(path, env);
    let kind = if request.is_binary_file() {
        Type::Data(
            fs.read(&*resolved_path)
                .await
                .map_err(|err| format!("Failed reading file at {resolved_path}: {err}"))?,
        )
    } else {
        Type::Text(
            fs.read_to_string(&*resolved_path)
                .await
                .map_err(|err| format!("Failed reading file at {resolved_path}: {err}"))?,
        )
    };
    let response = ServerOriginatedSubMessage::ReadFileResponse(ReadFileResponse { r#type: Some(kind) });

    Ok(response.into())
}

pub async fn write_file(request: WriteFileRequest, env: &Env, fs: &Fs) -> RequestResult {
    use fig_proto::fig::write_file_request::Data;
    let path = request.path.as_ref().ok_or_else(|| "No path provided".to_string())?;
    let resolved_path = resolve_filepath(path, env);
    match request.data.unwrap() {
        Data::Binary(data) => fs
            .write(&*resolved_path, data)
            .await
            .map_err(|err| format!("Failed writing to file at {resolved_path}: {err}"))?,
        Data::Text(data) => fs
            .write(&*resolved_path, data.as_bytes())
            .await
            .map_err(|err| format!("Failed writing to file at {resolved_path}: {err}"))?,
    }

    RequestResult::success()
}

pub async fn append_to_file(request: AppendToFileRequest, env: &Env) -> RequestResult {
    use fig_proto::fig::append_to_file_request::Data;
    let path = request.path.as_ref().ok_or_else(|| "No path provided".to_string())?;
    let resolved_path = resolve_filepath(path, env);
    let mut file = OpenOptions::new()
        .append(true)
        .open(&*resolved_path)
        .await
        .map_err(|err| format!("Failed opening file at {resolved_path}: {err}"))?;

    match request.data.unwrap() {
        Data::Binary(data) => file
            .write(&data)
            .await
            .map_err(|err| format!("Failed writing to file at {resolved_path}: {err}"))?,
        Data::Text(data) => file
            .write(data.as_bytes())
            .await
            .map_err(|err| format!("Failed writing to file at {resolved_path}: {err}"))?,
    };

    RequestResult::success()
}

pub async fn destination_of_symbolic_link(request: DestinationOfSymbolicLinkRequest, env: &Env) -> RequestResult {
    let path = request.path.as_ref().ok_or("No path provided")?;
    let resolved_path = resolve_filepath(path, env);
    let real_path: Utf8PathBuf = tokio::fs::canonicalize(&*resolved_path)
        .await
        .map_err(|err| format!("Failed resolving symlink at {resolved_path}: {err}"))?
        .try_into()
        .map_err(|err| format!("Path is not Utf8: {err}"))?;

    let response = ServerOriginatedSubMessage::DestinationOfSymbolicLinkResponse(DestinationOfSymbolicLinkResponse {
        destination: Some(build_filepath(real_path)),
    });

    Ok(response.into())
}

pub async fn contents_of_directory(request: ContentsOfDirectoryRequest, env: &Env) -> RequestResult {
    let path = request.directory.as_ref().ok_or("No path provided")?;
    let resolved_path = resolve_filepath(path, env);
    let mut stream = tokio::fs::read_dir(&*resolved_path)
        .await
        .map_err(|err| format!("Failed listing directory in {resolved_path}: {err}"))?;

    let mut contents = Vec::new();
    while let Some(item) = stream
        .next_entry()
        .await
        .map_err(|err| format!("Failed listing directory entries in {resolved_path}: {err}"))?
    {
        contents.push(item.file_name().to_string_lossy().to_string());
    }

    let response =
        ServerOriginatedSubMessage::ContentsOfDirectoryResponse(ContentsOfDirectoryResponse { file_names: contents });

    Ok(response.into())
}

pub async fn create_directory_request(
    request: fig_proto::fig::CreateDirectoryRequest,
    env: &Env,
    fs: &Fs,
) -> RequestResult {
    let path = request.path.as_ref().ok_or("No path provided")?;
    let resolved_path = resolve_filepath(path, env);
    if request.recursive() {
        fs.create_dir_all(&*resolved_path)
            .await
            .map_err(|err| format!("Failed to create dir: {err}"))?;
    } else {
        fs.create_dir(&*resolved_path)
            .await
            .map_err(|err| format!("Failed to create dir: {err}"))?;
    }

    RequestResult::success()
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use fig_proto::fig::{
        FilePath,
        ReadFileRequest,
        WriteFileRequest,
    };

    use super::*;

    #[tokio::test]
    async fn test_read_file() {
        use fig_proto::fig::read_file_response::Type;

        let fs = Fs::new_fake();
        let env = Env::new();

        let path = Utf8PathBuf::from("/test.txt");
        let content = "Hello, World!";
        fs.write(&path, content.as_bytes()).await.unwrap();

        let mut request = ReadFileRequest {
            path: Some(FilePath {
                path: path.to_string(),
                relative_to: None,
                expand_tilde_in_path: None,
            }),
            ..Default::default()
        };

        let result = read_file(request.clone(), &env, &fs).await.unwrap();
        let text = match *result {
            ServerOriginatedSubMessage::ReadFileResponse(ReadFileResponse {
                r#type: Some(Type::Text(text)),
            }) => text,
            _ => panic!("Unexpected response type"),
        };
        assert_eq!(content, text);

        request.is_binary_file = Some(true);

        let result = read_file(request, &env, &fs).await.unwrap();
        let data = match *result {
            ServerOriginatedSubMessage::ReadFileResponse(ReadFileResponse {
                r#type: Some(Type::Data(data)),
            }) => data,
            _ => panic!("Unexpected response type"),
        };
        assert_eq!(content.as_bytes(), &data[..]);
    }

    #[tokio::test]
    async fn test_write_file() {
        use fig_proto::fig::write_file_request::Data;

        let fs = Fs::new_fake();
        let env = Env::new();

        let path = Utf8PathBuf::from("/test.txt");
        let content = "Hello, World!";

        let mut request = WriteFileRequest {
            path: Some(FilePath {
                path: path.to_string(),
                relative_to: None,
                expand_tilde_in_path: None,
            }),
            data: Some(Data::Text(content.to_string())),
        };

        write_file(request.clone(), &env, &fs).await.unwrap();

        let read_content = fs.read_to_string(&path).await.unwrap();
        assert_eq!(content, read_content);

        request.data = Some(Data::Binary(content.as_bytes().to_vec()));
        write_file(request, &env, &fs).await.unwrap();

        let read_content = fs.read(&path).await.unwrap();
        assert_eq!(content.as_bytes(), &read_content[..]);
    }
}
