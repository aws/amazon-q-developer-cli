use uuid::Uuid;
use tokio_util::sync::CancellationToken;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use aws_sdk_bedrockruntime::{Client as BedrockClient, types::{Message, ConversationRole, ContentBlock}};
use aws_config::{BehaviorVersion, Region};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use tokio::sync::RwLock;

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =  ===   JOB CONTINUATION TYPES   ===  ===  ===  ===  ===  ===  ===  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

#[derive(Debug, Clone, Copy)]
pub enum WorkerJobCompletionType {
    Normal,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub enum JobState {
    Running,
    Done(WorkerJobCompletionType, Option<String>), // Store error message as String instead of Result
}

pub type WorkerJobContinuationFn = Arc<
    dyn Fn(Arc<Worker>, WorkerJobCompletionType, Option<String>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send + Sync,
>;

pub struct Continuations {
    state: RwLock<JobState>,
    map: RwLock<HashMap<String, WorkerJobContinuationFn>>,
}

impl Continuations {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(JobState::Running),
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn boxed<F, Fut>(f: F) -> WorkerJobContinuationFn
    where
        F: Fn(Arc<Worker>, WorkerJobCompletionType, Option<String>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Arc::new(move |worker, completion_type, error_msg| Box::pin(f(worker, completion_type, error_msg)))
    }

    pub async fn add_or_run_now(&self, key: impl Into<String>, callback: WorkerJobContinuationFn, worker: Arc<Worker>) {
        match &*self.state.read().await {
            JobState::Running => {
                self.map.write().await.insert(key.into(), callback);
            }
            JobState::Done(completion_type, error_msg) => {
                let completion_type = *completion_type;
                let error_msg = error_msg.clone();
                tokio::spawn(callback(worker, completion_type, error_msg));
            }
        }
    }

    pub async fn complete(&self, result: Result<(), anyhow::Error>, worker: Arc<Worker>, cancellation_token: &CancellationToken) {
        let completion_type = if cancellation_token.is_cancelled() {
            WorkerJobCompletionType::Cancelled
        } else if result.is_err() {
            WorkerJobCompletionType::Failed
        } else {
            WorkerJobCompletionType::Normal
        };

        let error_msg = result.err().map(|e| e.to_string());

        {
            let mut st = self.state.write().await;
            *st = JobState::Done(completion_type, error_msg.clone());
        }
        let callbacks = {
            let mut map = self.map.write().await;
            std::mem::take(&mut *map)
        };
        for (_name, cb) in callbacks {
            let worker_clone = Arc::clone(&worker);
            let error_msg_clone = error_msg.clone();
            tokio::spawn(cb(worker_clone, completion_type, error_msg_clone));
        }
    }
}

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =  ===   LLM ACCESS    ===  ===  ===  ===  ===  ===  ===  ===  ===  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// Model Provider interface; request, stream, and response data structures
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub enum ModelResponseChunk {
    AssistantMessage(String),
    ToolUseRequest { tool_name: String, parameters: String },
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: String,
    pub tool_requests: Vec<ToolRequest>,
}

#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub tool_name: String,
    pub parameters: String,
}

#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    async fn request(
        &self,
        request: ModelRequest,
        when_receiving_begin: impl Fn() + Send,
        when_received: impl Fn(ModelResponseChunk) + Send,
        cancellation_token: CancellationToken,
    ) -> Result<ModelResponse, anyhow::Error>;
}

// Bedrock ConverseStream-based ModelProvider
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //

#[derive(Clone)]
pub struct BedrockConverseStreamModelProvider {
    client: BedrockClient,
    model_id: String,
}

impl BedrockConverseStreamModelProvider {
    pub fn new(client: BedrockClient) -> Self {
        Self {
            client,
            model_id: "us.anthropic.claude-sonnet-4-20250514-v1:0".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl ModelProvider for BedrockConverseStreamModelProvider {
    async fn request(
        &self,
        request: ModelRequest,
        when_receiving_begin: impl Fn() + Send,
        when_received: impl Fn(ModelResponseChunk) + Send,
        cancellation_token: CancellationToken,
    ) -> Result<ModelResponse, anyhow::Error> {
        let message = Message::builder()
            .role(ConversationRole::User)
            .content(ContentBlock::Text(request.prompt))
            .build()?;

        let response = tokio::select! {
            result = self.client
                .converse_stream()
                .model_id(&self.model_id)
                .messages(message)
                .send() => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("AWS Bedrock request failed:");
                        eprintln!("  Model ID: {}", self.model_id);
                        eprintln!("  Error type: {:?}", e);
                        eprintln!("  Error message: {}", e);
                        
                        // Check for common error types
                        if e.to_string().contains("dispatch failure") {
                            eprintln!("  Likely causes:");
                            eprintln!("    - AWS credentials not configured");
                            eprintln!("    - Network connectivity issues");
                            eprintln!("    - AWS region not set or incorrect");
                            eprintln!("    - Bedrock service not available in region");
                        }
                        
                        return Err(anyhow::anyhow!("Bedrock request failed: {}", e));
                    }
                }
            },
            _ = cancellation_token.cancelled() => {
                return Err(anyhow::anyhow!("Request cancelled"));
            }
        };

        when_receiving_begin();
        let mut accumulated_content = String::new();
        let mut stream = response.stream;

        loop {
            let event = tokio::select! {
                event = stream.recv() => event,
                _ = cancellation_token.cancelled() => {
                    println!("Model request cancelled during streaming");
                    return Err(anyhow::anyhow!("Request cancelled"));
                }
            };

            match event {
                Ok(Some(output)) => {
                    // Check cancellation between processing chunks
                    if cancellation_token.is_cancelled() {
                        println!("Model request cancelled during chunk processing");
                        return Err(anyhow::anyhow!("Request cancelled"));
                    }
                    
                    match output {
                        aws_sdk_bedrockruntime::types::ConverseStreamOutput::ContentBlockDelta(delta) => {
                            if let Some(delta_content) = delta.delta {
                                if let aws_sdk_bedrockruntime::types::ContentBlockDelta::Text(text) = delta_content {
                                    accumulated_content.push_str(&text);
                                    when_received(ModelResponseChunk::AssistantMessage(text));
                                }
                            }
                        }
                        aws_sdk_bedrockruntime::types::ConverseStreamOutput::MessageStop(_) => {
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(anyhow::anyhow!("Stream error: {}", e)),
            }
        }

        Ok(ModelResponse {
            content: accumulated_content,
            tool_requests: Vec::new(),
        })
    }
}

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =  ===    WORKERS, TASKS, AND JOBS    ===  ===  ===  ===  ===  ===  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// Worker - combination of everything an agent needs to execute tasks. Think of it
//      as "agent config JSON + conversation history + LLM access + tools"
//      - (not in this demo) Conversation history
//      - (not in this demo) Context resources
//      - (not in this demo) LLM Request builder (combine context, tools info,
//          conversation, any extra info)
//      - LLM Provider (provides access to given modev over given API)
//      - (not in this demo) Tools Provider (a layer responsible for approval and 
//          execution of the tools)
//
//      **The main reason** to have that all as a signle union unit is
//        that we want to have _multiple_ workers in memory at the same time,
//        working on different tasks using different context and tools
//
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerStates {
    Inactive,
    Working,
    Requesting,
    Receiving,
    Waiting,
    UsingTool,
    InactiveFailed,
}

pub struct Worker {
    pub id: Uuid,
    pub name: String,
    // + conversation history
    // + context resources
    // + request builder
    pub model_provider: BedrockConverseStreamModelProvider,
    // + tools provider
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}

impl Worker {
    pub fn new(name: String, model_provider: BedrockConverseStreamModelProvider) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            model_provider,
            state: Arc::new(Mutex::new(WorkerStates::Inactive)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_state(&self, new_state: WorkerStates, interface: &dyn WorkerToHostInterface) {
        {
            let mut state = self.state.lock().unwrap();
            *state = new_state;
        }
        interface.worker_state_change(self.id, new_state);
    }

    pub fn get_state(&self) -> WorkerStates {
        *self.state.lock().unwrap()
    }

    pub fn set_failure(&self, error: String) {
        let mut failure = self.last_failure.lock().unwrap();
        *failure = Some(error);
    }

    pub fn get_failure(&self) -> Option<String> {
        self.last_failure.lock().unwrap().clone()
    }
}

// Worker Task - main abstraction of a logical task implemented for a worker
//   Think of "Implementation for '/compact'" or "Implementation for main agent loop"
//   This is an interface for actula implementations of tasks that can be performed 
//   for a worker. Basic loop for a conversation agent, compact, custom sub-agents
//   orchestrator loop - they all will be implemented behind this trait.
//
//   An importnat note: implemented 'MainAgentLoopTask' would NOT include asking
//   the user for next prompt, it's intended to implement a finite 'agent turn',
//   loop LLM->Response->ToolUses->LLM, that breaks when response has no tool uses.
//   Main app UI would have to obtain the prompt from the user and launch `MainAgentLoopTask`
//   again for each prompt.
//
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //

#[async_trait::async_trait]
pub trait WorkerTask: Send + Sync {
    fn get_worker(&self) -> &Worker;
    async fn run(&self) -> Result<(), anyhow::Error>;
}

// WorkerJob - an entry for an active task running for given WorkerTask
// Basically WorkerTask + actual running task handle
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //

pub struct WorkerJob {
    pub worker: Arc<Worker>,
    pub worker_task: Arc<dyn WorkerTask>,
    pub cancellation_token: CancellationToken,
    pub task_handle: Option<tokio::task::JoinHandle<Result<(), anyhow::Error>>>,
    pub continuations: Arc<Continuations>,
}

impl WorkerJob {
    pub fn new(
        worker: Arc<Worker>,
        worker_task: Arc<dyn WorkerTask>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            worker_task,
            cancellation_token,
            task_handle: None,
            continuations: Arc::new(Continuations::new()),
        }
    }

    pub fn launch(&mut self) {
        let worker_task_clone = self.worker_task.clone();
        let continuations = Arc::clone(&self.continuations);
        let worker = Arc::clone(&self.worker);
        let cancellation_token = self.cancellation_token.clone();
        
        let task_handle = tokio::spawn(async move {
            let result = worker_task_clone.run().await;
            continuations.complete(result, worker, &cancellation_token).await;
            Ok(()) // Always return Ok since we handle the error in complete()
        });
        self.task_handle = Some(task_handle);
    }

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    pub async fn wait(self) -> Result<(), anyhow::Error> {
        match self.task_handle {
            Some(handle) => match handle.await {
                Ok(result) => result,
                Err(join_error) => Err(anyhow::anyhow!("Task panicked: {}", join_error)),
            },
            None => Err(anyhow::anyhow!("Task not launched")),
        }
    }
}


// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =  ===   DEMO WORKER TASK   ===  ===  ===  ===  ===  ===  ===  ===  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// Demo Prototype Worker Loop task
//   This is just a demo. It queries LLM through ModelProvider, then uses
//   WorkerToHostInterface.get_tool_confirmation to ask for input from the user
//   and sends it back as a fake chunk from the model.
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //

#[derive(Debug, Clone)]
pub struct WorkerInput {
    pub prompt: String,
}

pub struct WorkerProtoLoop {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    input: WorkerInput,
    host_interface: Arc<dyn WorkerToHostInterface>,
}

impl WorkerProtoLoop {
    pub fn new(
        worker: Arc<Worker>,
        input: WorkerInput,
        host_interface: Arc<dyn WorkerToHostInterface>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            input,
            host_interface,
            cancellation_token,
        }
    }
}

#[async_trait::async_trait]
impl WorkerTask for WorkerProtoLoop {
    fn get_worker(&self) -> &Worker {
        &self.worker
    }

    async fn run(&self) -> Result<(), anyhow::Error> {
        // Reset worker state and start up
        self.check_cancellation("before starting")?;
        self.reset_worker();
        
        // demo request from a model
        //      note: both request and UI callback a cancellable
        let model_request = self.build_request();
        let model_response = self.make_model_request(model_request).await?;

        // demo callback to the UI
        self.demo_tool_confirmation(model_response).await?;
        
        // finish
        self.complete_successfully()
    }
}

impl WorkerProtoLoop {
    fn check_cancellation(&self, stage: &str) -> Result<(), anyhow::Error> {
        if self.cancellation_token.is_cancelled() {
            println!("Worker {} cancelled {}", self.worker.id, stage);
            return Err(anyhow::anyhow!("Operation cancelled"));
        }
        Ok(())
    }

    fn reset_worker(&self) {
        self.worker.set_failure("".to_string());
        self.worker.set_state(WorkerStates::Working, &*self.host_interface);
    }

    fn build_request(&self) -> ModelRequest {
        ModelRequest {
            prompt: self.input.prompt.clone(),
        }
    }

    async fn make_model_request(&self, model_request: ModelRequest) -> Result<ModelResponse, anyhow::Error> {
        self.worker.set_state(WorkerStates::Requesting, &*self.host_interface);
        
        let response = self.worker.model_provider.request(
            model_request,
            || {
                self.worker.set_state(WorkerStates::Receiving, &*self.host_interface);
            },
            |chunk| {
                self.host_interface.response_chunk_received(self.worker.id, chunk);
            },
            self.cancellation_token.clone(),
        ).await.map_err(|e| {
            if !self.cancellation_token.is_cancelled() {
                let error_msg = format!("Model request failed: {}", e);
                eprintln!("Error in worker {}: {}", self.worker.id, error_msg);
                self.worker.set_failure(error_msg);
                self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
            } else {
                self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
            }
            e
        })?;
        
        Ok(response)
    }

    async fn demo_tool_confirmation(&self, model_response: ModelResponse) -> Result<(), anyhow::Error> {
        self.check_cancellation("before tool confirmation")?;
        self.worker.set_state(WorkerStates::Waiting, &*self.host_interface);
        
        let user_response = self.host_interface.get_tool_confirmation(
            self.worker.id,
            format!("Hello from worker! MR={}", 
                model_response.content.chars().take(50).collect::<String>()),
            self.cancellation_token.clone(),
        ).await.map_err(|e| {
            if !self.cancellation_token.is_cancelled() {
                let error_msg = format!("Tool confirmation failed: {}", e);
                eprintln!("Error in worker {}: {}", self.worker.id, error_msg);
                self.worker.set_failure(error_msg);
                self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
            } else {
                self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
            }
            e
        })?;
        
        // Publish user response as fake AssistantMessage chunk
        self.host_interface.response_chunk_received(
            self.worker.id,
            ModelResponseChunk::AssistantMessage(format!("\n\nUser said: {}\n", user_response))
        );
        
        Ok(())
    }

    fn complete_successfully(&self) -> Result<(), anyhow::Error> {
        self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
        println!("Worker {} completed successfully", self.worker.id);
        Ok(())
    }
}

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// = WorkerToHostInterface - primamry WoorkerTask <-> UI communication API  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// Note: get_tool_confirmation() API is simplified to demo an interruption for
// external information request. **This is NOT** intented way to ask for user
// prompt between agent turns. The production implementation would take ToolRequest,
// and return either OK, NOT_OK(reason), or modified ToolRequest. It's going to be
// up to UI how exactly to let user provide that response.

#[async_trait::async_trait]
pub trait WorkerToHostInterface: Send + Sync {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates);
    fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk);
    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, anyhow::Error>;
}

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =  Session - the main orchestrator of runing Worker Tasks and Jobs  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// Session maintains the lists of existing Workers and running Jobs
//  .build_worker() creates new worker instance. It would be parametrised to build
//      a worker based on provided configs, available tools, etc
//  .run_xxx() is a shortcut to build a Task of specific class and launch it as a Job
//  .run() internal method that actually launches provided Task as a Job
//  .cancel_all_jobs() - triggers cancellation tokens for all available jobs
//
//  In production implementation it would have self-cleaning for `jobs` list, and APIs
//  to remove workers that are no longer needed.

pub struct Session {
    model_providers: Vec<BedrockConverseStreamModelProvider>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,
}

impl Session {
    pub fn new(model_providers: Vec<BedrockConverseStreamModelProvider>) -> Self {
        Self {
            model_providers,
            workers: Arc::new(Mutex::new(Vec::new())),
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn build_worker(&self) -> Arc<Worker> {
        let model_provider = self.model_providers.first()
            .expect("At least one model provider required")
            .clone();
        
        let worker = Arc::new(Worker::new(
            "Test worker".to_string(),
            model_provider,
        ));
        
        self.workers.lock().unwrap().push(worker.clone());
        worker
    }

    pub fn run_demo_loop(
        &self,
        worker: Arc<Worker>,
        input: WorkerInput,
        ui_interface: Arc<dyn WorkerToHostInterface>,
    ) -> Result<Arc<WorkerJob>, anyhow::Error> {
        let cancellation_token = CancellationToken::new();
        
        let worker_loop = Arc::new(WorkerProtoLoop::new(
            worker.clone(),
            input,
            ui_interface,
            cancellation_token.clone(),
        ));
        
        self.run(worker, worker_loop, cancellation_token)
    }

    // Launching a Task - producing a Job
    fn run(
        &self,
        worker: Arc<Worker>,
        worker_task: Arc<dyn WorkerTask>,
        cancellation_token: CancellationToken,
    ) -> Result<Arc<WorkerJob>, anyhow::Error> {
        let mut job = WorkerJob::new(
            worker,
            worker_task,
            cancellation_token,
        );
        
        job.launch();

        let job = Arc::new(job);
        self.jobs.lock().unwrap().push(job.clone());
        Ok(job)
    }

    pub fn cancel_all_jobs(&self) {
        let jobs = self.jobs.lock().unwrap();
        for job in jobs.iter() {
            job.cancel();
        }
    }
}

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =    Demo CLI WorkerToHostInterface   ===  ===  ===  ===  ===  ===  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// An example implementatoon that prints out response from the task in provided
// terminal foreground color.

pub struct CliInterface {
    color_code: &'static str,
}

impl CliInterface {
    pub fn new(color_code: &'static str) -> Self {
        Self { color_code }
    }
}

#[async_trait::async_trait]
impl WorkerToHostInterface for CliInterface {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
        println!("\r\n\x1b[31m[{}] Switched to state: {:?}\x1b[0m\r\n", worker_id, new_state);
        
        // Log failed states with error details
        if new_state == WorkerStates::InactiveFailed {
            eprintln!("\x1b[91m[{}] Worker failed - check error details\x1b[0m", worker_id);
        }
    }

    fn response_chunk_received(&self, _worker_id: Uuid, chunk: ModelResponseChunk) {
        match chunk {
            ModelResponseChunk::AssistantMessage(text) => {
                print!("{}{}\x1b[0m", self.color_code, text);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }
            ModelResponseChunk::ToolUseRequest { tool_name, parameters } => {
                print!("{}[Tool: {} - {}]\x1b[0m", self.color_code, tool_name, parameters);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }
        }
    }

    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, anyhow::Error> {
        println!("\r\n\x1b[33m[{}] Requested: {}\x1b[0m\r\n", worker_id, request);
        
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        
        tokio::select! {
            // Fun fact: while tokio::select will complete when cancellation token it triuggered,
            // reader.read_line in the next line won't, and it will wait for the user to click Enter
            // It looks even more fun when two tasks/jobs requested confirmation - whoever asked first
            // will get the first <Enter>.
            // It has to be replaced with more adwanced TUI solution in production
            result = reader.read_line(&mut line) => {
                result?;
                Ok(line.trim().to_string())
            }
            _ = cancellation_token.cancelled() => {
                Err(anyhow::anyhow!("Operation cancelled"))
            }
        }
    }
}

// CLI Interface constructor for the running demo app
// ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- ---- //
//  Something wannabe like 'The Main Core UI Implementation' thing.
//  Produces CliInterface's with different colors for different workers.

#[derive(Debug, Clone)]
pub enum AnsiColor {
    White,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
}

impl AnsiColor {
    fn to_ansi_code(&self) -> &'static str {
        match self {
            AnsiColor::White => "\x1b[37m",
            AnsiColor::Red => "\x1b[31m",
            AnsiColor::Green => "\x1b[32m",
            AnsiColor::Yellow => "\x1b[33m",
            AnsiColor::Blue => "\x1b[34m",
            AnsiColor::Magenta => "\x1b[35m",
            AnsiColor::Cyan => "\x1b[36m",
        }
    }
}

#[derive(Clone)]
pub struct CliUi;

impl CliUi {
    pub fn new() -> Self {
        Self
    }
    
    pub fn interface(&self, color: AnsiColor) -> CliInterface {
        CliInterface::new(color.to_ansi_code())
    }

    pub fn report_job_completion(&self, worker: Arc<Worker>, completion_type: WorkerJobCompletionType) -> impl Future<Output = ()> + Send {
        let worker_id = worker.id;
        async move {
            match completion_type {
                WorkerJobCompletionType::Normal => println!("CONTINUATION: Worker {} completed successfully", worker_id),
                WorkerJobCompletionType::Cancelled => println!("CONTINUATION: Worker {} was cancelled", worker_id),
                WorkerJobCompletionType::Failed => println!("CONTINUATION: Worker {} failed", worker_id),
            }
        }
    }
}

// Initialization functions, stage0 builders
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //

// "Build Session" here is basically "Build Single Model Provider"
// Production implementation would need more underlying components, more
// advanced logic, and so on
pub async fn build_session() -> Result<Session, anyhow::Error> {
    println!("Loading AWS configuration...");
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .load()
        .await;
    
    // Log AWS configuration details
    println!("AWS Configuration:");
    println!("  Region: {:?}", config.region());
    
    if config.credentials_provider().is_some() {
        println!("  Credentials provider: configured");
    } else {
        eprintln!("  Credentials provider: NOT FOUND");
        eprintln!("  Please run: aws configure");
        eprintln!("  Or set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY");
        return Err(anyhow::anyhow!("No AWS credentials provider found"));
    }
    
    let bedrock_client = BedrockClient::new(&config);
    println!("Bedrock client created successfully");
    
    let model_provider = BedrockConverseStreamModelProvider::new(bedrock_client);
    let model_providers = vec![model_provider];
    
    Ok(Session::new(model_providers))
}

pub fn build_ui() -> CliUi {
    CliUi::new()
}

fn print_all_workers(session: &Session) {
    let workers = session.workers.lock().unwrap();
    for worker in workers.iter() {
        let state = worker.get_state();
        let failure = worker.get_failure();
        let failure_text = match failure {
            Some(f) if !f.is_empty() => format!(" - {}", f),
            _ => String::new(),
        };
        println!("  {} - {:?}{}", worker.id, state, failure_text);
    }
}

// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
// =   DEMO PROTOTYPE `main`   ===  ===  ===  ===  ===  ===  ===  ===  ===  = //
// ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== ==== //
//
//  To try different behavior, modify prompts for the tasks to answer quicker
//  or longer, and abort_delay_seconds to send cancellation sooner or later.
//
//      Example 'long' prompts: 'lorem ipsum please, twice', 'introduce yourself'
//      Example 'quick' prompts: 'say hello', 'hi!'
//

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("Starting AI CLI architecture demo...");

    // STAGE 0: Initialization
    
    // Create Session object, initializes LLM connectivity
    let session = match build_session().await {
        Ok(s) => {
            println!("Session initialized successfully");
            s
        }
        Err(e) => {
            eprintln!("Failed to initialize session: {}", e);
            eprintln!("Please check your AWS credentials and network connection");
            return Err(e);
        }
    };

    // Create UI implementation
    let ui = build_ui();
    
    // STAGE 1-1: Create a (default) worker

    // Create new worker
    let worker = session.build_worker();
    println!("Worker #1 {} created", worker.id);
    
    // STAGE 1-2: Launch a demo task for the worker

    // Launch a job, passing UI interface to deal with necessary communications
    let job1 = session.run_demo_loop(
        worker.clone(),
        WorkerInput {
            prompt: "lorem ipsum please, twice".to_string(),
        },
        Arc::new(ui.interface(AnsiColor::Cyan)),
    )?;

    // STAGE 2-1: Create another (default) worker

    // Create new worker
    let worker2 = session.build_worker();
    println!("Worker #2 {} created", worker2.id);
    
    // STAGE 2-2: Launch a demo task for the worker

    // Launch a job, passing UI interface to deal with necessary communications
    let job2 = session.run_demo_loop(
        worker2.clone(),
        WorkerInput {
            prompt: "introduce yourself".to_string(),
        },
        Arc::new(ui.interface(AnsiColor::Green)),
    )?;

    // Add continuations:
    let ui_clone = ui.clone();
    job1.continuations.add_or_run_now(
        "completion_report",
        Continuations::boxed(move |worker, completion_type, _error_msg| ui_clone.report_job_completion(worker, completion_type)),
        job1.worker.clone(),
    ).await;

    let ui_clone = ui.clone();
    job2.continuations.add_or_run_now(
        "completion_report", 
        Continuations::boxed(move |worker, completion_type, _error_msg| ui_clone.report_job_completion(worker, completion_type)),
        job2.worker.clone(),
    ).await;
    
    // STAGE 3: Force abort the task after a delay

    let abort_delay_seconds = 15;
    println!("Job started, running for {} seconds...", abort_delay_seconds);
    tokio::time::sleep(tokio::time::Duration::from_secs(abort_delay_seconds)).await;
    
    // Cancel all jobs by triggering cancellation token

    println!("Workers BEFORE cancellation:");
    print_all_workers(&session);

    println!("Cancelling all jobs...");
    session.cancel_all_jobs();

    println!("Workers RIGHT AFTER cancellation:");
    print_all_workers(&session);

    // STAGE 4: Verify final state
    
    // Check all workers status after 1 second delay
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    println!("Workers LATE AFTER cancellation:");
    print_all_workers(&session);
    
    println!("Application completed successfully");
    Ok(())
}
