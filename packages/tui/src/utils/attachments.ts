/**
 * In-band attachments for local file paths referenced in a prompt.
 *
 * A prompt that names a file on disk can carry that file's contents to the
 * model. Callers choose which kinds they want: a cloud session takes images,
 * documents, text, and binaries, while an ordinary session takes images only.
 * Kinds the caller did not ask for are never read.
 */

import { readFile, readdir, stat } from 'node:fs/promises';
import { statSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, extname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { readPngDimensions } from './clipboard-image.js';
import { logger } from './logger.js';
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

export interface ImageAttachment {
  base64: string;
  mimeType: string;
  width: number;
  height: number;
  sizeBytes: number;
  path: string;
}

export interface ResourceAttachment {
  uri: string;
  text: string;
  mimeType: string;
  path: string;
}

export interface BlobAttachment {
  uri: string;
  blob: string;
  mimeType: string;
  path: string;
}

export interface Attachments {
  images: ImageAttachment[];
  resources: ResourceAttachment[];
  blobs: BlobAttachment[];
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

/**
 * Sibling paths worth probing when a path does not exist as typed, because a
 * copied macOS screenshot name loses the narrow no-break space before AM/PM.
 */
const whitespaceRepairCandidates = (path: string): string[] => {
  const directory = dirname(path);
  const name = basename(path);
  if (directory === path || !name) return [];
  return unicodeWhitespaceCandidates(name).map((candidate) =>
    join(directory, candidate)
  );
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
    for (const candidatePath of whitespaceRepairCandidates(path)) {
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

  // Quote by kind, so a double-quoted path may contain apostrophes and a
  // single-quoted one may contain double quotes.
  const quotedPath =
    /"((?:[A-Za-z]:[\\/]|~[\\/]|\/)[^"\n]+)"|'((?:[A-Za-z]:[\\/]|~[\\/]|\/)[^'\n]+)'/g;
  // Quotes delimit exactly one candidate, so a quoted span that does not
  // resolve is a miss rather than an invitation to attach a shorter path
  // hiding inside it: "/photos/cat.png copy" must not attach /photos/cat.png.
  const failedQuotes: Array<{ start: number; end: number; path: string }> = [];
  for (
    let match = quotedPath.exec(text);
    match;
    match = quotedPath.exec(text)
  ) {
    if (signal?.aborted) return found;
    const candidate = expandHome(match[1] ?? match[2]!);
    const resolved = await resolvePath(candidate);
    if (resolved) add(resolved);
    else
      failedQuotes.push({
        start: match.index,
        end: match.index + match[0].length,
        path: candidate,
      });
  }

  /** Inside a quoted span that missed, only the whole span was ever named. */
  const namedOnlyAsPartOfFailedQuote = (
    at: number,
    candidate: string
  ): boolean =>
    failedQuotes.some(
      (span) =>
        at >= span.start &&
        at < span.end &&
        span.path.length > candidate.length &&
        span.path.startsWith(candidate)
    );

  let lineStart = 0;
  for (const line of text.split('\n')) {
    const lineOffset = lineStart;
    lineStart += line.length + 1;
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

        const expanded = expandHome(candidate);
        if (namedOnlyAsPartOfFailedQuote(lineOffset + start, expanded))
          continue;

        const resolved = await resolvePath(expanded);
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

export const imageMimeForPath = (path: string): string | undefined =>
  IMAGE_MIME_BY_EXT[extname(path).toLowerCase()];

export interface ResolvedImagePath {
  path: string;
  mimeType: string;
  sizeBytes: number;
}

/**
 * Facts about an existing image file this text names, or null.
 *
 * Synchronous so a caller inside a keypress handler can decide without
 * yielding: a few stats, and the bytes are read later when the turn is sent.
 * It probes the path as given and its whitespace-repaired variants, reporting
 * whichever resolves. It does not list the directory, so a name the send-time
 * scan recovers that way gets no chip and still attaches as plain path text.
 */
export function resolveImagePath(text: string): ResolvedImagePath | null {
  const trimmed = text.trim();
  if (!trimmed || trimmed.includes('\n')) return null;

  const path = expandHome(trimmed);
  const isAbsolute = path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);
  if (!isAbsolute) return null;

  const mimeType = imageMimeForPath(path);
  if (!mimeType) return null;

  for (const candidate of [path, ...whitespaceRepairCandidates(path)]) {
    let entry;
    try {
      entry = statSync(candidate);
    } catch {
      continue;
    }
    if (!entry.isFile()) return null;
    return { path: candidate, mimeType, sizeBytes: entry.size };
  }
  return null;
}

// Derived from the MIME table so a new supported format cannot be missed here.
const IMAGE_EXTENSION_PATTERN = new RegExp(
  `\\.(?:${Object.keys(IMAGE_MIME_BY_EXT)
    .map((extension) => extension.slice(1))
    .join('|')})\\b`,
  'i'
);

/**
 * Cheap synchronous test for whether scanning is worth its filesystem cost.
 *
 * False means a scan would find no image, so callers can skip it and stay
 * synchronous — which matters because prompt text such as a leading slash
 * command otherwise looks path-shaped to the scanner.
 */
export function mayReferenceImagePath(text: string): boolean {
  // Path resolution matches directory entries with whitespace normalised and
  // stripped, so an extension split by a space still resolves to a real image.
  // Testing the stripped text too keeps the false-means-empty property true.
  return (
    IMAGE_EXTENSION_PATTERN.test(text) ||
    IMAGE_EXTENSION_PATTERN.test(text.replace(/\s/g, ''))
  );
}

/**
 * Base64 of a buffer, or null when it cannot be encoded.
 *
 * A file past the runtime's maximum string length throws here rather than
 * truncating; returning null keeps that a skipped attachment instead of a
 * failed turn.
 */
function toBase64(buffer: Buffer, path: string): string | null {
  try {
    return buffer.toString('base64');
  } catch (error) {
    logger.warn('[attachments] file too large to encode, skipping', {
      path,
      sizeBytes: buffer.byteLength,
      error,
    });
    return null;
  }
}

// Only PNG carries dimensions we can read without a decoder; other formats
// report 0 so callers must treat the size as unknown rather than empty.
function buildImageAttachment(
  buffer: Buffer,
  path: string,
  mimeType: string
): ImageAttachment | null {
  const base64 = toBase64(buffer, path);
  if (base64 === null) return null;
  const dimensions =
    mimeType === 'image/png' ? readPngDimensions(buffer) : null;
  return {
    base64,
    mimeType,
    width: dimensions?.width ?? 0,
    height: dimensions?.height ?? 0,
    sizeBytes: buffer.byteLength,
    path,
  };
}

export type AttachmentKind = 'image' | 'document' | 'text' | 'binary';

/** Every kind a cloud prompt carries in-band. */
export const ALL_ATTACHMENT_KINDS: ReadonlySet<AttachmentKind> = new Set([
  'image',
  'document',
  'text',
  'binary',
]);

/** Images alone — what a session attaches regardless of where it runs. */
export const IMAGE_ATTACHMENT_KINDS: ReadonlySet<AttachmentKind> = new Set([
  'image',
]);

/** An empty result, for callers that can rule out a scan without running one. */
export const noAttachments = (): Attachments => ({
  images: [],
  resources: [],
  blobs: [],
});

type AttachmentTarget =
  | { kind: 'image'; mimeType: string }
  | { kind: 'document'; mimeType: string }
  | { kind: 'unclassified' };

type BuiltAttachment =
  | { type: 'image'; value: ImageAttachment }
  | { type: 'resource'; value: ResourceAttachment }
  | { type: 'blob'; value: BlobAttachment };

/**
 * What this path would contribute, or null when the caller does not want it.
 * Deciding from the extension keeps an unwanted file from being opened at all.
 */
function attachmentTarget(
  path: string,
  kinds: ReadonlySet<AttachmentKind>
): AttachmentTarget | null {
  const extension = extname(path).toLowerCase();

  const imageMime = IMAGE_MIME_BY_EXT[extension];
  if (imageMime) {
    return kinds.has('image') ? { kind: 'image', mimeType: imageMime } : null;
  }

  const documentMime = DOCUMENT_MIME_BY_EXT[extension];
  if (documentMime) {
    return kinds.has('document')
      ? { kind: 'document', mimeType: documentMime }
      : null;
  }

  // An unknown extension can only be classified by reading the bytes.
  return kinds.has('text') || kinds.has('binary')
    ? { kind: 'unclassified' }
    : null;
}

/** Bytes for a path that already resolved, or null when it cannot be read. */
async function readAttachment(
  path: string,
  signal?: AbortSignal
): Promise<Buffer | null> {
  try {
    return signal ? await readFile(path, { signal }) : await readFile(path);
  } catch (error) {
    if (signal?.aborted) return null;
    // The prompt still names the file, so a silent drop would leave the model
    // free to answer as though it had arrived.
    logger.warn('[attachments] referenced file could not be read', {
      path,
      error,
    });
    return null;
  }
}

/** The payload these bytes become, or null when the kind is unwanted. */
function buildAttachment(
  target: AttachmentTarget,
  buffer: Buffer,
  path: string,
  kinds: ReadonlySet<AttachmentKind>
): BuiltAttachment | null {
  if (target.kind === 'image') {
    const image = buildImageAttachment(buffer, path, target.mimeType);
    return image ? { type: 'image', value: image } : null;
  }

  const uri = pathToFileURL(path).href;

  if (target.kind === 'document') {
    const blob = toBase64(buffer, path);
    return blob === null
      ? null
      : { type: 'blob', value: { uri, blob, mimeType: target.mimeType, path } };
  }

  const decoded = decodeText(buffer);
  if (decoded !== null) {
    return kinds.has('text')
      ? {
          type: 'resource',
          value: { uri, text: decoded, mimeType: 'text/plain', path },
        }
      : null;
  }

  if (!kinds.has('binary')) return null;
  const blob = toBase64(buffer, path);
  return blob === null
    ? null
    : {
        type: 'blob',
        value: { uri, blob, mimeType: 'application/octet-stream', path },
      };
}

/**
 * Collect in-band attachments for local paths referenced in a prompt.
 *
 * Only the requested kinds are read. A path whose kind can be decided from its
 * extension is skipped before opening it, so naming a file in a prompt never
 * pulls its contents into the request when the caller did not ask for that
 * kind. An unknown extension can only be classified by reading, so it is read
 * only when text or binary is wanted.
 */
export async function collectAttachments(
  text: string,
  options: { kinds: ReadonlySet<AttachmentKind>; signal?: AbortSignal }
): Promise<Attachments> {
  const { kinds, signal } = options;
  if (kinds.size === 0) return noAttachments();

  const images: ImageAttachment[] = [];
  const resources: ResourceAttachment[] = [];
  const blobs: BlobAttachment[] = [];

  for (const path of await findLocalPaths(text, signal)) {
    if (signal?.aborted) break;

    const target = attachmentTarget(path, kinds);
    if (!target) continue;

    const buffer = await readAttachment(path, signal);
    if (!buffer) {
      if (signal?.aborted) break;
      continue;
    }

    const built = buildAttachment(target, buffer, path, kinds);
    if (built?.type === 'image') images.push(built.value);
    else if (built?.type === 'resource') resources.push(built.value);
    else if (built?.type === 'blob') blobs.push(built.value);
  }

  return { images, resources, blobs };
}
