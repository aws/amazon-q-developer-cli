use axum::{
    extract::{ws::WebSocket, WebSocketUpgrade},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    time::{timeout, Duration},
};
use tracing::{error, info};
use uuid::Uuid;

// Function to strip non-ASCII characters and ANSI escape sequences
fn sanitize_output(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            // Handle ANSI escape sequences - skip them entirely
            '\x1b' => {
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    // Skip the entire ANSI sequence
                    while let Some(next_ch) = chars.next() {
                        if next_ch.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                // Don't add anything to result - just skip
            }
            // Keep only ASCII printable characters, tabs, newlines, and carriage returns
            c if c.is_ascii() && (c.is_ascii_graphic() || c == ' ' || c == '\t' || c == '\n' || c == '\r') => {
                result.push(c);
            }
            // Skip all other characters (non-ASCII and control characters)
            _ => {}
        }
    }
    
    result
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Message {
    #[serde(rename = "command")]
    Command { data: String },
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "error")]
    Error { data: String },
    #[serde(rename = "exit")]
    Exit { code: i32 },
}

#[derive(Debug)]
struct Terminal {
    id: String,
    shell_process: Option<Child>,
    current_dir: String,
    env_vars: HashMap<String, String>,
}

impl Terminal {
    fn new() -> Self {
        let mut env_vars = HashMap::new();
        for (key, value) in env::vars() {
            env_vars.insert(key, value);
        }
        
        Self {
            id: Uuid::new_v4().to_string(),
            shell_process: None,
            current_dir: env::current_dir()
                .unwrap_or_else(|_| "/".into())
                .to_string_lossy()
                .to_string(),
            env_vars,
        }
    }

    fn get_prompt(&self) -> String {
        let username = self.env_vars.get("USER")
            .or_else(|| self.env_vars.get("USERNAME"))
            .map(|s| s.as_str())
            .unwrap_or("user");
        
        let hostname = self.env_vars.get("HOSTNAME")
            .or_else(|| self.env_vars.get("COMPUTERNAME"))
            .map(|s| s.as_str())
            .unwrap_or("localhost");
        
        let home_dir = self.env_vars.get("HOME")
            .map(|s| s.as_str())
            .unwrap_or("/");
            
        let current_dir = if self.current_dir == home_dir {
            "~".to_string()
        } else if self.current_dir.starts_with(home_dir) {
            format!("~{}", &self.current_dir[home_dir.len()..])
        } else {
            self.current_dir.clone()
        };
        
        format!("{}@{} [Q]:{}", username, hostname, current_dir)
    }

    async fn start_shell(&mut self, chat_args: Vec<String>) -> Result<(mpsc::Sender<String>, mpsc::Receiver<String>), String> {
        // Start Q chat process with provided arguments
        let mut cmd = Command::new("q");
        cmd.args(["chat"]);
        
        // Add any additional chat arguments
        for arg in chat_args {
            cmd.arg(arg);
        }
        
        let mut child = cmd
            .current_dir(&self.current_dir)
            .envs(&self.env_vars)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start Q chat: {}", e))?;

        let stdin = child.stdin.take().ok_or("Failed to get stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to get stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to get stderr")?;

        // Create channels for communication
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<String>(100);
        let (output_tx, output_rx) = mpsc::channel::<String>(100);

        // Store the child process
        self.shell_process = Some(child);

        // Spawn task to handle stdin (commands to Q chat)
        let output_tx_stdin = output_tx.clone();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(command) = cmd_rx.recv().await {
                // Send the command to Q chat
                if let Err(e) = stdin.write_all(format!("{}\n", command).as_bytes()).await {
                    let _ = output_tx_stdin.send(format!("Error writing to Q chat: {}", e)).await;
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    let _ = output_tx_stdin.send(format!("Error flushing Q chat input: {}", e)).await;
                    break;
                }
            }
        });

        // Spawn task to handle stdout (responses from Q chat)
        let output_tx_stdout = output_tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = Vec::new();
            
            loop {
                buffer.clear();
                match timeout(Duration::from_secs(30), reader.read_until(b'\n', &mut buffer)).await {
                    Ok(Ok(0)) => break, // EOF
                    Ok(Ok(_)) => {
                        if let Ok(line) = String::from_utf8(buffer.clone()) {
                            // Sanitize the output before sending to browser
                            let sanitized_line = sanitize_output(&line);
                            if let Err(_) = output_tx_stdout.send(sanitized_line).await {
                                break;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = output_tx_stdout.send(format!("Error reading from Q chat: {}", e)).await;
                        break;
                    }
                    Err(_) => {
                        // Timeout - Q chat might be thinking, continue waiting
                        continue;
                    }
                }
            }
        });

        // Spawn task to handle stderr (errors from Q chat)
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = Vec::new();
            
            loop {
                buffer.clear();
                match timeout(Duration::from_secs(5), reader.read_until(b'\n', &mut buffer)).await {
                    Ok(Ok(0)) => break, // EOF
                    Ok(Ok(_)) => {
                        if let Ok(line) = String::from_utf8(buffer.clone()) {
                            if !line.trim().is_empty() {
                                // Sanitize error output and send without prefix
                                let sanitized_line = sanitize_output(&line);
                                let _ = output_tx.send(sanitized_line).await;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = output_tx.send(format!("Error reading Q chat stderr: {}", e)).await;
                        break;
                    }
                    Err(_) => {
                        // Timeout - continue reading
                        continue;
                    }
                }
            }
        });

        Ok((cmd_tx, output_rx))
    }

    async fn handle_builtin_command(&mut self, command: &str) -> Option<Result<String, String>> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Some(Ok(String::new()));
        }

        match parts[0] {
            "cd" => {
                let target_dir = if parts.len() > 1 {
                    parts[1].to_string()
                } else {
                    // Default to home directory
                    self.env_vars.get("HOME").unwrap_or(&"/".to_string()).clone()
                };

                match self.change_directory(&target_dir) {
                    Ok(_) => Some(Ok(String::new())),
                    Err(e) => Some(Err(format!("cd: {}", e))),
                }
            }
            "pwd" => Some(Ok(format!("{}\n", self.current_dir))),
            "export" => {
                if parts.len() > 1 {
                    for part in &parts[1..] {
                        if let Some((key, value)) = part.split_once('=') {
                            self.env_vars.insert(key.to_string(), value.to_string());
                        }
                    }
                }
                Some(Ok(String::new()))
            }
            _ => None, // Not a builtin command
        }
    }

    fn change_directory(&mut self, path: &str) -> Result<(), String> {
        let new_path = if path.starts_with('/') {
            // Absolute path
            std::path::PathBuf::from(path)
        } else if path.starts_with("~/") {
            // Home directory relative path
            let home = self.env_vars.get("HOME").ok_or("HOME not set")?;
            std::path::PathBuf::from(home).join(&path[2..])
        } else if path == "~" {
            // Just home directory
            let home = self.env_vars.get("HOME").ok_or("HOME not set")?;
            std::path::PathBuf::from(home)
        } else {
            // Relative path
            std::path::PathBuf::from(&self.current_dir).join(path)
        };

        let canonical_path = new_path.canonicalize()
            .map_err(|e| format!("No such file or directory: {}", e))?;

        if !canonical_path.is_dir() {
            return Err("Not a directory".to_string());
        }

        self.current_dir = canonical_path.to_string_lossy().to_string();
        Ok(())
    }
}

async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    let mut terminal = Terminal::new();
    let (sender, mut receiver) = socket.split();
    let sender = std::sync::Arc::new(tokio::sync::Mutex::new(sender));
    
    // Start the Q chat shell with default arguments
    let (cmd_tx, mut output_rx) = match terminal.start_shell(vec![]).await {
        Ok((tx, rx)) => (tx, rx),
        Err(e) => {
            error!("Failed to start shell: {}", e);
            let mut sender_guard = sender.lock().await;
            let _ = sender_guard.send(axum::extract::ws::Message::Text(
                serde_json::to_string(&Message::Error { 
                    data: format!("Failed to start Q chat: {}", e) 
                }).unwrap()
            )).await;
            return;
        }
    };

    // Send initial prompt
    let initial_prompt = terminal.get_prompt();
    {
        let mut sender_guard = sender.lock().await;
        let _ = sender_guard.send(axum::extract::ws::Message::Text(
            serde_json::to_string(&Message::Output { 
                data: format!("{} $ ", initial_prompt) 
            }).unwrap()
        )).await;
    }

    // Spawn task to handle output from Q chat
    let sender_clone = sender.clone();
    tokio::spawn(async move {
        while let Some(output) = output_rx.recv().await {
            let message = Message::Output { data: output };
            if let Ok(json) = serde_json::to_string(&message) {
                let mut sender_guard = sender_clone.lock().await;
                if sender_guard.send(axum::extract::ws::Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming WebSocket messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Text(text)) => {
                if let Ok(message) = serde_json::from_str::<Message>(&text) {
                    match message {
                        Message::Command { data } => {
                            // Check for builtin commands first
                            if let Some(result) = terminal.handle_builtin_command(&data).await {
                                match result {
                                    Ok(output) => {
                                        if !output.is_empty() {
                                            let msg = Message::Output { data: output };
                                            if let Ok(json) = serde_json::to_string(&msg) {
                                                let mut sender_guard = sender.lock().await;
                                                let _ = sender_guard.send(axum::extract::ws::Message::Text(json)).await;
                                            }
                                        }
                                        // Send new prompt
                                        let prompt = terminal.get_prompt();
                                        let msg = Message::Output { data: format!("{} $ ", prompt) };
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let mut sender_guard = sender.lock().await;
                                            let _ = sender_guard.send(axum::extract::ws::Message::Text(json)).await;
                                        }
                                    }
                                    Err(error) => {
                                        let msg = Message::Error { data: error };
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let mut sender_guard = sender.lock().await;
                                            let _ = sender_guard.send(axum::extract::ws::Message::Text(json)).await;
                                        }
                                        // Send new prompt even after error
                                        let prompt = terminal.get_prompt();
                                        let msg = Message::Output { data: format!("{} $ ", prompt) };
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let mut sender_guard = sender.lock().await;
                                            let _ = sender_guard.send(axum::extract::ws::Message::Text(json)).await;
                                        }
                                    }
                                }
                            } else {
                                // Send command to Q chat
                                if cmd_tx.send(data).await.is_err() {
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("WebSocket connection closed");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

pub async fn start_web_server(port: u16, _chat_args: Vec<String>) -> eyre::Result<()> {
    info!("Starting web terminal server on port {}", port);
    
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(websocket_handler));

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    
    info!("Web terminal available at http://127.0.0.1:{}", port);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
