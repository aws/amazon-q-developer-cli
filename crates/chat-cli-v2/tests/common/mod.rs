//! Common test utilities for ACP integration tests.
#![allow(dead_code, unused)]

mod acp_client;
mod harness;
mod paths;

pub use acp_client::{
    AcpTestClient,
    CapturedNotifications,
    text_content,
};
pub use harness::{
    AcpTestHarness,
    AcpTestHarnessBuilder,
};
pub use paths::TestPaths;
