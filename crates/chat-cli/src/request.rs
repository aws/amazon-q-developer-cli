use std::env::current_exe;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{
    Arc,
    LazyLock,
};

use reqwest::Client;
use rustls::{
    ClientConfig,
    RootCertStore,
};
use url::ParseError;

#[derive(Debug)]
pub enum RequestError {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
    Io(std::io::Error),
    Dir(crate::util::directories::DirectoryError),
    Settings(crate::settings::SettingsError),
    UrlParseError(ParseError),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Reqwest(err) => write!(f, "Reqwest error: {err}"),
            RequestError::Serde(err) => write!(f, "Serde error: {err}"),
            RequestError::Io(err) => write!(f, "Io error: {err}"),
            RequestError::Dir(err) => write!(f, "Dir error: {err}"),
            RequestError::Settings(err) => write!(f, "Settings error: {err}"),
            RequestError::UrlParseError(err) => write!(f, "Url parse error: {err}"),
        }
    }
}

impl std::error::Error for RequestError {}

impl From<reqwest::Error> for RequestError {
    fn from(e: reqwest::Error) -> Self {
        RequestError::Reqwest(e)
    }
}

impl From<serde_json::Error> for RequestError {
    fn from(e: serde_json::Error) -> Self {
        RequestError::Serde(e)
    }
}

impl From<std::io::Error> for RequestError {
    fn from(e: std::io::Error) -> Self {
        RequestError::Io(e)
    }
}

impl From<crate::util::directories::DirectoryError> for RequestError {
    fn from(e: crate::util::directories::DirectoryError) -> Self {
        RequestError::Dir(e)
    }
}

impl From<crate::settings::SettingsError> for RequestError {
    fn from(e: crate::settings::SettingsError) -> Self {
        RequestError::Settings(e)
    }
}

impl From<ParseError> for RequestError {
    fn from(e: ParseError) -> Self {
        RequestError::UrlParseError(e)
    }
}

pub fn client() -> Option<&'static Client> {
    CLIENT_NATIVE_CERTS.as_ref()
}

pub fn create_default_root_cert_store() -> RootCertStore {
    let mut root_cert_store: RootCertStore = webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect();

    // The errors are ignored because root certificates often include
    // ancient or syntactically invalid certificates
    let rustls_native_certs::CertificateResult { certs, errors: _, .. } = rustls_native_certs::load_native_certs();
    for cert in certs {
        let _ = root_cert_store.add(cert);
    }

    let custom_cert = std::env::var("Q_CUSTOM_CERT")
        .ok()
        .or_else(|| crate::settings::state::get_string("Q_CUSTOM_CERT").ok().flatten());

    if let Some(custom_cert) = custom_cert {
        match File::open(Path::new(&custom_cert)) {
            Ok(file) => {
                let reader = &mut BufReader::new(file);
                for cert in rustls_pemfile::certs(reader) {
                    match cert {
                        Ok(cert) => {
                            if let Err(err) = root_cert_store.add(cert) {
                                tracing::error!(path =% custom_cert, %err, "Failed to add custom cert");
                            };
                        },
                        Err(err) => tracing::error!(path =% custom_cert, %err, "Failed to parse cert"),
                    }
                }
            },
            Err(err) => tracing::error!(path =% custom_cert, %err, "Failed to open cert at"),
        }
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

static CLIENT_CONFIG_NATIVE_CERTS: LazyLock<Arc<ClientConfig>> = LazyLock::new(|| Arc::new(client_config()));

pub fn client_config_cached() -> Arc<ClientConfig> {
    CLIENT_CONFIG_NATIVE_CERTS.clone()
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

pub static CLIENT_NATIVE_CERTS: LazyLock<Option<Client>> = LazyLock::new(|| {
    Some(
        Client::builder()
            .use_preconfigured_tls((*client_config_cached()).clone())
            .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
            .cookie_store(true)
            .build()
            .unwrap(),
    )
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_client() {
        client().unwrap();
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

        let client = client().unwrap();
        let res = client.get(format!("{url}/hello")).send().await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "text/plain");
        assert_eq!(res.text().await.unwrap(), "world");

        mock.expect(1).assert();
    }
}
