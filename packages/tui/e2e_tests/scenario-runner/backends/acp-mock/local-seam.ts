import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import type { KasAcpMockScenarioConfig } from './config';

interface ScenarioMockLocalConfig {
  save?: {
    error?: string;
  };
  load?: {
    error?: string;
    sessionId?: string;
    importedPath?: string;
  };
}

export interface LocalSeam {
  binPath: string;
  cleanup: () => void;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function writeLocalSeamBinary(
  localConfig: ScenarioMockLocalConfig
): LocalSeam {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-scenario-local-'));
  const isWindows = process.platform === 'win32';
  const binPath = path.join(dir, isWindows ? 'chat_cli.cmd' : 'chat_cli');
  const loadSessionId = localConfig.load?.sessionId ?? 'imported-session';
  const importPath =
    localConfig.load?.importedPath ?? path.join(dir, 'sessions', loadSessionId);

  if (isWindows) {
    const powershellPath = path.join(dir, 'chat_cli.ps1');
    const script =
      `@echo off\r\n` +
      `powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0chat_cli.ps1" %*\r\n` +
      `exit /b %ERRORLEVEL%\r\n`;
    fs.writeFileSync(binPath, script, 'utf-8');
    const powershellScript =
      `param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsList)\r\n` +
      `$sub = ''\r\n` +
      `$out = ''\r\n` +
      `$archive = ''\r\n` +
      `$ssid = ''\r\n` +
      `for ($i = 0; $i -lt $ArgsList.Length; $i++) {\r\n` +
      `  switch ($ArgsList[$i]) {\r\n` +
      `    'export-session' { $sub = 'export' }\r\n` +
      `    'import-session' { $sub = 'import' }\r\n` +
      `    'ensure-session' { $sub = 'ensure' }\r\n` +
      `    '--out' { if ($i + 1 -lt $ArgsList.Length) { $out = $ArgsList[$i + 1]; $i++ } }\r\n` +
      `    '--archive' { if ($i + 1 -lt $ArgsList.Length) { $archive = $ArgsList[$i + 1]; $i++ } }\r\n` +
      `    '--source-session-id' { if ($i + 1 -lt $ArgsList.Length) { $ssid = $ArgsList[$i + 1]; $i++ } }\r\n` +
      `  }\r\n` +
      `}\r\n` +
      `function Emit-Json([object]$Value) {\r\n` +
      `  $Value | ConvertTo-Json -Compress\r\n` +
      `}\r\n` +
      `switch ($sub) {\r\n` +
      `  'export' {\r\n` +
      (localConfig.save?.error
        ? `    Emit-Json @{ kind = 'error'; data = @{ message = ${JSON.stringify(localConfig.save.error)} } }\r\n`
        : `    if ($out -ne '') {\r\n` +
          `      $parent = Split-Path -Parent $out\r\n` +
          `      if ($parent) { [System.IO.Directory]::CreateDirectory($parent) | Out-Null }\r\n` +
          `      [System.IO.File]::WriteAllText($out, '')\r\n` +
          `    }\r\n` +
          `    Emit-Json @{ kind = 'exportSession'; data = @{ path = $out } }\r\n`) +
      `    break\r\n` +
      `  }\r\n` +
      `  'import' {\r\n` +
      (localConfig.load?.error
        ? `    Emit-Json @{ kind = 'error'; data = @{ message = ${JSON.stringify(localConfig.load.error)} } }\r\n`
        : `    Emit-Json @{ kind = 'importSession'; data = @{ path = ${JSON.stringify(importPath)} } }\r\n`) +
      `    break\r\n` +
      `  }\r\n` +
      `  'ensure' {\r\n` +
      `    if ($ssid -eq '') { $ssid = ${JSON.stringify(loadSessionId)} }\r\n` +
      `    Emit-Json @{ kind = 'ensureSession'; data = @{ sessionId = $ssid } }\r\n` +
      `    break\r\n` +
      `  }\r\n` +
      `  default {\r\n` +
      `    Emit-Json @{ kind = 'error'; data = @{ message = 'Unsupported local seam command' } }\r\n` +
      `    break\r\n` +
      `  }\r\n` +
      `}\r\n`;
    fs.writeFileSync(powershellPath, powershellScript, 'utf-8');
  } else {
    const script =
      `#!/usr/bin/env bash\n` +
      `sub=""\n` +
      `out=""\n` +
      `archive=""\n` +
      `ssid=""\n` +
      `while [[ $# -gt 0 ]]; do\n` +
      `  case "$1" in\n` +
      `    export-session) sub="export" ;;\n` +
      `    import-session) sub="import" ;;\n` +
      `    ensure-session) sub="ensure" ;;\n` +
      `    --out) out="$2"; shift ;;\n` +
      `    --archive) archive="$2"; shift ;;\n` +
      `    --source-session-id) ssid="$2"; shift ;;\n` +
      `  esac\n` +
      `  shift\n` +
      `done\n` +
      `case "$sub" in\n` +
      `  export)\n` +
      (localConfig.save?.error
        ? `    cat <<'__KIRO_ERROR__'\n` +
          `{"kind":"error","data":{"message":${JSON.stringify(localConfig.save.error)}}}\n` +
          `__KIRO_ERROR__\n`
        : `    mkdir -p "$(dirname "$out")"\n` +
          `    : > "$out"\n` +
          `    printf '{"kind":"exportSession","data":{"path":"%s"}}\\n' "$out"\n`) +
      `    ;;\n` +
      `  import)\n` +
      (localConfig.load?.error
        ? `    cat <<'__KIRO_ERROR__'\n` +
          `{"kind":"error","data":{"message":${JSON.stringify(localConfig.load.error)}}}\n` +
          `__KIRO_ERROR__\n`
        : `    cat <<'__KIRO_IMPORT__'\n` +
          `${JSON.stringify({
            kind: 'importSession',
            data: { path: importPath },
          })}\n` +
          `__KIRO_IMPORT__\n`) +
      `    ;;\n` +
      `  ensure)\n` +
      `    if [[ -z "$ssid" ]]; then ssid=${shellQuote(loadSessionId)}; fi\n` +
      `    printf '{"kind":"ensureSession","data":{"sessionId":"%s"}}\\n' "$ssid"\n` +
      `    ;;\n` +
      `  *)\n` +
      `    cat <<'__KIRO_UNSUPPORTED__'\n` +
      `{"kind":"error","data":{"message":"Unsupported local seam command"}}\n` +
      `__KIRO_UNSUPPORTED__\n` +
      `    ;;\n` +
      `esac\n`;
    fs.writeFileSync(binPath, script, 'utf-8');
    fs.chmodSync(binPath, 0o755);
  }

  return {
    binPath,
    cleanup: () => fs.rmSync(dir, { recursive: true, force: true }),
  };
}

export function installLocalSeam(
  testCaseOptions: { extraEnv?: Record<string, string> },
  profileConfig: KasAcpMockScenarioConfig | undefined
): LocalSeam | null {
  const localConfig = asRecord(profileConfig?.local) as
    | ScenarioMockLocalConfig
    | null;
  if (!localConfig) return null;

  const seam = writeLocalSeamBinary(localConfig);
  testCaseOptions.extraEnv = {
    ...testCaseOptions.extraEnv,
    KIRO_CHAT_CLI_BIN: seam.binPath,
  };
  return seam;
}
