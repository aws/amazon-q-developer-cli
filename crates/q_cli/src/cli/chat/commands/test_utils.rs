use std::sync::Arc;

use fig_os_shim::Context;

/// Create a test context for unit tests
pub fn create_test_context() -> Arc<Context> {
    Context::new()
}
