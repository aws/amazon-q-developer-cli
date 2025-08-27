use rmcp::ServiceError;
use rmcp::model::{
    ListPromptsResult,
    ListResourceTemplatesResult,
    ListResourcesResult,
    ListToolsResult,
};
use thiserror::Error;

pub type Result<V> = core::result::Result<V, ServiceError>;
pub type MessengerResult = core::result::Result<(), MessengerError>;

/// An interface that abstracts the implementation for information delivery from client and its
/// consumer. It is through this interface secondary information (i.e. information that are needed
/// to make requests to mcp servers) are obtained passively. Consumers of client can of course
/// choose to "actively" retrieve these information via explicitly making these requests.
#[allow(dead_code)]
#[async_trait::async_trait]
pub trait Messenger: std::fmt::Debug + Send + Sync + 'static {
    /// Sends the result of a tools list operation to the consumer
    /// This function is used to deliver information about available tools
    async fn send_tools_list_result(&self, result: Result<ListToolsResult>) -> MessengerResult;

    /// Sends the result of a prompts list operation to the consumer
    /// This function is used to deliver information about available prompts
    async fn send_prompts_list_result(&self, result: Result<ListPromptsResult>) -> MessengerResult;

    /// Sends the result of a resources list operation to the consumer
    /// This function is used to deliver information about available resources
    async fn send_resources_list_result(&self, result: Result<ListResourcesResult>) -> MessengerResult;

    /// Sends the result of a resource templates list operation to the consumer
    /// This function is used to deliver information about available resource templates
    async fn send_resource_templates_list_result(&self, result: Result<ListResourceTemplatesResult>)
    -> MessengerResult;

    /// Signals to the orchestrator that a server has started initializing
    async fn send_init_msg(&self) -> MessengerResult;

    /// Signals to the orchestrator that a server has deinitialized
    fn send_deinit_msg(&self);

    /// Creates a duplicate of the messenger object
    /// This function is used to create a new instance of the messenger with the same configuration
    fn duplicate(&self) -> Box<dyn Messenger>;
}

#[derive(Clone, Debug, Error)]
pub enum MessengerError {
    #[error("{0}")]
    Custom(String),
}

#[derive(Clone, Debug)]
pub struct NullMessenger;

#[async_trait::async_trait]
impl Messenger for NullMessenger {
    async fn send_tools_list_result(&self, _result: Result<ListToolsResult>) -> MessengerResult {
        Ok(())
    }

    async fn send_prompts_list_result(&self, _result: Result<ListPromptsResult>) -> MessengerResult {
        Ok(())
    }

    async fn send_resources_list_result(&self, _result: Result<ListResourcesResult>) -> MessengerResult {
        Ok(())
    }

    async fn send_resource_templates_list_result(
        &self,
        _result: Result<ListResourceTemplatesResult>,
    ) -> MessengerResult {
        Ok(())
    }

    async fn send_init_msg(&self) -> MessengerResult {
        Ok(())
    }

    fn send_deinit_msg(&self) {}

    fn duplicate(&self) -> Box<dyn Messenger> {
        Box::new(NullMessenger)
    }
}
