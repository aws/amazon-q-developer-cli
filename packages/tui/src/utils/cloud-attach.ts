import { readFile, readdir, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import { basename, dirname, extname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { readPngDimensions } from './clipboard-image.js';
import { ImageFormat } from '../types/generated/agent.js';

const IMAGE_EXT_ALIASES: Record<string, ImageFormat> = {
  jpg: ImageFormat.Jpeg,
};
const IMAGE_MIME_BY_EXT: Record<string, string> = Object.fromEntries([
  ...Object.values(ImageFormat).map((format) => [
    `.${format}`,
    `image/${format}`,
  ]),
  ...Object.entries(IMAGE_EXT_ALIASES).map(([alias, format]) => [
    `.${alias}`,
    `image/${format}`,
  ]),
]);

const DOCUMENT_MIME_BY_EXT: Record<string, string> = {
  '.pdf': 'application/pdf',
  '.csv': 'text/csv',
  '.doc': 'application/msword',
  '.docx':
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  '.xls': 'application/vnd.ms-excel',
  '.xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  '.html': 'text/html',
  '.md': 'text/markdown',
};

export interface CloudImageAttachment {
  base64: string;
  mimeType: string;
  width: number;
  height: number;
  sizeBytes: number;
  path: string;
}

export interface CloudResourceAttachment {
  uri: string;
  text: string;
  mimeType: string;
  path: string;
}

export interface CloudBlobAttachment {
  uri: string;
  blob: string;
  mimeType: string;
  path: string;
}

export interface CloudAttachments {
  images: CloudImageAttachment[];
  resources: CloudResourceAttachment[];
  blobs: CloudBlobAttachment[];
}

interface ResolvedFile {
  path: string;
}

const expandHome = (path: string): string => {
  if (path === '~') return homedir();
  if (path.startsWith('~/') || path.startsWith('~\\')) {
    return join(homedir(), path.slice(2));
  }
  return path;
};

const normalizeWhitespace = (value: string): string =>
  value.replace(/\s/g, ' ');
const removeWhitespace = (value: string): string => value.replace(/\s/g, '');

const unicodeWhitespaceCandidates = (name: string): string[] => {
  const NNBSP = '\u202f';
  const candidates = new Set<string>();
  candidates.add(name.replace(/ ([AP]M)(?=\.|$)/, `${NNBSP}$1`));
  candidates.add(name.replace(/(\d)([AP]M)(?=\.|$)/, `$1${NNBSP}$2`));
  candidates.delete(name);
  return [...candidates];
};

function createPathResolver() {
  const fileCache = new Map<string, Promise<boolean>>();
  const directoryCache = new Map<string, Promise<string[] | null>>();
  const resolutionCache = new Map<string, Promise<ResolvedFile | null>>();

  const isFile = (path: string): Promise<boolean> => {
    let result = fileCache.get(path);
    if (!result) {
      result = stat(path)
        .then((entry) => entry.isFile())
        .catch(() => false);
      fileCache.set(path, result);
    }
    return result;
  };

  const readDirectory = (path: string): Promise<string[] | null> => {
    let result = directoryCache.get(path);
    if (!result) {
      result = readdir(path).catch(() => null);
      directoryCache.set(path, result);
    }
    return result;
  };

  const resolveUncached = async (
    path: string
  ): Promise<ResolvedFile | null> => {
    if (await isFile(path)) return { path };

    const directory = dirname(path);
    const name = basename(path);
    if (directory === path || !name) return null;

    // Exact probes still work when macOS privacy controls deny directory listing.
    for (const candidate of unicodeWhitespaceCandidates(name)) {
      const candidatePath = join(directory, candidate);
      if (await isFile(candidatePath)) return { path: candidatePath };
    }

    const entries = await readDirectory(directory);
    if (!entries) return null;

    for (const key of [normalizeWhitespace, removeWhitespace]) {
      const wanted = key(name);
      let match: string | null = null;
      for (const entry of entries) {
        if (key(entry) !== wanted) continue;
        if (match !== null) return null;
        match = entry;
      }
      if (match !== null) {
        const resolvedPath = join(directory, match);
        if (await isFile(resolvedPath)) return { path: resolvedPath };
      }
    }
    return null;
  };

  return (path: string): Promise<ResolvedFile | null> => {
    let result = resolutionCache.get(path);
    if (!result) {
      result = resolveUncached(path);
      resolutionCache.set(path, result);
    }
    return result;
  };
}

const MAX_UNQUOTED_PATH_TOKENS = 32;

/** Find local file paths in prompt text without blocking the TUI event loop. */
export async function findLocalPaths(
  text: string,
  signal?: AbortSignal
): Promise<string[]> {
  // Expanded @file content is already in the prompt and must not become a new attachment source.
  text = text.replace(
    /<attached_file path="[^"]*">[\s\S]*?<\/attached_file>/g,
    ''
  );

  const resolvePath = createPathResolver();
  const found: string[] = [];
  const seen = new Set<string>();
  const add = (resolved: ResolvedFile | null) => {
    if (resolved && !seen.has(resolved.path)) {
      seen.add(resolved.path);
      found.push(resolved.path);
    }
  };

  const quotedPath = /["']((?:[A-Za-z]:[\\/]|~[\\/]|\/)[^"'\n]+)["']/g;
  for (
    let match = quotedPath.exec(text);
    match;
    match = quotedPath.exec(text)
  ) {
    if (signal?.aborted) return found;
    add(await resolvePath(expandHome(match[1]!)));
  }

  for (const line of text.split('\n')) {
    const pathStart = /(?:^|[\s"'([{<])((?:[A-Za-z]:[\\/]|~[\\/]|\/))/g;
    for (
      let match = pathStart.exec(line);
      match;
      match = pathStart.exec(line)
    ) {
      if (signal?.aborted) return found;
      const start = match.index + match[0].length - match[1]!.length;
      const tokens = line.slice(start).split(' ');
      for (
        let count = Math.min(tokens.length, MAX_UNQUOTED_PATH_TOKENS);
        count >= 1;
        count--
      ) {
        let candidate = tokens.slice(0, count).join(' ');
        candidate = candidate.replace(/[.,;:!?)\]}>"']+$/, '');
        if (!candidate || candidate === '/' || candidate === '~') continue;

        const resolved = await resolvePath(expandHome(candidate));
        if (resolved) {
          add(resolved);
          break;
        }
      }
    }
  }
  return found;
}

const utf8Strict = new TextDecoder('utf-8', { fatal: true });
const decodeText = (buffer: Buffer): string | null => {
  // NUL is valid UTF-8 but strongly indicates binary data.
  if (buffer.includes(0)) return null;
  try {
    return utf8Strict.decode(buffer);
  } catch {
    return null;
  }
};

/** Collect in-band attachments for local paths referenced in a cloud prompt. */
export async function collectCloudAttachments(
  text: string,
  signal?: AbortSignal
): Promise<CloudAttachments> {
  const images: CloudImageAttachment[] = [];
  const resources: CloudResourceAttachment[] = [];
  const blobs: CloudBlobAttachment[] = [];

  for (const path of await findLocalPaths(text, signal)) {
    if (signal?.aborted) break;

    let buffer: Buffer;
    try {
      buffer = signal ? await readFile(path, { signal }) : await readFile(path);
    } catch {
      if (signal?.aborted) break;
      continue;
    }

    const extension = extname(path).toLowerCase();
    const imageMime = IMAGE_MIME_BY_EXT[extension];
    if (imageMime) {
      const dimensions =
        imageMime === 'image/png' ? readPngDimensions(buffer) : null;
      images.push({
        base64: buffer.toString('base64'),
        mimeType: imageMime,
        width: dimensions?.width ?? 0,
        height: dimensions?.height ?? 0,
        sizeBytes: buffer.byteLength,
        path,
      });
      continue;
    }

    const uri = pathToFileURL(path).href;
    const documentMime = DOCUMENT_MIME_BY_EXT[extension];
    if (documentMime) {
      blobs.push({
        uri,
        blob: buffer.toString('base64'),
        mimeType: documentMime,
        path,
      });
      continue;
    }

    const decoded = decodeText(buffer);
    if (decoded !== null) {
      resources.push({ uri, text: decoded, mimeType: 'text/plain', path });
    } else {
      blobs.push({
        uri,
        blob: buffer.toString('base64'),
        mimeType: 'application/octet-stream',
        path,
      });
    }
  }

  return { images, resources, blobs };
}
