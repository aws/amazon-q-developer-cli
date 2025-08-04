# Web Terminal

The web-terminal crate provides a web-based interface for Amazon Q CLI, allowing users to interact with Amazon Q through a browser instead of the command line.

## Overview

The web terminal creates a WebSocket-based terminal emulator that runs in the browser, providing a familiar terminal experience while leveraging the full power of Amazon Q CLI.

## Features

- **WebSocket Communication**: Real-time bidirectional communication between browser and terminal
- **Terminal Emulation**: Full terminal emulation with support for colors, cursor movement, and control sequences
- **Process Management**: Spawns and manages shell processes with proper I/O handling
- **Cross-Platform**: Works on macOS and Linux
- **Responsive Design**: Adapts to different screen sizes and devices

## Architecture

The web terminal consists of several key components:

### Server Components

- **Web Server**: Axum-based HTTP server that serves the web interface
- **WebSocket Handler**: Manages WebSocket connections and message routing
- **Terminal Manager**: Spawns and manages terminal processes
- **Process I/O**: Handles stdin/stdout/stderr communication with spawned processes

### Client Components

- **HTML Interface**: Terminal emulator UI built with HTML/CSS/JavaScript
- **WebSocket Client**: Handles communication with the server
- **Terminal Renderer**: Renders terminal output and handles user input

## Usage

### Starting the Web Terminal

Use the `webchat` command to start the web terminal server:

```bash
q webchat --port 8080
```

Available options:
- `--port, -p`: Port to run the web server on (default: 8080)
- `--agent`: Context profile to use
- `--model`: Current model to use
- `--trust-all-tools, -a`: Allow all tools without confirmation
- `--trust-tools`: Trust only specific tools

### Accessing the Interface

Once started, open your browser and navigate to:
```
http://localhost:8080
```

The interface provides a full terminal experience where you can interact with Amazon Q just as you would in the command line.

## Implementation Details

### WebSocket Protocol

The web terminal uses a simple JSON-based protocol over WebSocket:

```json
{
  "type": "input",
  "data": "user input text"
}
```

```json
{
  "type": "output", 
  "data": "terminal output text"
}
```

### Process Management

Each WebSocket connection spawns its own shell process using `/bin/bash`.

The process lifecycle is managed automatically:
- Process is spawned when WebSocket connects
- Process is terminated when WebSocket disconnects
- I/O is handled asynchronously using Tokio

### Security Considerations

- The web terminal runs locally and binds to `127.0.0.1` by default
- No authentication is required for local access
- All terminal operations run with the same permissions as the user who started the server
- CORS is configured to allow local access only

## Development

### Building

The web-terminal crate is built as part of the main Amazon Q CLI build:

```bash
cargo build
```

### Testing

Run tests for the web-terminal crate:

```bash
cargo test --package web-terminal
```

### Dependencies

Key dependencies include:
- `axum`: Web framework for HTTP server
- `tokio-tungstenite`: WebSocket implementation
- `tokio`: Async runtime
- `serde`: JSON serialization
- `tower-http`: HTTP middleware

## Troubleshooting

### Common Issues

**Port Already in Use**
```
Error: Address already in use (os error 48)
```
Solution: Use a different port with `--port` option or stop the process using the port.

**WebSocket Connection Failed**
- Check that the server is running
- Verify the correct port is being used
- Ensure no firewall is blocking the connection

**Terminal Not Responding**
- Refresh the browser page to reconnect
- Check server logs for error messages
- Restart the web terminal server

### Logging

Enable verbose logging to debug issues:

```bash
q webchat -v
```

This will show detailed information about WebSocket connections, process spawning, and I/O operations.

## Future Enhancements

Potential improvements for the web terminal:

- **Authentication**: Add optional authentication for remote access
- **Multiple Sessions**: Support multiple terminal sessions in tabs
- **File Upload/Download**: Direct file transfer capabilities
- **Themes**: Customizable terminal themes and colors
- **Keyboard Shortcuts**: Additional keyboard shortcuts and hotkeys
- **Session Persistence**: Save and restore terminal sessions
