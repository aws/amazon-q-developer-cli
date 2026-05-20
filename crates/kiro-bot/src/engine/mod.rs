//! Bot runtime engine — ACP pool, dispatch, authorization, and policies.

pub mod acp;
pub mod authz;
pub mod coordinator;
pub mod core;
pub mod dispatch_server;
pub mod dynamo_coordinator;
pub mod feedback;
pub mod response_policy;
pub mod tool_budget;
pub mod user_map;
