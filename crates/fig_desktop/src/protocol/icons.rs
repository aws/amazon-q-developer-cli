use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::Hash;
use std::io::Cursor;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    LazyLock,
};
use std::time::Duration;

use anyhow::Result;
use fig_os_shim::Context;
use image::imageops::FilterType;
use image::{
    GenericImage,
    GenericImageView,
    ImageFormat,
    Rgba,
};
use moka::future::Cache;
use percent_encoding::percent_decode_str;
use tracing::{
    debug,
    trace,
    warn,
};
use url::Url;
use wry::http::header::CONTENT_TYPE;
use wry::http::{
    Request,
    Response,
    StatusCode,
};

use crate::platform::PlatformState;
use crate::protocol::util::{
    parse_hex_rgb,
    scale_u8,
};
use crate::webview::WindowId;

const DEFAULT_ICON: &str = "template";

#[derive(Hash, Eq, PartialEq, Debug)]
pub enum AssetSpecifier<'a> {
    Named(Cow<'a, str>),
    PathBased(Cow<'a, Path>),
}

static ASSETS: LazyLock<HashMap<AssetSpecifier<'static>, Arc<Cow<'static, [u8]>>>> = LazyLock::new(|| {
    let mut map = HashMap::new();

    macro_rules! load_assets {
        ($($name: expr),*) => {
            $(
                let bytes = include_bytes!(concat!(env!("AUTOCOMPLETE_ICONS_PROCESSED"), "/", $name, ".png"));
                map.insert(
                    AssetSpecifier::Named($name.into()),
                    Arc::new(bytes.as_ref().into()),
                );
            )*
        };
    }

    load_assets! {
        "alert", "asterisk", "box", "carrot", "characters", "command", "commandkey", "cpu", "database",
        "file", "folder", "flag", "gear", "invite", "option", "package", "string", "symlink", "template"
    }

    map
});

pub type ProcessedAsset = (Arc<Cow<'static, [u8]>>, AssetKind);

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
static ASSET_CACHE: LazyLock<Cache<PathBuf, ProcessedAsset>> = LazyLock::new(|| {
    Cache::builder()
        .weigher(|k: &PathBuf, v: &(Arc<Cow<'_, [u8]>>, AssetKind)| {
            (k.as_os_str().len() + v.0.len()).try_into().unwrap_or(u32::MAX)
        })
        .time_to_live(Duration::from_secs(120))
        .build()
});

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone)]
pub enum AssetKind {
    Png,
    Svg,
}

impl AssetKind {
    pub fn mime(&self) -> mime::Mime {
        match self {
            AssetKind::Png => mime::IMAGE_PNG,
            AssetKind::Svg => mime::IMAGE_SVG,
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub async fn process_asset(path: PathBuf) -> Result<ProcessedAsset> {
    if let Some(asset) = ASSET_CACHE.get(&path).await {
        trace!("icon cache hit");
        return Ok(asset);
    }
    trace!(?path, "cache miss processing asset");

    let is_svg = path
        .extension()
        .and_then(OsStr::to_str)
        .is_none_or(|ext| ext.to_lowercase() == "svg");

    let built = if is_svg {
        (Arc::new(tokio::fs::read(&path).await?.into()), AssetKind::Svg)
    } else {
        let path = path.clone();
        tokio::task::spawn_blocking(move || {
            let icon = image::open(path)?;
            let icon = icon.resize(32, 32, FilterType::CatmullRom);
            let mut cursor = Cursor::new(Vec::new());
            icon.write_to(&mut cursor, ImageFormat::Png)?;
            let buffer = cursor.into_inner();
            anyhow::Ok((Arc::new(buffer.into()), AssetKind::Png))
        })
        .await??
    };

    ASSET_CACHE.insert(path, built.clone()).await;

    Ok(built)
}

async fn resolve_asset(asset: &AssetSpecifier<'_>, fallback: Option<&str>) -> (Arc<Cow<'static, [u8]>>, AssetKind) {
    match &asset {
        AssetSpecifier::Named(_) => match ASSETS.get(asset).map(|asset| (asset.clone(), AssetKind::Png)) {
            Some(asset) => Some(asset),
            None => PlatformState::icon_lookup(asset).await,
        },
        AssetSpecifier::PathBased(_) => PlatformState::icon_lookup(asset).await,
    }
    .or_else(|| match fallback {
        Some(fallback) => ASSETS
            .get(&AssetSpecifier::Named(fallback.into()))
            .map(|asset| (asset.clone(), AssetKind::Png)),
        None => None,
    })
    .unwrap_or_else(|| {
        ASSETS
            .get(&AssetSpecifier::Named(DEFAULT_ICON.into()))
            .map(|asset| (asset.clone(), AssetKind::Png))
            .unwrap()
    })
}

fn build_asset_response(data: Cow<'static, [u8]>, asset_kind: AssetKind) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, asset_kind.mime().essence_str())
        .body(data)
        .unwrap()
}

async fn cached_asset_response(asset: &AssetSpecifier<'_>, fallback: Option<&str>) -> Response<Cow<'static, [u8]>> {
    trace!(?asset, "building response for asset");
    let (data, asset_kind) = resolve_asset(asset, fallback).await;
    build_asset_response((*data).clone(), asset_kind)
}

async fn build_default() -> Response<Cow<'static, [u8]>> {
    cached_asset_response(&AssetSpecifier::Named(DEFAULT_ICON.into()), None).await
}

pub async fn handle(
    _ctx: Arc<Context>,
    request: Request<Vec<u8>>,
    _: WindowId,
) -> anyhow::Result<Response<Cow<'static, [u8]>>> {
    debug!(uri =% request.uri(), "Fig protocol request");
    let url = Url::parse(&request.uri().to_string())?;
    let domain = url.domain();
    let pairs: HashMap<_, _> = url.query_pairs().collect();

    let res = match domain {
        Some("template") => {
            let query_pairs: HashMap<Cow<'_, str>, Cow<'_, str>> = url.query_pairs().collect();

            let asset = ASSETS.get(&AssetSpecifier::Named("template".into())).unwrap();
            let mut image = image::load_from_memory_with_format(asset, image::ImageFormat::Png).unwrap();

            if let Some(color) = query_pairs.get("color") {
                if let Some(rgb) = parse_hex_rgb(color) {
                    for y in 0..image.height() {
                        for x in 0..image.width() {
                            let Rgba([r, g, b, a]) = image.get_pixel(x, y);
                            image.put_pixel(
                                x,
                                y,
                                Rgba([scale_u8(r, rgb[0]), scale_u8(g, rgb[1]), scale_u8(b, rgb[2]), a]),
                            );
                        }
                    }
                }
            }

            let mut png_bytes = std::io::Cursor::new(Vec::new());
            image.write_to(&mut png_bytes, image::ImageFormat::Png).unwrap();
            Some(build_asset_response(png_bytes.into_inner().into(), AssetKind::Png))
        },
        Some("icon" | "asset") => match pairs.get("asset").or_else(|| pairs.get("type")) {
            Some(name) => Some(cached_asset_response(&AssetSpecifier::Named(Cow::Borrowed(name)), None).await),
            None => None,
        },
        Some("path") => {
            let decoded_str = &*percent_decode_str(url.path()).decode_utf8().inspect_err(|err| {
                warn!(%err, "Failed to decode fig url");
            })?;

            cfg_if::cfg_if! {
                if #[cfg(windows)] {
                    let path = transform_unix_to_windows_path(path);
                    // TODO: we might want to shellexpand like we do below, but we need
                    // context on what the home dir is
                    let path_str = path.to_str().unwrap_or("");
                } else {
                    let path_str = shellexpand::tilde(&decoded_str);
                    let path = Path::new(path_str.as_ref());
                }
            }

            let fallback = match tokio::fs::metadata(path).await {
                Ok(meta) => {
                    if meta.is_dir() {
                        Some("folder")
                    } else if meta.is_file() {
                        Some("file")
                    } else if meta.is_symlink() {
                        Some("symlink")
                    } else {
                        warn!(%path_str, "Unknown file type");
                        None
                    }
                },
                Err(err) => {
                    warn!(%path_str, %err, "Failed to stat path");
                    Some(if path_str.ends_with('/') { "folder" } else { "file" })
                },
            };

            Some(cached_asset_response(&AssetSpecifier::PathBased(Cow::Borrowed(path)), fallback).await)
        },
        _ => None,
    };

    match res {
        Some(res) => Ok(res),
        None => Ok(build_default().await),
    }
}

/// Translate a unix style path into a windows style path assuming root dir is the drive
#[cfg_attr(not(windows), allow(dead_code))]
fn transform_unix_to_windows_path(path: Cow<'_, str>) -> Cow<'_, Path> {
    use std::path::Component;

    let folder = path.ends_with('/') || path.ends_with('\\');

    let path_components: Vec<_> = Path::new(path.as_ref()).components().collect();
    match &path_components[..] {
        [Component::RootDir, Component::Normal(drive), rest @ ..] => {
            let mut root = std::ffi::OsString::from(drive);
            root.push(":");

            for component in rest {
                root.push("\\");
                root.push(component.as_os_str());
            }

            if folder {
                root.push("/");
            }

            Cow::from(PathBuf::from(root))
        },
        _ => match path {
            Cow::Borrowed(path) => Cow::Borrowed(Path::new(path)),
            Cow::Owned(path) => Cow::Owned(PathBuf::from(path)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_to_windows_path_transform() {
        assert_eq!(
            transform_unix_to_windows_path("/c/User/chay/Downloads".into()),
            Path::new(r"c:\User\chay\Downloads")
        );
        assert_eq!(transform_unix_to_windows_path("/".into()), Path::new("/"));
    }
}
