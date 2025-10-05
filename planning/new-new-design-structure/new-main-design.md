# Q CLI Re-architect prototype

The goal of this project is to build minimal functionl proof-of-concept of Q CLI-like command line agent.

I want to build a CLI app with minimal UX, but some paths to extend it in following iterations.


## What I want to see in the code

Here is my pseudo-code showing components and their interactions:

```
fun main() {
    // Creates Session object, initializes LLM connectivity (and otehr required support structures)
    // Basically the app init point for 'internals'
    val session = build_session();

    // Create new worker (in future will take more arguments to finetune workers for their tasks)
    val worker = session.build_worker();
    
    // Create UI 'implementation'
    // Basically the app init point for user-facing things
    val ui = build_ui();

    // Launch a job, passing UI 'interface' to deal with necessary communications
    val job = session.run(
        worker, 
        new WorkerInput{prompt = "Create lorem ipsum 10 times"}, 
        ui.inerface
    );

    sleep(10_000); // sleep for 10 seconds
    session.cancel(job); // aborts the session by triggering cancellation token associated with the job
}

class Session {
    model_providers: List<ModelProvider>, // READ-ONLY after init - no concurrency guards needed
    workers: List<Worker>, // CONCURRENT WRITES - needs synchronization for add/remove operations
    jobs: List<Job>, // CONCURRENT WRITES - needs synchronization for add/remove operations
    threads: ThreadPool, // THREAD-SAFE by design - no additional guards needed
    // later we will have tools host, config reader, conversation store, etc

    fun build_worker() -> Worker {
        val worker = new Worker{
            worker_id = UUID.new(),
            worker_name = "Test worker",
            model_provider = this.model_providers.first(),
            worker_state = WorkerStates.INACTIVE,
            // later we will build tools provider, conversation state wrapper, load config, etc
        };
        this.workers.add(worker);
        return worker;
    }

    fun run(worker:Worker, input: WorkerInput, ui_interface: UiInterface) -> Job {
        val cancellation_token = createCancellationToken();
        val worker_loop = new WorkerProtoLoop{
            worker, input,
            ui_interface,
            cancellation_token,
        };
        val thread_job = this.threads.schedule(
            worker_loop.run
        );
        val job = new WorkerJob{
            worker,
            cancellation_token,
            thread_job,
            worker_loop as WorkerTask,
        }
        this.jobs.add(job)
        thread_job.onComplete += cleanupJob(job);
        thread_job.launch();
        return job;
    }

    fun cancel(job: Job) {
        if (job.thread_job.isStillActive()) {
            job.cancelationToken.triggerCancellation();
        }
    }

    private fun cleanupJob(job: Job) {
        this.jobs.remove(job);
    }
}

class Worker {
    readonly worker_id: UUID,
    readonly worker_name: String,
    model_provider: ModelProvider, // Will be able to switch it in the future, but only when in state INACTIVE
    worker_state: WorkerStates, // CONCURRENT WRITES - needs synchronization (modified by WorkerProtoLoop)
    last_failure: Exception,  // CONCURRENT WRITES - needs synchronization (modified by WorkerProtoLoop)
}

enum WorkerStates {
    INACTIVE,
    WORKING,
    REQUESTING,
    RECEIVING,
    WAITING,
    USING_TOOL,
    INACTIVE_FAILED,
}

class WorkerInput {
    prompt: String,
}

interface WorkerTask {
    fun getWorker() -> Worker;
    async fun run();
}

class WorkerProtoLoop: WorkerTask {
    worker: Worker, 
    input: WorkerInput,
    host_interface: WorkerToHostInterface,
    cancellation_token: CancellationToken(System),

    fun getWorker() -> Worker { return this.worker; }

    private fun setState(state: WorkerStates) {
        this.worker.state = state;
        host_interface.workerStateChange(
            this.worker.id,
            this.worker.state,
        );
    }

    async fun run() {
        try {
            this.worker.last_failure = None;
            this.setState(WORKING);
            // Build model request
            val model_request = this.build_request(input);
            this.setState(REQUESTING);
            val response = await this.worker.model_provider.request(
                model_request,
                () -> this.setState(RECEIVING),
                (parsed_chunk) -> this.host_interface.responseChunkReceived(this.worker.id, parsed_chunk),
                this.cancellation_token,
            );
            // Later we will add conversation state update here
            // We will run tool use requests through tool provider approval process, and then invoke host_interface to request an approval if needed
            this.setState(WAITING);
            val ui_response = await this.host_interface.getToolConfirmation(
                this.worker.id,
                "hello from the worker",
                this.cancellation_token,
            );
            // ...and then we will actually use the tool
            this.setState(INACTIVE);
        } catch (exception: Exception) {
            this.worker.lastFailure = exception;
            this.setState(INACTIVE_FAILED);
        }
    }
}

struct WorkerJob {
    worker: Worker,
    cancellation_token: CancellationToken(System),
    thread_job: ThreadJob(System),
    worker_loop as WorkerTask,
}

interface WorkerToHostInterface {
    fun workerStateChange(worker_id: UUID, new_state:WorkerStates);
    fun responseChunkReceived(worker_id: UUID, response_chunk: ModelResponseChunk);
    async fun getToolConfirmation(worker_id: UUID, request: String, cancellation_token: CancellationToken(System)) -> String;
}

class CliInterface: WorkerToHostInterface {
    fun workerStateChange(worker_id: UUID, new_state:WorkerStates){
        stdio::print("\r\n{RED:ON}Switched to state: ${new_state}{RED:OFF}\r\n")
    }

    fun responseChunkReceived(worker_id: UUID, response_chunk: ModelResponseChunk){
        stdio::print(response_chunk.text);
    }

    async fun getToolConfirmation(worker_id: UUID, request: String, cancellation_token: CancellationToken(System)) -> String {
        stdio::print("\r\n{YELLOW:ON}Requested: ${request}{YELLOW:OFF}\r\n");
        // Note for code generation: `stdio::readLineAsync()` is an imaginary function. What we need here is for the UI to 
        // either wait for user to type somthing and click 'Enter', or for cancellation token to be invoked.
        // If it was cancellation token, we need to return null or something like that, assuming the caller (the worker loop)
        // will also check cancellation token
        result = await await_for_first(
            stdio::readLineAsync(),
            cancellation_token
        );
        return result;
    }
}

class CliUi {
    interface: CliInterface;
}

interface ModelProvider {
    fun request(
                request: ModelRequest,
                when_receiving_begin: Func<void>,
                when_received: Func<ModelResponseChunk, void>,
                cancellation_token: CancellationToken(System),
        ) -> ModelResponse;
}

class BedrockConverseStreamModelProvider {
    client: BedrockClient,
    model_id: String,

    fun request(
            request: ModelRequest,
            when_receiving_begin: Func<void>,
            when_received: Func<ModelResponseChunk, void>,
            cancellation_token: CancellationToken(System),
    ) -> ModelResponse {
        // convert incoming `request` into ConverseStream request
        // send request
        // when begin receiving - invoke when_receiving_begin()
        // receive and parse response
        //     accumulate received pieces in memory
        //     for each piece invoke when_received(piece)
        // return accumulated data as ModelResponse
        // IMPORTANT! Each long running step must account for cancellation token
    }
}

struct ModelRequest{
    // Simplified version of Bedrock model request, onluy focused on user messages for now
}
struct ModelResponseChunk{
    // Simple signal: type, one of assistantMessage or toolUseRequest, plus the text payload from the received stream chunk
}
struct ModelResponse{
    // Simplified version of Bedrock model response. Initially limited to received assistant messages and tool use requests
}

```
