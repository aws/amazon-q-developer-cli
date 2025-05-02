use crate::{
    PromptsListResult,
    ResourceTemplatesListResult,
    ResourcesListResult,
    ToolsListResult,
};

/// An interface that abstracts the implementation for information delivery from client and its
/// consumer. It is through this interface secondary information (i.e. information that are needed
/// to make requests to mcp servers) are obtained passively. Consumers of client can of course
/// choose to "actively" retrieve these information via explicitly making these requests.
pub trait Messenger: Send + Sync + 'static {
    /// Sends the result of a tools list operation to the consumer
    /// This function is used to deliver information about available tools
    fn send_tools_list_result(result: ToolsListResult);

    /// Sends the result of a prompts list operation to the consumer
    /// This function is used to deliver information about available prompts
    fn send_prompts_list_result(result: PromptsListResult);

    /// Sends the result of a resources list operation to the consumer
    /// This function is used to deliver information about available resources
    fn send_resources_list_result(result: ResourcesListResult);

    /// Sends the result of a resource templates list operation to the consumer
    /// This function is used to deliver information about available resource templates
    fn send_resource_templates_list_result(result: ResourceTemplatesListResult);
}
