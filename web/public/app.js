// ============================================================================
// State Management Classes
// ============================================================================

/**
 * Worker data model
 */
class WorkerData {
    constructor(id, name, agent, state, currentJobId = null) {
        this.id = id;
        this.name = name;
        this.agent = agent;
        this.state = state;  // idle, busy, idle_failed
        this.currentJobId = currentJobId;
    }
}

/**
 * Application state management
 */
class WebUIState {
    constructor() {
        this.workers = new Map();  // worker_id -> WorkerData
        this.selectedWorkerId = null;
        this.conversations = new Map();  // worker_id -> ConversationEntry[]
        this.activeResponses = new Map();  // worker_id -> accumulated text
        this.connectionState = 'disconnected';  // disconnected, connecting, connected
    }
    
    // Worker management
    addWorker(worker) {
        this.workers.set(worker.id, worker);
    }
    
    removeWorker(workerId) {
        this.workers.delete(workerId);
        this.conversations.delete(workerId);
        this.activeResponses.delete(workerId);
    }
    
    updateWorkerState(workerId, newState) {
        const worker = this.workers.get(workerId);
        if (worker) {
            worker.state = newState;
        }
    }
    
    // Worker selection
    selectWorker(workerId) {
        this.selectedWorkerId = workerId;
    }
    
    getSelectedWorker() {
        return this.workers.get(this.selectedWorkerId);
    }
    
    // Conversation management
    setConversation(workerId, entries) {
        this.conversations.set(workerId, entries);
    }
    
    appendConversationEntry(workerId, entry) {
        const conversation = this.conversations.get(workerId) || [];
        conversation.push(entry);
        this.conversations.set(workerId, conversation);
    }
    
    getConversation(workerId) {
        return this.conversations.get(workerId) || [];
    }
}

// ============================================================================
// WebSocket Client
// ============================================================================

/**
 * WebSocket client with reconnection logic
 */
class WebSocketClient {
    constructor(app) {
        this.app = app;
        this.ws = null;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
        this.reconnectDelay = 1000;  // ms
    }
    
    connect() {
        this.app.state.connectionState = 'connecting';
        this.ws = new WebSocket(`ws://${window.location.host}/ws`);
        
        this.ws.onopen = () => {
            console.log('WebSocket connected');
            this.app.state.connectionState = 'connected';
            this.reconnectAttempts = 0;
            this.app.onConnected();
        };
        
        this.ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            this.app.handleEvent(data);
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
        };
        
        this.ws.onclose = () => {
            console.log('WebSocket closed');
            this.app.state.connectionState = 'disconnected';
            this.reconnect();
        };
    }
    
    reconnect() {
        if (this.reconnectAttempts >= this.maxReconnectAttempts) {
            console.error('Max reconnection attempts reached');
            return;
        }
        
        this.reconnectAttempts++;
        const delay = this.reconnectDelay * this.reconnectAttempts;
        
        console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
        setTimeout(() => this.connect(), delay);
    }
    
    send(command) {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(command));
        } else {
            console.error('WebSocket not connected');
        }
    }
}

// ============================================================================
// UI Components
// ============================================================================

/**
 * Worker list sidebar component
 */
class WorkerList {
    constructor(app) {
        this.app = app;
        this.element = document.getElementById('worker-list');
    }
    
    render() {
        const workers = Array.from(this.app.state.workers.values());
        
        this.element.innerHTML = workers
            .map(w => this.renderWorker(w))
            .join('');
        
        // Add event listeners
        this.element.querySelectorAll('.worker-item').forEach(item => {
            item.addEventListener('click', (e) => {
                const workerId = e.currentTarget.dataset.workerId;
                this.app.selectWorker(workerId);
            });
        });
    }
    
    renderWorker(worker) {
        const icon = this.getStateIcon(worker.state);
        const active = worker.id === this.app.state.selectedWorkerId ? 'active' : '';
        
        return `
            <div class="worker-item ${active}" data-worker-id="${worker.id}">
                <span class="worker-icon">${icon}</span>
                <div class="worker-info">
                    <div class="worker-name">${this.escapeHtml(worker.name)}</div>
                    <div class="worker-state">${worker.state}</div>
                </div>
            </div>
        `;
    }
    
    getStateIcon(state) {
        switch (state) {
            case 'idle': return '●';
            case 'busy': return '⚙';
            case 'idle_failed': return '✗';
            default: return '?';
        }
    }
    
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

/**
 * Conversation view component
 */
class ConversationView {
    constructor(app) {
        this.app = app;
        this.element = document.getElementById('conversation');
    }
    
    render() {
        const workerId = this.app.state.selectedWorkerId;
        if (!workerId) {
            this.element.innerHTML = '<div class="no-worker">Select a worker to view conversation</div>';
            return;
        }
        
        const entries = this.app.state.getConversation(workerId);
        this.element.innerHTML = entries
            .map(e => this.renderEntry(e))
            .join('');
        
        this.scrollToBottom();
    }
    
    renderEntry(entry) {
        const bubbleClass = this.getBubbleClass(entry.type);
        const content = this.formatContent(entry);
        
        return `
            <div class="bubble ${bubbleClass}">
                ${this.escapeHtml(content)}
            </div>
        `;
    }
    
    getBubbleClass(type) {
        switch (type) {
            case 'user_message': return 'bubble-user';
            case 'assistant_message': return 'bubble-assistant';
            case 'tool_use': return 'bubble-tool';
            case 'error': return 'bubble-error';
            default: return 'bubble-default';
        }
    }
    
    formatContent(entry) {
        if (entry.type === 'tool_use') {
            const tools = entry.tool_uses
                .map(t => `${t.tool_name}(${JSON.stringify(t.tool_input)})`)
                .join(', ');
            return `${entry.content}\n\nTools: ${tools}`;
        }
        return entry.content;
    }
    
    appendEntry(entry) {
        const html = this.renderEntry(entry);
        this.element.insertAdjacentHTML('beforeend', html);
        this.scrollToBottom();
    }
    
    updateLastEntry(content) {
        const lastBubble = this.element.lastElementChild;
        if (lastBubble) {
            lastBubble.textContent = content;
        }
    }
    
    scrollToBottom() {
        this.element.scrollTop = this.element.scrollHeight;
    }
    
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

/**
 * Input area component
 */
class InputArea {
    constructor(app) {
        this.app = app;
        this.input = document.getElementById('prompt-input');
        this.sendButton = document.getElementById('send-button');
        this.cancelButton = document.getElementById('cancel-button');
        
        this.setupEventListeners();
    }
    
    setupEventListeners() {
        this.sendButton.addEventListener('click', () => this.sendMessage());
        
        this.input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                this.sendMessage();
            }
        });
        
        this.cancelButton.addEventListener('click', () => this.cancelJob());
    }
    
    sendMessage() {
        const text = this.input.value.trim();
        if (!text) return;
        
        const workerId = this.app.state.selectedWorkerId;
        if (!workerId) {
            alert('Please select a worker');
            return;
        }
        
        // Add user message to conversation immediately
        const userEntry = {
            type: 'user_message',
            content: text,
            timestamp: Date.now() / 1000,
        };
        this.app.state.appendConversationEntry(workerId, userEntry);
        this.app.components.conversationView.appendEntry(userEntry);
        
        this.app.ws.send({
            type: 'prompt',
            worker_id: workerId,
            text: text,
        });
        
        this.input.value = '';
    }
    
    cancelJob() {
        const workerId = this.app.state.selectedWorkerId;
        if (!workerId) return;
        
        this.app.ws.send({
            type: 'cancel',
            worker_id: workerId,
        });
    }
    
    updateState() {
        const worker = this.app.state.getSelectedWorker();
        const isBusy = worker && worker.state === 'busy';
        
        this.input.disabled = isBusy;
        this.sendButton.disabled = isBusy;
        this.cancelButton.style.display = isBusy ? 'inline-block' : 'none';
    }
}

/**
 * New worker dialog component
 */
class NewWorkerDialog {
    constructor(app) {
        this.app = app;
        this.dialog = document.getElementById('new-worker-dialog');
        this.form = document.getElementById('new-worker-form');
        this.nameInput = document.getElementById('worker-name-input');
        this.agentInput = document.getElementById('worker-agent-input');
        this.workingDirInput = document.getElementById('worker-workdir-input');
        this.createButton = document.getElementById('create-worker-button');
        this.cancelButton = document.getElementById('cancel-dialog-button');
        
        this.setupEventListeners();
    }
    
    setupEventListeners() {
        this.createButton.addEventListener('click', () => this.createWorker());
        this.cancelButton.addEventListener('click', () => this.close());
        
        this.form.addEventListener('submit', (e) => {
            e.preventDefault();
            this.createWorker();
        });
        
        // Close on outside click
        this.dialog.addEventListener('click', (e) => {
            if (e.target === this.dialog) {
                this.close();
            }
        });
    }
    
    show() {
        this.dialog.style.display = 'flex';
        this.agentInput.focus();
    }
    
    close() {
        this.dialog.style.display = 'none';
        this.form.reset();
    }
    
    createWorker() {
        const agent = this.agentInput.value.trim();
        if (!agent) {
            alert('Agent name is required');
            return;
        }
        
        this.app.ws.send({
            type: 'create_worker',
            name: this.nameInput.value.trim() || null,
            agent: agent,
            working_directory: this.workingDirInput.value.trim() || null,
        });
        
        this.close();
    }
}

// ============================================================================
// Response Accumulator
// ============================================================================

/**
 * Accumulates streaming response chunks into single bubbles
 */
class ResponseAccumulator {
    constructor(app) {
        this.app = app;
        this.activeResponses = new Map();  // worker_id -> ResponseState
    }
    
    handleChunk(event) {
        const { worker_id, chunk } = event;
        
        if (chunk.chunk_type === 'assistant_response') {
            this.handleAssistantChunk(worker_id, chunk.text);
        } else if (chunk.chunk_type === 'tool_use') {
            this.handleToolUseChunk(worker_id, chunk);
        } else if (chunk.chunk_type === 'tool_result') {
            this.handleToolResultChunk(worker_id, chunk);
        }
    }
    
    handleAssistantChunk(workerId, text) {
        if (!this.activeResponses.has(workerId)) {
            // Start new response
            const entry = {
                type: 'assistant_message',
                content: text,
                timestamp: Date.now() / 1000,
            };
            
            this.activeResponses.set(workerId, {
                entry: entry,
                startTime: Date.now(),
            });
            
            // Create bubble in UI if this is the selected worker
            if (workerId === this.app.state.selectedWorkerId) {
                this.app.components.conversationView.appendEntry(entry);
            }
        } else {
            // Append to existing response
            const state = this.activeResponses.get(workerId);
            state.entry.content += text;
            
            // Update bubble in UI if this is the selected worker
            if (workerId === this.app.state.selectedWorkerId) {
                this.app.components.conversationView.updateLastEntry(state.entry.content);
            }
        }
    }
    
    handleToolUseChunk(workerId, chunk) {
        // Tool use is typically sent as single chunk, not streamed
        const entry = {
            type: 'tool_use',
            content: 'Using tool...',
            tool_uses: [{
                tool_name: chunk.tool_name,
                tool_input: chunk.tool_input,
            }],
            timestamp: Date.now() / 1000,
        };
        
        if (workerId === this.app.state.selectedWorkerId) {
            this.app.components.conversationView.appendEntry(entry);
        }
        
        this.app.state.appendConversationEntry(workerId, entry);
    }
    
    handleToolResultChunk(workerId, chunk) {
        // Tool result is typically sent as single chunk
        const entry = {
            type: 'tool_result',
            content: `Tool ${chunk.tool_name} result: ${chunk.result}`,
            timestamp: Date.now() / 1000,
        };
        
        if (workerId === this.app.state.selectedWorkerId) {
            this.app.components.conversationView.appendEntry(entry);
        }
        
        this.app.state.appendConversationEntry(workerId, entry);
    }
    
    finalize(workerId) {
        // Called on JobCompleted
        const state = this.activeResponses.get(workerId);
        if (state) {
            // Add to conversation history
            this.app.state.appendConversationEntry(workerId, state.entry);
            
            // Clear active response
            this.activeResponses.delete(workerId);
        }
    }
}

// ============================================================================
// Main Application
// ============================================================================

/**
 * Main application class
 */
class WebUIApp {
    constructor() {
        this.state = new WebUIState();
        this.ws = new WebSocketClient(this);
        this.components = {
            workerList: new WorkerList(this),
            conversationView: new ConversationView(this),
            inputArea: new InputArea(this),
            newWorkerDialog: new NewWorkerDialog(this),
        };
        this.accumulator = new ResponseAccumulator(this);
    }
    
    async init() {
        this.setupGlobalEventListeners();
        await this.ws.connect();
    }
    
    setupGlobalEventListeners() {
        document.getElementById('new-worker-button').addEventListener('click', () => {
            this.components.newWorkerDialog.show();
        });
    }
    
    onConnected() {
        // Initial snapshots are sent automatically by server
        console.log('Connected, waiting for initial snapshots');
    }
    
    selectWorker(workerId) {
        this.state.selectWorker(workerId);
        
        // Request conversation history if not loaded
        if (!this.state.conversations.has(workerId)) {
            this.ws.send({
                type: 'get_conversation_history',
                worker_id: workerId,
            });
        }
        
        this.components.workerList.render();
        this.components.conversationView.render();
        this.components.inputArea.updateState();
    }
    
    handleEvent(event) {
        console.log('Event received:', event.type);
        
        switch (event.type) {
            case 'workers_snapshot':
                this.handleWorkersSnapshot(event);
                break;
            case 'conversation_snapshot':
                this.handleConversationSnapshot(event);
                break;
            case 'worker_created':
                this.handleWorkerCreated(event);
                break;
            case 'worker_state_changed':
                this.handleWorkerStateChanged(event);
                break;
            case 'output_chunk':
                this.accumulator.handleChunk(event);
                break;
            case 'job_started':
                this.handleJobStarted(event);
                break;
            case 'job_completed':
                this.handleJobCompleted(event);
                break;
            case 'error':
                this.handleError(event);
                break;
            default:
                console.log('Unhandled event type:', event.type);
        }
    }
    
    handleWorkersSnapshot(event) {
        event.workers.forEach(w => {
            this.state.addWorker(new WorkerData(
                w.id, w.name, w.agent, w.state, null
            ));
        });
        
        // Auto-select first worker if none selected
        if (!this.state.selectedWorkerId && event.workers.length > 0) {
            this.selectWorker(event.workers[0].id);
        }
        
        this.components.workerList.render();
    }
    
    handleConversationSnapshot(event) {
        this.state.setConversation(event.worker_id, event.entries);
        
        if (event.worker_id === this.state.selectedWorkerId) {
            this.components.conversationView.render();
        }
    }
    
    handleWorkerCreated(event) {
        this.state.addWorker(new WorkerData(
            event.worker_id, event.name, 'default', 'idle', null
        ));
        this.components.workerList.render();
        
        // Auto-select new worker
        this.selectWorker(event.worker_id);
    }
    
    handleWorkerStateChanged(event) {
        this.state.updateWorkerState(event.worker_id, event.new_state);
        this.components.workerList.render();
        this.components.inputArea.updateState();
    }
    
    handleJobStarted(event) {
        const worker = this.state.workers.get(event.worker_id);
        if (worker) {
            worker.currentJobId = event.job_id;
        }
        this.components.inputArea.updateState();
    }
    
    handleJobCompleted(event) {
        const worker = this.state.workers.get(event.worker_id);
        if (worker) {
            worker.currentJobId = null;
        }
        this.accumulator.finalize(event.worker_id);
        
        // Display error message if job failed
        if (event.result.status === 'failed') {
            const errorEntry = {
                type: 'error',
                content: event.result.error,
                timestamp: event.timestamp,
            };
            this.state.appendConversationEntry(event.worker_id, errorEntry);
            if (event.worker_id === this.state.selectedWorkerId) {
                this.components.conversationView.appendEntry(errorEntry);
            }
        }
        
        this.components.inputArea.updateState();
    }
    
    handleError(event) {
        alert(`Error: ${event.message}`);
        console.error('Command error:', event);
    }
}

// ============================================================================
// Initialize app on page load
// ============================================================================

let app;
document.addEventListener('DOMContentLoaded', () => {
    app = new WebUIApp();
    app.init();
});
