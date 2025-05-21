use amzn_toolkit_telemetry_client::config::endpoint::{
    Endpoint,
    EndpointFuture,
    Params,
    ResolveEndpoint,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct StaticEndpoint(pub &'static str);

impl ResolveEndpoint for StaticEndpoint {
    fn resolve_endpoint<'a>(&'a self, _params: &'a Params) -> EndpointFuture<'a> {
        let endpoint = Endpoint::builder().url(self.0).build();
        tracing::info!(?endpoint, "Resolving endpoint");
        EndpointFuture::ready(Ok(endpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_static_endpoint() {
        let endpoint = StaticEndpoint("https://example.com");
        let params = Params::builder().build().unwrap();
        let endpoint = endpoint.resolve_endpoint(&params).await.unwrap();
        assert_eq!(endpoint.url(), "https://example.com");
        assert!(endpoint.properties().is_empty());
        assert!(endpoint.headers().count() == 0);
    }
}
