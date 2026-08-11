import { closeSync, fstatSync, openSync, readSync, writeSync } from 'node:fs';

export const SESSION_METADATA_MAX_BYTES = 1024 * 1024;
export const DASHBOARD_SIDECAR_MAX_BYTES = 8 * 1024 * 1024;
export const LOCK_METADATA_MAX_BYTES = 64 * 1024;

export class OversizedJsonFileError extends Error {
  constructor(path: string, maxBytes: number) {
    super(`JSON file exceeds ${maxBytes} byte limit: ${path}`);
    this.name = 'OversizedJsonFileError';
  }
}

export interface BoundedFileTail {
  bytes: Buffer;
  complete: boolean;
}

type DescriptorReader = (
  fd: number,
  buffer: Buffer,
  offset: number,
  length: number,
  position: number | null
) => number;

type DescriptorWriter = (
  fd: number,
  buffer: Buffer,
  offset: number,
  length: number,
  position: number | null
) => number;

const defaultReader: DescriptorReader = (
  fd,
  buffer,
  offset,
  length,
  position
) => readSync(fd, buffer, offset, length, position);

/** Read a fixed descriptor range completely, returning only bytes obtained. */
export function readFileRange(
  fd: number,
  position: number,
  length: number,
  reader: DescriptorReader = defaultReader
): Buffer {
  const buffer = Buffer.allocUnsafe(length);
  let total = 0;
  while (total < length) {
    const read = reader(fd, buffer, total, length - total, position + total);
    if (read === 0) break;
    total += read;
  }
  return buffer.subarray(0, total);
}

/** Write all bytes to a descriptor or fail if the writer stops progressing. */
export function writeFileFully(
  fd: number,
  bytes: Buffer,
  writer: DescriptorWriter = (fd, buffer, offset, length, position) =>
    writeSync(fd, buffer, offset, length, position)
): void {
  let total = 0;
  while (total < bytes.length) {
    const written = writer(fd, bytes, total, bytes.length - total, null);
    if (written === 0) {
      throw new Error('Unable to complete descriptor write');
    }
    total += written;
  }
}

/** Read at most the last `maxBytes` from one descriptor snapshot. */
export function readBoundedFileTail(
  path: string,
  maxBytes: number
): BoundedFileTail {
  const fd = openSync(path, 'r');
  try {
    const snapshotSize = fstatSync(fd).size;
    const start = Math.max(0, snapshotSize - maxBytes);
    return {
      bytes: readFileRange(fd, start, snapshotSize - start),
      complete: start === 0,
    };
  } finally {
    closeSync(fd);
  }
}

/** Reject JSON that exceeds its byte cap, including files that grow mid-read. */
export function readBoundedJson(path: string, maxBytes: number): unknown {
  const fd = openSync(path, 'r');
  try {
    if (fstatSync(fd).size > maxBytes) {
      throw new OversizedJsonFileError(path, maxBytes);
    }
    const chunks: Buffer[] = [];
    let total = 0;
    for (;;) {
      const remaining = maxBytes + 1 - total;
      if (remaining <= 0) throw new OversizedJsonFileError(path, maxBytes);
      const chunk = Buffer.allocUnsafe(Math.min(64 * 1024, remaining));
      const read = readSync(fd, chunk, 0, chunk.length, null);
      if (read === 0) break;
      chunks.push(chunk.subarray(0, read));
      total += read;
      if (total > maxBytes) throw new OversizedJsonFileError(path, maxBytes);
    }
    return JSON.parse(Buffer.concat(chunks, total).toString('utf-8'));
  } finally {
    closeSync(fd);
  }
}
