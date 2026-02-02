import * as pty from 'bun-pty';
import { createServer } from 'http';
import { WebSocketServer } from 'ws';
import { readFileSync } from 'fs';

const PORT = 3000;
const COLS = 160;
const ROWS = 50;

const server = createServer((req, res) => {
  if (req.url === '/' || req.url === '/shell.html') {
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(readFileSync('./shell.html'));
  } else {
    res.writeHead(404);
    res.end('Not found');
  }
});

const wss = new WebSocketServer({ server });

wss.on('connection', (ws) => {
  const shell = process.env.SHELL || '/bin/bash';
  const ptyProcess = pty.spawn(shell, ['-l'], {
    name: 'xterm-256color',
    cols: COLS,
    rows: ROWS,
    cwd: process.cwd(),
    env: process.env as Record<string, string>,
  });

  ptyProcess.onData((data) => ws.send(data));
  ptyProcess.onExit(() => ws.close());
  
  ws.on('message', (data) => ptyProcess.write(data.toString()));
  ws.on('close', () => ptyProcess.kill());
});

server.listen(PORT, () => {
  console.log(`Server running at http://localhost:${PORT}`);
});
