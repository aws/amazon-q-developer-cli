use aws_config::default_provider::region::DefaultRegionChain;
use aws_config::ecs::EcsCredentialsProvider;
use aws_config::environment::credentials::EnvironmentVariableCredentialsProvider;
use aws_config::imds::credentials::ImdsCredentialsProvider;
use aws_config::meta::credentials::CredentialsProviderChain;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_config::provider_config::ProviderConfig;
use aws_config::web_identity_token::WebIdentityTokenCredentialsProvider;
use aws_credential_types::Credentials;
use aws_credential_types::provider::{
    self,
    ProvideCredentials,
    future,
};
use tracing::Instrument;

#[derive(Debug)]
pub struct CredentialsChain {
    provider_chain: CredentialsProviderChain,
}

impl CredentialsChain {
    /// Based on code the code for
    /// [aws_config::default_provider::credentials::DefaultCredentialsChain]
    pub async fn new() -> Self {
        let region = DefaultRegionChain::builder().build().region().await;
        let config = ProviderConfig::default().with_region(region.clone());

        let env_provider = EnvironmentVariableCredentialsProvider::new();
        let profile_provider = ProfileFileCredentialsProvider::builder().configure(&config).build();
        let web_identity_token_provider = WebIdentityTokenCredentialsProvider::builder()
            .configure(&config)
            .build();
        let imds_provider = ImdsCredentialsProvider::builder().configure(&config).build();
        let ecs_provider = EcsCredentialsProvider::builder().configure(&config).build();

        let mut provider_chain = CredentialsProviderChain::first_try("Environment", env_provider);

        provider_chain = provider_chain
            .or_else("Profile", profile_provider)
            .or_else("WebIdentityToken", web_identity_token_provider)
            .or_else("EcsContainer", ecs_provider)
            .or_else("Ec2InstanceMetadata", imds_provider);

        CredentialsChain { provider_chain }
    }

    async fn credentials(&self) -> provider::Result {
        self.provider_chain
            .provide_credentials()
            .instrument(tracing::debug_span!("provide_credentials", provider = %"default_chain"))
            .await
    }
}

impl ProvideCredentials for CredentialsChain {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.credentials())
    }

    fn fallback_on_interrupt(&self) -> Option<Credentials> {
        self.provider_chain.fallback_on_interrupt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_credentials_chain() {
        let credentials_chain = CredentialsChain::new().await;
        let credentials_res = credentials_chain.provide_credentials().await;
        let fallback_on_interrupt_res = credentials_chain.fallback_on_interrupt();
        println!("credentials_res: {credentials_res:?}, fallback_on_interrupt_res: {fallback_on_interrupt_res:?}");
    }
}
