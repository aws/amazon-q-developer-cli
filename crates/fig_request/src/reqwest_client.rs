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

pub fn create_default_root_cert_store(native_certs: bool) -> RootCertStore {
    let mut root_cert_store = RootCertStore::empty();

    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if native_certs {
        if let Ok(certs) = rustls_native_certs::load_native_certs() {
            for cert in certs {
                // This error is ignored because root certificates often include
                // ancient or syntactically invalid certificates
                let _ = root_cert_store.add(cert);
            }
        }
    }

    let custom_cert = std::env::var("Q_CUSTOM_CERT")
        .ok()
        .or_else(|| fig_settings::state::get_string("Q_CUSTOM_CERT").ok().flatten());

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

fn client_config(native_certs: bool) -> ClientConfig {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));

    ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)
        .expect("Failed to set supported TLS versions")
        .with_root_certificates(create_default_root_cert_store(native_certs))
        .with_no_client_auth()
}

static CLIENT_CONFIG_NATIVE_CERTS: LazyLock<Arc<ClientConfig>> = LazyLock::new(|| Arc::new(client_config(true)));
static CLIENT_CONFIG_NO_NATIVE_CERTS: LazyLock<Arc<ClientConfig>> = LazyLock::new(|| Arc::new(client_config(false)));

pub fn client_config_cached(native_certs: bool) -> Arc<ClientConfig> {
    if native_certs {
        CLIENT_CONFIG_NATIVE_CERTS.clone()
    } else {
        CLIENT_CONFIG_NO_NATIVE_CERTS.clone()
    }
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

pub fn user_agent() -> &'static str {
    &USER_AGENT
}

pub static CLIENT_NATIVE_CERTS: LazyLock<Option<Client>> = LazyLock::new(|| {
    Some(
        Client::builder()
            .use_preconfigured_tls((*client_config_cached(true)).clone())
            .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
            .cookie_store(true)
            .build()
            .unwrap(),
    )
});

pub static CLIENT_NO_NATIVE_CERTS: LazyLock<Option<Client>> = LazyLock::new(|| {
    Some(
        Client::builder()
            .use_preconfigured_tls((*client_config_cached(false)).clone())
            .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
            .cookie_store(true)
            .build()
            .unwrap(),
    )
});

pub fn reqwest_client(native_certs: bool) -> Option<&'static reqwest::Client> {
    if native_certs {
        CLIENT_NATIVE_CERTS.as_ref()
    } else {
        CLIENT_NO_NATIVE_CERTS.as_ref()
    }
}

pub static CLIENT_NATIVE_CERT_NO_REDIRECT: LazyLock<Option<Client>> = LazyLock::new(|| {
    Client::builder()
        .use_preconfigured_tls((*client_config_cached(true)).clone())
        .user_agent(USER_AGENT.chars().filter(|c| c.is_ascii_graphic()).collect::<String>())
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()
});

pub fn reqwest_client_no_redirect() -> Option<&'static reqwest::Client> {
    CLIENT_NATIVE_CERT_NO_REDIRECT.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_client() {
        reqwest_client(true).unwrap();
        reqwest_client(false).unwrap();
        reqwest_client_no_redirect().unwrap();
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

        for client in [reqwest_client(false).unwrap(), reqwest_client(true).unwrap()] {
            let res = client.get(format!("{url}/hello")).send().await.unwrap();
            assert_eq!(res.status(), 200);
            assert_eq!(res.headers()["content-type"], "text/plain");
            assert_eq!(res.text().await.unwrap(), "world");
        }

        mock.expect(2).assert();
    }

    #[tokio::test]
    async fn no_redirect_request_test() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/hello")
            .with_status(302)
            .with_header("location", "/redirect")
            .create();

        let url = server.url();

        let client = reqwest_client_no_redirect().unwrap();
        let res = client.get(format!("{url}/hello")).send().await.unwrap();
        assert_eq!(res.status(), 302);
        assert_eq!(res.headers()["location"], "/redirect");

        mock.expect(1).assert();
    }
}
