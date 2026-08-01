import { spawn } from 'node:child_process';
import { useEffect, useState } from 'react';

function terminalSize(): { columns: number; rows: number } {
  return {
    columns: process.stdout.columns || 120,
    rows: process.stdout.rows || 34,
  };
}

export function useTerminalSize(): { columns: number; rows: number } {
  const [size, setSize] = useState(terminalSize);
  useEffect(() => {
    const resize = () => setSize(terminalSize());
    process.stdout.on('resize', resize);
    return () => {
      process.stdout.off('resize', resize);
    };
  }, []);
  return size;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function copyToClipboard(text: string): void {
  if (process.platform !== 'darwin') return;
  const child = spawn('/usr/bin/pbcopy', {
    stdio: ['pipe', 'ignore', 'ignore'],
  });
  child.on('error', () => {});
  child.stdin?.end(text);
}

export function inBounds(
  event: { x: number; y: number },
  bounds?: { x: number; y: number; width: number; height: number }
): boolean {
  return Boolean(
    bounds &&
    event.x >= bounds.x &&
    event.x < bounds.x + bounds.width &&
    event.y >= bounds.y &&
    event.y < bounds.y + bounds.height
  );
}
