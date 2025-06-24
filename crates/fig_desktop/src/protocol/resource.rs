use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use fig_os_shim::Context;
use tracing::info;
use wry::http::header::CONTENT_TYPE;
use wry::http::{
    Request,
    Response,
    StatusCode,
};

use super::util::{
    res_400,
    res_404,
    res_500,
};
use crate::webview::WindowId;

fn relativize(path: &Path) -> &Path {
    match path.strip_prefix("/") {
        Ok(path) => path,
        Err(_) => path,
    }
}

pub trait Scope {
    const PATH: &'static str;
}

pub struct Dashboard;

impl Scope for Dashboard {
    const PATH: &'static str = "dashboard";
}

pub struct Autocomplete;

impl Scope for Autocomplete {
    const PATH: &'static str = "autocomplete";
}

/// handle `qcliresource://localhost/`
pub async fn handle<S: Scope>(
    ctx: Arc<Context>,
    request: Request<Vec<u8>>,
    _: WindowId,
) -> anyhow::Result<Response<Cow<'static, [u8]>>> {
    let resources_path = fig_util::directories::resources_path_ctx(&ctx)?.join(S::PATH);

    if request.uri().host() != Some("localhost") {
        return res_400();
    }

    let uri_path = Path::new(request.uri().path());
    let mut path = resources_path.join(relativize(uri_path));

    path = match path.canonicalize() {
        Ok(path) => path,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            if uri_path.extension().is_none() {
                resources_path.join("index.html")
            } else {
                return res_404();
            }
        },
        Err(err) => return res_500(err),
    };

    // dont allow escaping the resources dir
    if !path.starts_with(&resources_path) {
        return res_400();
    }

    match path.metadata() {
        Ok(metadata) => {
            if metadata.is_dir() {
                path.push("index.html");
            }
        },
        Err(err) if err.kind() == ErrorKind::NotFound => {
            if uri_path.extension().is_none() {
                path = resources_path.join("index.html");
            } else {
                return res_404();
            }
        },
        Err(err) => return res_500(err),
    };

    info!("serving resource: {}", path.display());

    let content = match std::fs::read(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => return res_404(),
        Err(err) => return res_500(err),
    };

    let ext = path.extension().and_then(|ext| ext.to_str());
    let mime = match ext {
        Some("html") => mime::TEXT_HTML_UTF_8.as_ref(),
        Some("css") => mime::TEXT_CSS_UTF_8.as_ref(),
        Some("js") => mime::APPLICATION_JAVASCRIPT_UTF_8.as_ref(),
        Some("json") => mime::APPLICATION_JSON.as_ref(),
        Some("svg") => mime::IMAGE_SVG.as_ref(),
        Some("png") => mime::IMAGE_PNG.as_ref(),
        Some("jpg" | "jpeg") => mime::IMAGE_JPEG.as_ref(),
        Some("woff2") => mime::FONT_WOFF2.as_ref(),
        Some("woff") => mime::FONT_WOFF.as_ref(),
        Some("txt" | "text") => mime::TEXT_PLAIN.as_ref(),
        _ => match infer::get(&content) {
            Some(mime) => mime.mime_type(),
            // https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types/Common_types
            None => mime::APPLICATION_OCTET_STREAM.as_ref(),
        },
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, mime)
        .body(content.into())?)
}
