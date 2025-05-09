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
use thiserror::Error;
use url::ParseError;

use crate::database::Database;
use crate::database::state::StateDatabase;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Dir(#[from] crate::util::directories::DirectoryError),
    #[error(transparent)]
    Settings(#[from] crate::database::DatabaseError),
    #[error(transparent)]
    UrlParseError(#[from] ParseError),
}

pub fn new_client(database: &Database) -> Result<Client, RequestError> {
    Ok(Client::builder()
        .use_preconfigured_tls(client_config(database))
        .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
        .cookie_store(true)
        .build()?)
}

pub fn create_default_root_cert_store(database: &Database) -> RootCertStore {
    let mut root_cert_store: RootCertStore = webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect();

    // The errors are ignored because root certificates often include
    // ancient or syntactically invalid certificates
    let rustls_native_certs::CertificateResult { certs, errors: _, .. } = rustls_native_certs::load_native_certs();
    for cert in certs {
        let _ = root_cert_store.add(cert);
    }

    if let Some(custom_cert) = database.get_q_custom_cert() {
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

fn client_config(database: &Database) -> ClientConfig {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));

    ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)
        .expect("Failed to set supported TLS versions")
        .with_root_certificates(create_default_root_cert_store(database))
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
        let database = Database::new().await.unwrap();
        new_client(&database).unwrap();
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

        let database = Database::new().await.unwrap();
        let client = new_client(&database).unwrap();
        let res = client.get(format!("{url}/hello")).send().await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "text/plain");
        assert_eq!(res.text().await.unwrap(), "world");

        mock.expect(1).assert();
    }
}
