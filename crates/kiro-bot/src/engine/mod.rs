//! Bot runtime engine — ACP pool, dispatch, authorization, and policies.

pub mod acp;
pub mod attachment_read;
pub mod authz;
pub mod coordinator;
pub mod coordinator_bootstrap;
pub mod core;
pub mod dispatch_server;
pub mod dynamo_coordinator;
pub mod feedback;
pub mod rate_limit;
pub mod response_policy;
pub mod retrieval_check;
pub mod task_metadata;
pub mod tool_budget;
pub mod user_map;
