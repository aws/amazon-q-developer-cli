use std::borrow::Cow;
use std::sync::{
    Arc,
    LazyLock,
};

use anyhow::Result;
use fig_auth::builder_id_token;
use fig_os_shim::Context;
use fig_request::reqwest::Client;
use fnv::FnvHashSet;
use futures::prelude::*;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    MappedMutexGuard,
    Mutex,
    MutexGuard,
};
use tracing::error;
use url::Url;
use wry::http::header::CONTENT_TYPE;
use wry::http::{
    HeaderValue,
    Request,
    Response,
    StatusCode,
};

use crate::webview::WindowId;

const APPLICATION_JAVASCRIPT: HeaderValue = HeaderValue::from_static("application/javascript");

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthType {
    None,
    Midway,
}

impl AuthType {
    pub async fn get(&self, default_client: &Client, url: Url) -> Result<fig_request::reqwest::Response> {
        match self {
            AuthType::Midway => Ok(fig_request::midway::midway_request(url).await?.error_for_status()?),
            _ => Ok(default_client.get(url).send().await?.error_for_status()?),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CdnSource {
    url: Url,
    auth_type: AuthType,
}

static CDNS: LazyLock<Vec<CdnSource>> = LazyLock::new(|| {
    vec![
        // Public cdn
        CdnSource {
            url: "https://specs.q.us-east-1.amazonaws.com".try_into().unwrap(),
            auth_type: AuthType::None,
        },
        // Internal Amazon spec cdn
        CdnSource {
            url: "https://prod.us-east-1.shellspecs.jupiter.ai.aws.dev"
                .try_into()
                .unwrap(),
            auth_type: AuthType::Midway,
        },
    ]
});

fn res_404() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(CONTENT_TYPE, "text/plain")
        .body(b"Not Found".as_ref().into())
        .unwrap()
}

fn res_ok(bytes: Vec<u8>, content_type: HeaderValue) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(bytes.into())
        .unwrap()
}

#[derive(Debug, Clone)]
struct SpecIndexMeta {
    cdn_source: CdnSource,
    spec_index: SpecIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecIndex {
    completions: Vec<String>,
    diff_versioned_completions: Vec<String>,
}

static INDEX_CACHE: Mutex<Option<Vec<Result<SpecIndexMeta>>>> = Mutex::const_new(None);

pub async fn clear_index_cache() {
    *INDEX_CACHE.lock().await = None;
}

async fn remote_index_json(client: &Client) -> MappedMutexGuard<'_, Vec<Result<SpecIndexMeta>>> {
    let mut cache = INDEX_CACHE.lock().await;

    if cache.is_none() {
        *cache = Some(
            future::join_all(CDNS.iter().map(|cdn_source| async move {
                if AuthType::Midway == cdn_source.auth_type {
                    let auth_token = match builder_id_token().await {
                        Ok(auth_token) => match auth_token {
                            Some(auth_token) => auth_token,
                            None => return None,
                        },
                        Err(err) => {
                            error!(%err, "Failed to load auth");
                            return None;
                        },
                    };

                    if !auth_token.is_amzn_user() {
                        return None;
                    }
                }

                let mut url = cdn_source.url.clone();
                url.set_path("index.json");

                let response = match cdn_source.auth_type.get(client, url).await {
                    Ok(response) => response,
                    Err(err) => {
                        error!(%err, "Failed to fetch spec index");
                        return Some(Err(err));
                    },
                };

                let spec_index = match response.json().await {
                    Ok(s) => s,
                    Err(s) => {
                        error!(%s, "Failed to parse spec index");
                        return Some(Err(s.into()));
                    },
                };

                Some(Ok(SpecIndexMeta {
                    cdn_source: cdn_source.clone(),
                    spec_index,
                }))
            }))
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        );
    }

    MutexGuard::map(cache, |cache| cache.as_mut().unwrap())
}

async fn merged_index_json(client: &Client) -> Result<SpecIndex> {
    let mut completions = FnvHashSet::default();
    let mut diff_versioned_completions = FnvHashSet::default();

    for res in remote_index_json(client).await.iter() {
        match res {
            Ok(meta) => {
                completions.extend(meta.spec_index.completions.clone());
                diff_versioned_completions.extend(meta.spec_index.diff_versioned_completions.clone());
            },
            Err(err) => {
                tracing::error!(%err, "failed to fetch spec index");
            },
        }
    }

    let mut completions: Vec<_> = completions.into_iter().collect();
    completions.sort();

    let mut diff_versioned_completions: Vec<_> = diff_versioned_completions.into_iter().collect();
    diff_versioned_completions.sort();

    Ok(SpecIndex {
        completions,
        diff_versioned_completions,
    })
}

// handle `spec://localhost/spec.js`
pub async fn handle(
    _ctx: Arc<Context>,
    request: Request<Vec<u8>>,
    _: WindowId,
) -> anyhow::Result<Response<Cow<'static, [u8]>>> {
    let Some(client) = fig_request::client() else {
        return Ok(res_404());
    };

    let path = request.uri().path();

    if path == "/index.json" {
        let index = merged_index_json(client).await?;
        Ok(res_ok(
            serde_json::to_vec(&index)?,
            "application/json".try_into().unwrap(),
        ))
    } else {
        // default to trying the first cdn
        let mut cdn_source = CDNS[0].clone();

        let spec_name = path.strip_prefix('/').unwrap_or(path);
        let spec_name = spec_name.strip_suffix(".js").unwrap_or(spec_name);

        for meta in remote_index_json(client).await.iter().skip(1).flatten() {
            if meta
                .spec_index
                .completions
                .binary_search_by(|name| name.as_str().cmp(spec_name))
                .is_ok()
            {
                cdn_source = meta.cdn_source.clone();
                break;
            }
        }

        let mut url = cdn_source.url.clone();
        url.set_path(path);

        let response = cdn_source.auth_type.get(client, url).await?;

        let content_type = response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .cloned()
            .unwrap_or(APPLICATION_JAVASCRIPT);

        Ok(res_ok(
            response.bytes().await?.to_vec(),
            content_type.as_bytes().try_into()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdns() {
        println!("{:?}", *CDNS);
    }

    #[tokio::test]
    async fn test_index_json() {
        let client = Client::new();
        let index = remote_index_json(&client).await;
        println!("{index:?}");
    }
}
