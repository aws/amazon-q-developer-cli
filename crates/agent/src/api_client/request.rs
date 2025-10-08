use std::env::current_exe;
use std::sync::{
    Arc,
    LazyLock,
};

use reqwest::Client;
use rustls::{
    ClientConfig,
    RootCertStore,
};
use thiserror::Error;
use url::ParseError;

use crate::agent::util::error::UtilError;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Util(#[from] UtilError),
    #[error(transparent)]
    UrlParseError(#[from] ParseError),
}

pub fn new_client() -> Result<Client, RequestError> {
    Ok(Client::builder()
        .use_preconfigured_tls(client_config())
        .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
        .cookie_store(true)
        .build()?)
}

pub fn create_default_root_cert_store() -> RootCertStore {
    let mut root_cert_store: RootCertStore = webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect();

    // The errors are ignored because root certificates often include
    // ancient or syntactically invalid certificates
    let rustls_native_certs::CertificateResult { certs, errors: _, .. } = rustls_native_certs::load_native_certs();
    for cert in certs {
        let _ = root_cert_store.add(cert);
    }

    root_cert_store
}

fn client_config() -> ClientConfig {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));

    ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)
        .expect("Failed to set supported TLS versions")
        .with_root_certificates(create_default_root_cert_store())
        .with_no_client_auth()
}

static USER_AGENT: LazyLock<String> = LazyLock::new(|| {
    let name = current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().and_then(|name| name.to_str().map(String::from)))
        .unwrap_or_else(|| "unknown-rust-client".into());

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let version = env!("CARGO_PKG_VERSION");

    format!("{name}-{os}-{arch}-{version}")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_client() {
        new_client().unwrap();
    }

    #[tokio::test]
    async fn request_test() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/hello")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("world")
            .create();
        let url = server.url();

        let client = new_client().unwrap();
        let res = client.get(format!("{url}/hello")).send().await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "text/plain");
        assert_eq!(res.text().await.unwrap(), "world");

        mock.expect(1).assert();
    }
}
