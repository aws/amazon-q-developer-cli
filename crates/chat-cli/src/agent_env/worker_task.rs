use super::worker::Worker;

#[async_trait::async_trait]
pub trait WorkerTask: Send + Sync {
    fn get_worker(&self) -> &Worker;
    async fn run(&self) -> Result<(), eyre::Error>;
}
