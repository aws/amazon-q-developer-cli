use tracing::error;
use url::Url;

pub fn url() -> Url {
    if let Ok(dashboard_url) = std::env::var("DASHBOARD_URL") {
        return Url::parse(&dashboard_url).unwrap();
    }

    if let Some(dev_url) = fig_settings::settings::get_string_opt("developer.dashboard.host") {
        match Url::parse(&dev_url) {
            Ok(url) => return url,
            Err(err) => {
                error!(%err, "Failed to parse developer.dashboard.host");
            },
        }
    };

    Url::parse("qcliresource://localhost").unwrap()
}
