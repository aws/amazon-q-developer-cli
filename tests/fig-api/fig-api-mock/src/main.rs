use std::sync::Arc;

use clap::Parser;
use fig_desktop_api::handler::{
    EventHandler,
    Wrapped,
};
use fig_desktop_api::kv::{
    DashKVStore,
    KVStore,
};
use fig_desktop_api::requests::{
    RequestResult,
    RequestResultImpl,
};
use fig_os_shim::{
    ContextArcProvider,
    ContextProvider,
};
use fig_proto::fig::NotificationRequest;
use fig_settings::{
    Settings,
    SettingsProvider,
    State,
    StateProvider,
};

#[derive(Parser)]
enum Cli {
    Request {
        request_b64: String,
        #[arg(long)]
        cwd: Option<String>,
    },
    Init,
}

struct MockHandler;

struct Context {
    kv: DashKVStore,
    settings: Settings,
    state: State,
    ctx: Arc<fig_os_shim::Context>,
}

impl KVStore for Context {
    fn set_raw(&self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) {
        self.kv.set_raw(key, value);
    }

    fn get_raw(&self, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        self.kv.get_raw(key)
    }
}

impl SettingsProvider for Context {
    fn settings(&self) -> &Settings {
        &self.settings
    }
}

impl StateProvider for Context {
    fn state(&self) -> &State {
        &self.state
    }
}

impl ContextProvider for Context {
    fn context(&self) -> &fig_os_shim::Context {
        &self.ctx
    }
}

impl ContextArcProvider for Context {
    fn context_arc(&self) -> Arc<fig_os_shim::Context> {
        Arc::clone(&self.ctx)
    }
}

#[async_trait::async_trait]
impl EventHandler for MockHandler {
    type Ctx = Context;

    async fn notification(&self, _request: Wrapped<Self::Ctx, NotificationRequest>) -> RequestResult {
        RequestResult::success()
    }
}

#[tokio::main]
async fn main() {
    match Cli::parse() {
        Cli::Request { request_b64, cwd } => {
            if let Some(cwd) = cwd {
                std::env::set_current_dir(cwd).unwrap();
            }

            let request = fig_desktop_api::handler::request_from_b64(&request_b64).unwrap();
            let response = fig_desktop_api::handler::api_request(
                MockHandler,
                Context {
                    kv: DashKVStore::new(),
                    settings: Settings::new(),
                    state: State::new(),
                    ctx: fig_os_shim::Context::new(),
                },
                request,
            )
            .await
            .unwrap();
            let response_b64 = fig_desktop_api::handler::response_to_b64(response);
            println!("{response_b64}");
        },
        Cli::Init => {
            println!("{}", fig_desktop_api::init_script::javascript_init(false));
        },
    }
}
