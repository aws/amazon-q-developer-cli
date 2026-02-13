#!/usr/bin/env bun
/**
 * Analyze CPU profile to find top offenders with time-series trends.
 * Usage: bun run scripts/analyze-profile.ts [profile-path] [--json] [--html]
 */

import { readdirSync, readFileSync, writeFileSync } from 'fs';
import path from 'path';

const SPARKLINE = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const outputJson = process.argv.includes('--json');
const outputHtml = process.argv.includes('--html');

interface CallFrame {
  functionName: string;
  url: string;
  lineNumber: number;
}

interface ProfileNode {
  id: number;
  callFrame: CallFrame;
  hitCount?: number;
  children?: number[];
}

interface Profile {
  nodes: ProfileNode[];
  samples: number[];
  timeDeltas: number[];
  startTime: number;
  endTime: number;
}

function getLatestProfile(dir: string): string | null {
  const files = readdirSync(dir).filter(f => f.endsWith('.cpuprofile'));
  if (files.length === 0) return null;
  files.sort((a, b) => b.localeCompare(a));
  return path.join(dir, files[0]!);
}

function classifyFunction(url: string): 'our-code' | 'dependency' | 'runtime' {
  if (!url || url === 'unknown') return 'runtime';
  if (url.includes('/packages/tui/src/')) return 'our-code';
  if (url.includes('node_modules')) return 'dependency';
  return 'runtime';
}

function sparkline(values: number[]): string {
  if (values.length === 0) return '';
  const max = Math.max(...values, 1);
  return values.map(v => SPARKLINE[Math.min(Math.floor((v / max) * 7), 7)]).join('');
}

function formatMs(us: number): string {
  return (us / 1000).toFixed(1) + 'ms';
}

function analyze(profilePath: string) {
  const profile: Profile = JSON.parse(readFileSync(profilePath, 'utf-8'));
  const nodes = new Map(profile.nodes.map(n => [n.id, n]));
  
  // Calculate sample interval (average time per sample)
  const totalTime = profile.endTime - profile.startTime; // microseconds
  const sampleInterval = totalTime / profile.samples.length;
  
  // Initialize report data
  reportData = {
    timestamp: new Date().toISOString(),
    durationS: totalTime / 1000000,
    samples: profile.samples.length,
    functions: [],
  };
  
  // Build child->parent map for total time calculation
  const nodeParent = new Map<number, number>();
  for (const node of profile.nodes) {
    for (const childId of node.children || []) {
      nodeParent.set(childId, node.id);
    }
  }
  
  // Count samples for each node (self time) and propagate up (total time)
  const nodeSelfHits = new Map<number, number>();
  const nodeTotalHits = new Map<number, number>();
  
  for (const sampleId of profile.samples) {
    nodeSelfHits.set(sampleId, (nodeSelfHits.get(sampleId) || 0) + 1);
    
    // Walk up the tree to attribute total time
    let current: number | undefined = sampleId;
    while (current !== undefined) {
      nodeTotalHits.set(current, (nodeTotalHits.get(current) || 0) + 1);
      current = nodeParent.get(current);
    }
  }
  
  // Build call tree for caller/callee relationships
  const callTree = new Map<string, { callers: Map<string, number>; callees: Map<string, number> }>();
  
  for (const sampleId of profile.samples) {
    let current: number | undefined = sampleId;
    let childKey: string | undefined;
    
    while (current !== undefined) {
      const node = nodes.get(current);
      if (!node) break;
      
      const fn = node.callFrame.functionName || '(anonymous)';
      const file = node.callFrame.url.split('/').pop() || 'unknown';
      const key = `${fn} @ ${file}`;
      
      if (!callTree.has(key)) {
        callTree.set(key, { callers: new Map(), callees: new Map() });
      }
      
      if (childKey) {
        callTree.get(key)!.callees.set(childKey, (callTree.get(key)!.callees.get(childKey) || 0) + 1);
        callTree.get(childKey)!.callers.set(key, (callTree.get(childKey)!.callers.get(key) || 0) + 1);
      }
      
      childKey = key;
      current = nodeParent.get(current);
    }
  }
  
  // Aggregate by function name
  const stats = new Map<string, { selfHits: number; totalHits: number; calls: number; url: string; type: string }>();
  for (const node of profile.nodes) {
    const fn = node.callFrame.functionName || '(anonymous)';
    const url = node.callFrame.url;
    const file = url.split('/').pop() || 'unknown';
    const key = `${fn} @ ${file}`;
    const existing = stats.get(key) || { selfHits: 0, totalHits: 0, calls: 0, url, type: classifyFunction(url) };
    existing.selfHits += nodeSelfHits.get(node.id) || 0;
    existing.totalHits += nodeTotalHits.get(node.id) || 0;
    // Each unique node ID represents calls to that function
    if (nodeSelfHits.has(node.id) || nodeTotalHits.has(node.id)) {
      existing.calls += nodeSelfHits.get(node.id) || 0;
    }
    stats.set(key, existing);
  }

  // Time-series: group samples into 10-second windows
  const windowMs = 10000000; // 10 seconds in microseconds
  const windows: Map<string, number>[] = [];
  let currentWindow: Map<string, number> = new Map();
  let windowStart = 0;
  let elapsed = 0;

  for (let i = 0; i < profile.samples.length; i++) {
    elapsed += profile.timeDeltas[i] || 0;
    if (elapsed - windowStart >= windowMs) {
      windows.push(new Map(currentWindow));
      currentWindow = new Map();
      windowStart = elapsed;
    }
    const node = nodes.get(profile.samples[i]!);
    if (node) {
      const fn = node.callFrame.functionName || '(anonymous)';
      const file = node.callFrame.url.split('/').pop() || 'unknown';
      const key = `${fn} @ ${file}`;
      currentWindow.set(key, (currentWindow.get(key) || 0) + 1);
    }
  }
  if (currentWindow.size > 0) windows.push(currentWindow);

  // Sort by self hits
  const sorted = [...stats.entries()]
    .sort((a, b) => b[1].selfHits - a[1].selfHits)
    .slice(0, 20);

  // Print results
  console.log(`\n📊 CPU Profile Analysis: ${path.basename(profilePath)}`);
  console.log(`   Duration: ${(totalTime / 1000000).toFixed(1)}s`);
  console.log(`   Samples: ${profile.samples.length}`);
  console.log(`   Sample interval: ${(sampleInterval / 1000).toFixed(2)}ms`);
  console.log(`   Windows: ${windows.length} (10s each)\n`);

  console.log('Top 20 functions by self time:\n');
  console.log('   Self Time  |  Total Time  |   Calls   | Type    | Trend        | Function');
  console.log('--------------|--------------|-----------|---------|--------------|' + '-'.repeat(35));

  for (const [fn, data] of sorted) {
    const selfTime = data.selfHits * sampleInterval;
    const totalTime = data.totalHits * sampleInterval;
    const trend = windows.map(w => w.get(fn) || 0);
    const typeLabel = data.type === 'our-code' ? '🟢 ours' : 
                      data.type === 'dependency' ? '🟡 dep' : '⚪ rt';
    const calls = data.calls > 1000000 ? `${(data.calls/1000000).toFixed(1)}M` :
                  data.calls > 1000 ? `${(data.calls/1000).toFixed(1)}K` : 
                  data.calls.toString();
    console.log(
      `${formatMs(selfTime).padStart(12)}  | ${formatMs(totalTime).padStart(11)}  | ${calls.padStart(9)} | ${typeLabel.padEnd(7)} | ${sparkline(trend).padEnd(12)} | ${fn.slice(0, 35)}`
    );
    
    const tree = callTree.get(fn);
    reportData.functions.push({
      name: fn,
      selfTimeMs: selfTime / 1000,
      totalTimeMs: totalTime / 1000,
      calls: data.calls,
      type: data.type,
      trend,
      callers: tree ? Object.fromEntries([...tree.callers.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5)) : {},
      callees: tree ? Object.fromEntries([...tree.callees.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5)) : {},
    });
  }

  // Per-window breakdown for top 3 functions
  const top3 = sorted.slice(0, 3);
  console.log('\n📈 10-second window breakdown (top 3):\n');
  console.log('Window |', top3.map(([fn]) => fn.slice(0, 20).padEnd(20)).join(' | '));
  console.log('-------|', top3.map(() => '-'.repeat(20)).join('-|-'));
  
  for (let i = 0; i < windows.length; i++) {
    const w = windows[i]!;
    const values = top3.map(([fn]) => {
      const hits = w.get(fn) || 0;
      const ms = (hits * sampleInterval / 1000).toFixed(0);
      return `${ms}ms`.padEnd(20);
    });
    console.log(`${((i + 1) * 10).toString().padStart(4)}s  |`, values.join(' | '));
  }

  // Summary
  const ourCode = sorted.filter(([_, d]) => d.type === 'our-code');
  const deps = sorted.filter(([_, d]) => d.type === 'dependency');
  
  console.log('\n📋 Summary:');
  console.log(`   Our code in top 20: ${ourCode.length}`);
  console.log(`   Dependencies in top 20: ${deps.length}`);
  
  if (deps.length > 0) {
    console.log('\n🔍 Hot dependencies:');
    for (const [fn, data] of deps.slice(0, 5)) {
      const selfTime = data.selfHits * sampleInterval;
      console.log(`   - ${fn} (${formatMs(selfTime)} self)`);
    }
  }
  
  // Save reports
  const reportsDir = path.join(path.dirname(profilePath), '..', 'reports');
  require('fs').mkdirSync(reportsDir, { recursive: true });
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  
  if (outputJson || outputHtml) {
    const jsonPath = path.join(reportsDir, `profile-${timestamp}.json`);
    writeFileSync(jsonPath, JSON.stringify(reportData, null, 2));
    console.log(`\n📄 JSON saved: ${jsonPath}`);
  }
  
  if (outputHtml) {
    const htmlPath = path.join(reportsDir, `profile-${timestamp}.html`);
    writeFileSync(htmlPath, generateHtml(reportData));
    console.log(`📄 HTML saved: ${htmlPath}`);
  }
}

function generateHtml(data: typeof reportData): string {
  const top5 = data.functions.slice(0, 5);
  
  return `<!DOCTYPE html>
<html><head>
<title>Profile Report - ${data.timestamp}</title>
<style>
  body { font-family: system-ui; max-width: 1200px; margin: 0 auto; padding: 20px; background: #1a1a2e; color: #eee; }
  h1, h2 { color: #00d9ff; }
  table { width: 100%; border-collapse: collapse; margin: 20px 0; }
  th, td { padding: 8px 12px; text-align: left; border-bottom: 1px solid #333; }
  th { background: #16213e; color: #00d9ff; }
  .row { cursor: pointer; }
  .row:hover { background: #1f3460; }
  .ours { color: #4ade80; }
  .dep { color: #fbbf24; }
  .rt { color: #94a3b8; }
  .trend { font-family: monospace; letter-spacing: -2px; }
  .meta { color: #888; margin-bottom: 20px; }
  .details { display: none; background: #0f0f23; }
  .details.open { display: table-row; }
  .details td { padding: 15px 20px; }
  .call-list { margin: 5px 0; padding-left: 20px; }
  .call-list li { margin: 3px 0; color: #888; }
  .call-list .fn { color: #00d9ff; }
  .hot-path { background: #1f1f3a; padding: 15px; border-radius: 8px; margin: 10px 0; }
  .hot-path h3 { margin: 0 0 10px 0; color: #ff6b6b; }
  .path-item { padding: 5px 0; border-left: 2px solid #ff6b6b; padding-left: 10px; margin-left: 10px; }
</style>
</head><body>
<h1>🔥 CPU Profile Report</h1>
<div class="meta">
  <p>Duration: ${data.durationS.toFixed(1)}s | Samples: ${data.samples} | Generated: ${data.timestamp}</p>
</div>

<h2>🔥 Top 5 Hot Paths</h2>
${top5.map((f, i) => `
<div class="hot-path">
  <h3>#${i + 1}: ${escapeHtml(f.name.split(' @ ')[0] ?? f.name)} (${f.selfTimeMs.toFixed(0)}ms self)</h3>
  <div><strong>Called by:</strong></div>
  ${Object.entries((f as any).callers || {}).slice(0, 3).map(([caller, count]) => 
    `<div class="path-item">← ${escapeHtml(caller.split(' @ ')[0] ?? caller)} (${count}x)</div>`
  ).join('') || '<div class="path-item">← (root)</div>'}
  <div style="margin-top:10px"><strong>Calls:</strong></div>
  ${Object.entries((f as any).callees || {}).slice(0, 3).map(([callee, count]) => 
    `<div class="path-item">→ ${escapeHtml(callee.split(' @ ')[0] ?? callee)} (${count}x)</div>`
  ).join('') || '<div class="path-item">→ (leaf)</div>'}
</div>
`).join('')}

<h2>Top Functions by Self Time</h2>
<p style="color:#888">Click a row to see callers and callees</p>
<table>
  <tr><th>Function</th><th>Self Time</th><th>Total Time</th><th>Calls</th><th>Type</th><th>Trend</th></tr>
  ${data.functions.map((f, i) => `
  <tr class="row" onclick="toggle(${i})">
    <td>${escapeHtml(f.name)}</td>
    <td>${f.selfTimeMs.toFixed(1)}ms</td>
    <td>${f.totalTimeMs.toFixed(1)}ms</td>
    <td>${f.calls.toLocaleString()}</td>
    <td class="${f.type === 'our-code' ? 'ours' : f.type === 'dependency' ? 'dep' : 'rt'}">${f.type}</td>
    <td class="trend">${sparkline(f.trend)}</td>
  </tr>
  <tr class="details" id="details-${i}">
    <td colspan="6">
      <strong>Called by:</strong>
      <ul class="call-list">
        ${Object.entries((f as any).callers || {}).map(([caller, count]) => 
          `<li><span class="fn">${escapeHtml(caller)}</span> (${count}x)</li>`
        ).join('') || '<li>(root)</li>'}
      </ul>
      <strong>Calls:</strong>
      <ul class="call-list">
        ${Object.entries((f as any).callees || {}).map(([callee, count]) => 
          `<li><span class="fn">${escapeHtml(callee)}</span> (${count}x)</li>`
        ).join('') || '<li>(leaf)</li>'}
      </ul>
    </td>
  </tr>`).join('')}
</table>
<script>
function toggle(i) {
  document.getElementById('details-' + i).classList.toggle('open');
}
</script>
</body></html>`;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

let reportData: {
  timestamp: string;
  durationS: number;
  samples: number;
  functions: { name: string; selfTimeMs: number; totalTimeMs: number; calls: number; type: string; trend: number[]; callers: Record<string, number>; callees: Record<string, number> }[];
} = { timestamp: '', durationS: 0, samples: 0, functions: [] };

// Main
const profileDir = path.join(__dirname, '..', 'profiles');
const args = process.argv.slice(2).filter(a => !a.startsWith('--'));
const profilePath = args[0] || getLatestProfile(profileDir);

if (!profilePath) {
  console.error('No profile found. Run: bun run dev:profile');
  process.exit(1);
}

analyze(profilePath);
