/**
 * Knight Rider — LLM-driven exploratory test server.
 *
 * Starts HTTP server immediately, boots target process async.
 * Captures named frames (screenshots) persisted to disk as HTML.
 * Dashboard replays frames as a video.
 *
 * Usage:
 *   bun run knight-rider                          # launch TUI from source (default, picks up local changes)
 *   bun run knight-rider --system                 # launch using system kiro-cli binary
 *   bun run knight-rider --v1                     # launch kiro-cli v1 (legacy Rust TUI)
 *   bun run knight-rider --cmd "bash"             # launch any command
 *   bun run knight-rider --port 4000              # custom port
 *   bun run knight-rider --out /tmp/my-test       # custom output dir
 *
 * API:
 *   GET  /                    → dashboard (live view + frame replay)
 *   GET  /report              → self-contained evidence report (works offline)
 *   WS   /ws                  → raw PTY stream for live viewer
 *   GET  /api/status          → { ready, error, frameCount, outputDir }
 *   GET  /api/screen          → { lines: string[] }
 *   GET  /api/screen/html     → rendered HTML with colors
 *   GET  /api/frames          → all captured frames
 *   POST /api/keys            → { keys: string }  — send keystrokes
 *   POST /api/enter           → press Enter
 *   POST /api/escape          → press Escape (close panels/menus)
 *   POST /api/ctrlc           → press Ctrl+C (cancel)
 *   POST /api/up              → press Arrow Up (navigate panels/menus)
 *   POST /api/down            → press Arrow Down (scroll panels, navigate pickers)
 *   POST /api/wait-for-text   → { text, timeout? }  — block until text visible
 *   POST /api/sleep           → { ms }
 *   POST /api/frame           → { label }  — capture named screenshot to disk
 *
 * V2 TUI interaction patterns:
 *   Slash commands:  type "/" → autocomplete menu appears → type to filter → Enter to select
 *   Agent picker:    /agent → Enter → interactive picker → type to filter → Enter to select
 *   Tools panel:     /tools → Enter → scrollable panel → ↑↓ to scroll → ESC to close
 *   Panels/overlays: ALWAYS capture a frame BEFORE pressing ESC to close
 *   Prompt input:    type char by char with small delays (30-50ms) for TUI to process
 *
 * Evidence output:
 *   Each frame is saved as .html (with colors) and .txt (plain text).
 *   index.html is a self-contained report with ▶ Play video replay.
 *   Works offline after server stops — just open index.html in a browser.
 */

import * as fs from 'fs';
import * as path from 'path';
import { PtyManager } from '../src/test-utils/shared/pty-manager';

function hasFlag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}
function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i !== -1 ? process.argv[i + 1] : undefined;
}

const PORT = parseInt(arg('port') ?? '3001');
const CMD = arg('cmd');
const USE_V1 = hasFlag('v1');
const USE_SYSTEM = hasFlag('system');
const OUTPUT_DIR = arg('out') ?? path.join(__dirname, 'test-outputs', `knight-rider-${Date.now()}`);
const WIDTH = 120;
const HEIGHT = 40;

// Create output dir
fs.mkdirSync(OUTPUT_DIR, { recursive: true });

// Resolve the actual command to run.
// Default: run TUI from source via bun so local changes are picked up.
// --system: use the system kiro-cli binary instead.
const REPO_ROOT = path.resolve(__dirname, '../../..');
const CARGO_BIN = path.join(REPO_ROOT, 'target/debug/chat_cli');

function resolveCommand(): { cmd: string; env: Record<string, string> } {
  if (CMD) return { cmd: CMD, env: {} };
  if (USE_V1) return { cmd: 'kiro-cli chat', env: {} };
  if (USE_SYSTEM) return { cmd: 'kiro-cli chat --tui', env: {} };
  // Default: run TUI source directly with local Rust binary
  return {
    cmd: 'bun ./src/index.tsx',
    env: { KIRO_AGENT_PATH: CARGO_BIN },
  };
}

const { cmd: resolvedCmd, env: extraEnv } = resolveCommand();

// ── State ────────────────────────────────────────────────────────

let pty: PtyManager | null = null;
let ready = false;
let bootError: string | null = null;

interface Frame {
  label: string;
  timestamp: number;
  text: string[];
  html: string;
}

const frames: Frame[] = [];
const wsClients = new Set<any>();

function captureFrame(label: string): Frame {
  if (!pty) throw new Error(bootError ?? 'still booting');
  const frame: Frame = {
    label,
    timestamp: Date.now(),
    text: pty.getSnapshot(),
    html: pty.getSnapshotHtml(),
  };
  frames.push(frame);

  // Persist to disk
  const idx = String(frames.length).padStart(3, '0');
  const safeName = label.replace(/[^a-zA-Z0-9_-]/g, '_');
  fs.writeFileSync(path.join(OUTPUT_DIR, `${idx}-${safeName}.html`), frame.html);
  fs.writeFileSync(path.join(OUTPUT_DIR, `${idx}-${safeName}.txt`), frame.text.join('\n'));

  // Update index
  writeIndex();
  return frame;
}

function writeIndex() {
  const html = generateReportHtml();
  fs.writeFileSync(path.join(OUTPUT_DIR, 'index.html'), html);
}

function generateReportHtml(): string {
  if (!frames.length) return '<html><body>No frames captured yet.</body></html>';
  const t0 = frames[0]!.timestamp;
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<title>KNIGHT RIDER — Evidence</title>
<style>
  * { box-sizing:border-box; margin:0; padding:0; }
  body { background:#0d1117; color:#c9d1d9; font-family:system-ui; height:100vh; display:flex; flex-direction:column; }
  #header { padding:12px 16px; background:#161b22; border-bottom:1px solid #30363d; display:flex; align-items:center; gap:16px; position:relative; overflow:hidden; }
  #header h1 { font-size:16px; }
  #scanner { position:absolute; bottom:0; left:0; width:100%; height:3px; }
  #scanner::after { content:''; position:absolute; width:80px; height:100%; background:linear-gradient(90deg,transparent,#ff1a1a,transparent); animation:scan 2s ease-in-out 3 forwards; }
  @keyframes scan { 0%{left:-80px} 50%{left:calc(100%)} 100%{left:-80px} }
  #controls { display:flex; align-items:center; gap:8px; }
  #controls button { padding:4px 12px; border-radius:4px; border:1px solid #30363d; background:#21262d; color:#c9d1d9; cursor:pointer; }
  #controls button:hover { background:#30363d; }
  #controls button.active { background:#1f6feb; border-color:#1f6feb; }
  #controls span { font-size:12px; color:#8b949e; }
  #main { flex:1; display:flex; overflow:hidden; }
  #sidebar { width:280px; border-right:1px solid #30363d; overflow-y:auto; padding:8px; }
  .frame-item { padding:8px 10px; border-radius:4px; cursor:pointer; font-size:13px; margin-bottom:4px; display:flex; justify-content:space-between; align-items:center; }
  .frame-item:hover { background:#161b22; }
  .frame-item.active { background:#1f6feb33; border:1px solid #1f6feb; }
  .frame-item .label { font-weight:500; }
  .frame-item .meta { color:#8b949e; font-size:11px; }
  #viewer { flex:1; overflow:auto; padding:16px; }
  #viewer pre { font-family:'SF Mono',Monaco,'Cascadia Code',monospace; font-size:13px; line-height:1.4; background:#010409; padding:12px; border-radius:6px; border:1px solid #30363d; }
</style>
</head>
<body>
<div id="header">
  <h1><span style="color:#ff1a1a;font-weight:800;letter-spacing:2px">KNIGHT RIDER</span> — ${frames.length} frames</h1>
  <div id="controls">
    <button onclick="play()" id="playBtn">▶ Play</button>
    <button onclick="stop()">⏹</button>
    <button onclick="prev()">◀</button>
    <button onclick="next()">▶</button>
    <input type="range" id="speed" min="200" max="3000" value="1000" style="width:80px">
    <span id="speedLabel">1.0s</span>
    <span id="info"></span>
  </div>
  <div id="scanner"></div>
</div>
<div id="main">
  <div id="sidebar">${frames.map((f, i) => {
    const dt = ((f.timestamp - t0) / 1000).toFixed(1);
    return `<div class="frame-item" onclick="show(${i})" id="fi${i}"><span class="label">#${i + 1} ${f.label}</span><span class="meta">${dt}s</span></div>`;
  }).join('')}</div>
  <div id="viewer" id="viewer"></div>
</div>
<script>
const frames = ${JSON.stringify(frames.map(f => f.html))};
let cur = -1, timer = null;
function show(i) {
  cur = i;
  document.getElementById('viewer').innerHTML = frames[i];
  document.querySelectorAll('.frame-item').forEach((el,j) => el.classList.toggle('active', j===i));
  document.getElementById('info').textContent = '#'+(i+1)+'/'+frames.length;
}
function next() { if (cur < frames.length-1) show(cur+1); }
function prev() { if (cur > 0) show(cur-1); }
function play() {
  stop();
  document.getElementById('playBtn').classList.add('active');
  if (cur < 0 || cur >= frames.length-1) show(0); else next();
  timer = setInterval(() => {
    if (cur >= frames.length-1) { stop(); return; }
    next();
  }, parseInt(document.getElementById('speed').value));
}
function stop() { clearInterval(timer); timer=null; document.getElementById('playBtn').classList.remove('active'); }
document.getElementById('speed').oninput = (e) => { document.getElementById('speedLabel').textContent = (e.target.value/1000).toFixed(1)+'s'; };
show(0);
</script>
</body>
</html>`;
}

// ── Dashboard HTML (live mode) ───────────────────────────────────

const DASHBOARD_HTML = `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<title>KNIGHT RIDER</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css">
<style>
  * { box-sizing:border-box; margin:0; padding:0; }
  body { background:#0d1117; color:#c9d1d9; font-family:system-ui; height:100vh; display:flex; flex-direction:column; }
  #header { padding:10px 16px; background:#161b22; border-bottom:1px solid #30363d; display:flex; align-items:center; gap:16px; position:relative; overflow:hidden; }
  #header h1 { font-size:16px; white-space:nowrap; }
  #scanner { position:absolute; bottom:0; left:0; width:100%; height:3px; }
  #scanner::after { content:''; position:absolute; width:80px; height:100%; background:linear-gradient(90deg,transparent,#ff1a1a,transparent); animation:scan 2s ease-in-out 3 forwards; }
  @keyframes scan { 0%{left:-80px} 50%{left:calc(100%)} 100%{left:-80px} }
  .tab { padding:4px 12px; border-radius:4px; cursor:pointer; font-size:13px; border:1px solid #30363d; background:transparent; color:#c9d1d9; }
  .tab.active { background:#1f6feb; border-color:#1f6feb; color:#fff; }
  #status { font-size:12px; padding:2px 8px; border-radius:4px; margin-left:auto; }
  #status.ok { background:#0d2818; color:#3fb950; }
  #status.err { background:#2d1117; color:#f85149; }
  #status.boot { background:#2d2200; color:#d29922; }
  .panel { flex:1; display:none; overflow:hidden; }
  .panel.active { display:flex; flex-direction:column; }
  #live-terminal { flex:1; padding:8px; }
  #frames-panel { flex-direction:row !important; }
  #frame-list { width:260px; border-right:1px solid #30363d; overflow-y:auto; padding:8px; }
  .fi { padding:8px 10px; border-radius:4px; cursor:pointer; font-size:13px; margin-bottom:4px; display:flex; justify-content:space-between; }
  .fi:hover { background:#161b22; }
  .fi.active { background:#1f6feb33; border:1px solid #1f6feb; }
  #frame-view { flex:1; display:flex; flex-direction:column; }
  #ftoolbar { padding:8px 12px; background:#161b22; border-bottom:1px solid #30363d; display:flex; align-items:center; gap:8px; }
  #ftoolbar button { padding:4px 10px; border-radius:4px; border:1px solid #30363d; background:#21262d; color:#c9d1d9; cursor:pointer; font-size:13px; }
  #ftoolbar button:hover { background:#30363d; }
  #ftoolbar button.on { background:#1f6feb; border-color:#1f6feb; }
  #fcontent { flex:1; overflow:auto; padding:12px; }
  #noframes { padding:40px; text-align:center; color:#8b949e; }
</style>
</head>
<body>
<div id="header">
  <h1><span style="color:#ff1a1a;font-weight:800;letter-spacing:2px">KNIGHT RIDER</span></h1>
  <button class="tab active" onclick="showTab('live',this)">Live</button>
  <button class="tab" onclick="showTab('frames',this)">Frames <span id="fc">(0)</span></button>
  <span id="status" class="boot">booting...</span>
  <div id="scanner"></div>
</div>
<div id="live-panel" class="panel active"><div id="live-terminal"></div></div>
<div id="frames-panel" class="panel">
  <div id="frame-list"><div id="noframes">No frames yet.</div></div>
  <div id="frame-view">
    <div id="ftoolbar">
      <button onclick="playF()" id="pbtn">▶ Play</button>
      <button onclick="stopF()">⏹</button>
      <input type="range" id="spd" min="200" max="3000" value="1000" style="width:80px">
      <span id="sl">1.0s</span>
      <span id="fi2"></span>
    </div>
    <div id="fcontent"></div>
  </div>
</div>
<script src="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.js"></script>
<script src="https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.js"></script>
<script>
const term=new Terminal({scrollback:10000,cursorBlink:true,theme:{background:'#0d1117'}});
const fit=new FitAddon.FitAddon();term.loadAddon(fit);
term.open(document.getElementById('live-terminal'));fit.fit();
window.addEventListener('resize',()=>fit.fit());
const ws=new WebSocket('ws://'+location.host+'/ws');
const st=document.getElementById('status');
ws.onopen=()=>{st.textContent='connected';st.className='ok'};
ws.onclose=()=>{st.textContent='disconnected';st.className='err'};
ws.onmessage=(e)=>term.write(e.data);

function showTab(n,btn){
  document.querySelectorAll('.panel').forEach(p=>p.classList.remove('active'));
  document.querySelectorAll('.tab').forEach(t=>t.classList.remove('active'));
  document.getElementById(n+'-panel').classList.add('active');
  btn.classList.add('active');
  if(n==='frames')refreshF();
  if(n==='live')fit.fit();
}

let af=[],cf=-1,pt=null;
async function refreshF(){
  const r=await fetch('/api/frames');af=(await r.json()).frames;
  document.getElementById('fc').textContent='('+af.length+')';
  const el=document.getElementById('frame-list');
  if(!af.length){el.innerHTML='<div id="noframes">No frames yet.</div>';return}
  el.innerHTML=af.map((f,i)=>'<div class="fi'+(i===cf?' active':'')+'" onclick="showF('+i+')"><span>#'+(i+1)+' '+f.label+'</span><span style="color:#8b949e;font-size:11px">'+new Date(f.timestamp).toLocaleTimeString()+'</span></div>').join('');
}
function showF(i){cf=i;document.getElementById('fcontent').innerHTML=af[i].html;document.getElementById('fi2').textContent='#'+(i+1)+'/'+af.length;document.querySelectorAll('.fi').forEach((e,j)=>e.classList.toggle('active',j===i))}
function playF(){stopF();document.getElementById('pbtn').classList.add('on');if(cf<0||cf>=af.length-1)showF(0);else showF(cf+1);pt=setInterval(()=>{if(cf>=af.length-1){stopF();return}showF(cf+1)},parseInt(document.getElementById('spd').value))}
function stopF(){clearInterval(pt);pt=null;document.getElementById('pbtn').classList.remove('on')}
document.getElementById('spd').oninput=(e)=>{document.getElementById('sl').textContent=(e.target.value/1000).toFixed(1)+'s'};
setInterval(()=>{if(document.getElementById('frames-panel').classList.contains('active')&&!pt)refreshF()},3000);
</script>
</body>
</html>`;

// ── Helpers ──────────────────────────────────────────────────────

async function readBody(req: Request): Promise<any> {
  try { return await req.json(); } catch { return {}; }
}
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}
function requirePty(): PtyManager {
  if (!pty) throw new Error(bootError ?? 'still booting');
  return pty;
}

// ── HTTP + WebSocket server (starts IMMEDIATELY) ─────────────────

const server = Bun.serve({
  port: PORT,
  async fetch(req, server) {
    const url = new URL(req.url);

    if (url.pathname === '/ws') {
      if (server.upgrade(req)) return undefined as any;
      return new Response('WebSocket upgrade failed', { status: 400 });
    }
    if (url.pathname === '/') return new Response(DASHBOARD_HTML, { headers: { 'content-type': 'text/html' } });
    if (url.pathname === '/report') return new Response(generateReportHtml(), { headers: { 'content-type': 'text/html' } });

    const json = (data: any, status = 200) =>
      new Response(JSON.stringify(data, null, 2), { status, headers: { 'content-type': 'application/json' } });

    if (url.pathname === '/api/status') return json({ ready, error: bootError, frameCount: frames.length, outputDir: OUTPUT_DIR });
    if (url.pathname === '/api/frames') return json({ frames });

    try {
      const p = requirePty();
      switch (url.pathname) {
        case '/api/screen': return json({ lines: p.getSnapshot() });
        case '/api/screen/html': return new Response(p.getSnapshotHtml(), { headers: { 'content-type': 'text/html' } });
        case '/api/keys': { const { keys } = await readBody(req); if (!keys) return json({ error: 'missing "keys"' }, 400); await p.sendKeys(keys); await sleep(100); return json({ ok: true }); }
        case '/api/enter': await p.sendKeys('\r'); await sleep(100); return json({ ok: true });
        case '/api/escape': await p.sendKeys(String.fromCharCode(0x1b)); await sleep(100); return json({ ok: true });
        case '/api/up': await p.sendKeys('\x1b[A'); await sleep(100); return json({ ok: true });
        case '/api/down': await p.sendKeys('\x1b[B'); await sleep(100); return json({ ok: true });
        case '/api/ctrlc': await p.sendKeys(String.fromCharCode(0x03)); await sleep(100); return json({ ok: true });
        case '/api/wait-for-text': { const { text, timeout } = await readBody(req); if (!text) return json({ error: 'missing "text"' }, 400); await p.waitForVisibleText(text, timeout ?? 10000); return json({ ok: true }); }
        case '/api/sleep': { const { ms } = await readBody(req); await sleep(ms ?? 1000); return json({ ok: true }); }
        case '/api/frame': { const { label } = await readBody(req); if (!label) return json({ error: 'missing "label"' }, 400); const f = captureFrame(label); return json({ ok: true, index: frames.length - 1, label: f.label, file: OUTPUT_DIR }); }
        default: return json({ error: 'not found' }, 404);
      }
    } catch (e: any) {
      return json({ error: e.message }, e.message.includes('booting') ? 503 : 500);
    }
  },
  websocket: {
    open(ws) { wsClients.add(ws); },
    close(ws) { wsClients.delete(ws); },
    message() {},
  },
});

console.log(`🏎️  Knight Rider — http://localhost:${PORT}`);
console.log(`   Mode:    ${CMD ? 'custom' : USE_SYSTEM ? 'system kiro-cli' : USE_V1 ? 'v1' : 'local dev (source)'}`);
console.log(`   Command: ${resolvedCmd}`);
if (Object.keys(extraEnv).length) console.log(`   Env:     ${Object.entries(extraEnv).map(([k, v]) => `${k}=${v}`).join(', ')}`);
console.log(`   Frames:  ${OUTPUT_DIR}`);

// ── Boot PTY ─────────────────────────────────────────────────────

(async () => {
  try {
    const parts = resolvedCmd.split(/\s+/);
    const p = new PtyManager({ width: WIDTH, height: HEIGHT, cwd: process.cwd(), env: extraEnv });
    p.spawn(parts[0]!, parts.slice(1));
    p.onData((data) => {
      for (const ws of wsClients) {
        try { ws.send(data); } catch { wsClients.delete(ws); }
      }
    });
    pty = p;
    ready = true;
    console.log(`✅ Ready`);
  } catch (e: any) {
    bootError = e.message;
    console.error(`❌ Boot failed: ${e.message}`);
  }
})();

process.on('SIGINT', () => {
  console.log(`\n🛑 Stopped. Evidence: ${OUTPUT_DIR}/index.html`);
  server.stop();
  pty?.kill();
  process.exit(0);
});
