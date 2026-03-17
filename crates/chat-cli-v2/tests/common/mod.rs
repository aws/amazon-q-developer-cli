//! Common test utilities for ACP integration tests.
#![allow(dead_code, unused)]

mod acp_client;
#[cfg(unix)]
mod harness;
mod paths;

pub use acp_client::{
    AcpTestClient,
    CapturedNotifications,
    PermissionResponse,
    text_content,
};
#[cfg(unix)]
pub use harness::{
    AcpTestHarness,
    AcpTestHarnessBuilder,
};
pub use paths::TestPaths;
