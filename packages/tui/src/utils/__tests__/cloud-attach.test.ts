import { afterAll, beforeAll, describe, expect, it } from 'bun:test';
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { collectCloudAttachments, findLocalPaths } from '../cloud-attach.js';

const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64'
);

let dir: string;
let png: string;
let spacedPng: string;
let txt: string;
let spacedTxt: string;
let bin: string;

beforeAll(() => {
  dir = mkdtempSync(join(tmpdir(), 'kiro-cloud-attach-'));
  png = join(dir, 'shot.png');
  writeFileSync(png, TINY_PNG);
  spacedPng = join(dir, 'Screenshot 2026-07-22 at 1.22.52 PM.png');
  writeFileSync(spacedPng, TINY_PNG);
  txt = join(dir, 'notes.txt');
  writeFileSync(txt, 'hello world\nsecond line\n');
  spacedTxt = join(dir, 'secret #100% sauce.txt');
  writeFileSync(spacedTxt, 'the word is XYLOPHONE-42\n');
  bin = join(dir, 'blob.bin');
  // Invalid UTF-8 keeps this fixture unambiguously binary.
  writeFileSync(bin, Buffer.from([0x50, 0x4b, 0xff, 0xfe, 0x01]));
});

afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('findLocalPaths', () => {
  it('finds a bare absolute path', async () => {
    expect(await findLocalPaths(`what is in ${png}?`)).toEqual([png]);
  });

  it('finds a quoted path with spaces', async () => {
    expect(await findLocalPaths(`what is in "${spacedPng}"`)).toEqual([
      spacedPng,
    ]);
  });

  it('finds an unquoted path with spaces via longest-prefix match', async () => {
    expect(await findLocalPaths(`${spacedPng} what is this`)).toEqual([
      spacedPng,
    ]);
  });

  it('finds a leading path without probing an unbounded prose suffix', async () => {
    const prose = Array.from({ length: 100 }, (_, index) => `word${index}`);
    expect(await findLocalPaths(`${png} ${prose.join(' ')}`)).toEqual([png]);
  });

  it('strips trailing prose punctuation', async () => {
    expect(await findLocalPaths(`look at ${png}, then report`)).toEqual([png]);
  });

  it('dedupes repeated references', async () => {
    expect(await findLocalPaths(`${png} and again ${png}`)).toEqual([png]);
  });

  it('ignores non-existent paths and non-path slashes', async () => {
    expect(await findLocalPaths('a/b vs c/d and /no/such/file.png')).toEqual(
      []
    );
  });

  it('ignores directories', async () => {
    expect(await findLocalPaths(dir)).toEqual([]);
  });

  describe('macOS screenshot whitespace filenames', () => {
    const NNBSP = '\u202f';
    let realPath: string;

    beforeAll(() => {
      realPath = join(dir, `Screenshot 2026-07-22 at 4.59.51${NNBSP}PM.png`);
      writeFileSync(realPath, TINY_PNG);
    });

    it('resolves when U+202F was replaced by a plain space', async () => {
      const mangled = join(dir, 'Screenshot 2026-07-22 at 4.59.51 PM.png');
      expect(await findLocalPaths(`"${mangled}" describe`)).toEqual([realPath]);
    });

    it('resolves when U+202F was stripped entirely', async () => {
      const mangled = join(dir, 'Screenshot 2026-07-22 at 4.59.51PM.png');
      expect(await findLocalPaths(`"${mangled}" describe`)).toEqual([realPath]);
    });

    it('resolves the exact name', async () => {
      expect(await findLocalPaths(`"${realPath}" describe`)).toEqual([
        realPath,
      ]);
    });

    it('bails when two files normalize identically', async () => {
      const NBSP = '\u00a0';
      writeFileSync(join(dir, `ambig${NNBSP}x.png`), TINY_PNG);
      writeFileSync(join(dir, `ambig${NBSP}x.png`), TINY_PNG);
      expect(await findLocalPaths(`"${join(dir, 'ambig x.png')}"`)).toEqual([]);
    });
  });

  it('does not scan paths inside expanded attached-file content', async () => {
    const expanded = `<attached_file path="${txt}">\nreference ${bin}\n</attached_file>`;
    expect(await findLocalPaths(expanded)).toEqual([]);
  });
});

describe.if(process.platform !== 'win32')(
  'macOS screenshot paths in unlistable directories',
  () => {
    it('resolves mangled whitespace through direct candidate probes', async () => {
      const protectedDir = join(dir, 'protected');
      const NNBSP = '\u202f';
      const plainSpaceName = `Screenshot 2026-07-23 at 2.50.19${NNBSP}AM.png`;
      const deletedSpaceName = `Screenshot 2026-07-23 at 3.51.20${NNBSP}PM.png`;
      mkdirSync(protectedDir);
      writeFileSync(join(protectedDir, plainSpaceName), TINY_PNG);
      writeFileSync(join(protectedDir, deletedSpaceName), TINY_PNG);
      chmodSync(protectedDir, 0o100);

      try {
        expect(() => readdirSync(protectedDir)).toThrow();
        expect(
          await findLocalPaths(
            `"${join(protectedDir, 'Screenshot 2026-07-23 at 2.50.19 AM.png')}"`
          )
        ).toEqual([join(protectedDir, plainSpaceName)]);
        expect(
          await findLocalPaths(
            `"${join(protectedDir, 'Screenshot 2026-07-23 at 3.51.20PM.png')}"`
          )
        ).toEqual([join(protectedDir, deletedSpaceName)]);
      } finally {
        chmodSync(protectedDir, 0o700);
      }
    });
  }
);

describe('collectCloudAttachments', () => {
  it('returns an image payload for an image path', async () => {
    const result = await collectCloudAttachments(`describe ${png}`);
    expect(result.images).toHaveLength(1);
    expect(result.images[0]!.mimeType).toBe('image/png');
    expect(result.images[0]!.width).toBe(1);
    expect(Buffer.from(result.images[0]!.base64, 'base64')).toEqual(TINY_PNG);
    expect(result.resources).toHaveLength(0);
  });

  it('returns a text resource for a text path with spaces', async () => {
    const result = await collectCloudAttachments(
      `"${spacedTxt}" what is the word?`
    );
    expect(result.resources).toHaveLength(1);
    expect(result.resources[0]!.text).toContain('XYLOPHONE-42');
    expect(result.resources[0]!.uri).toBe(pathToFileURL(spacedTxt).href);
    expect(result.resources[0]!.mimeType).toBe('text/plain');
    expect(result.resources[0]!.uri).toContain('%20');
    expect(result.resources[0]!.uri).toContain('%23');
    expect(result.resources[0]!.uri).toContain('%25');
    expect(result.images).toHaveLength(0);
  });

  it('collects a mix of images and text files', async () => {
    const result = await collectCloudAttachments(`compare ${png} with ${txt}`);
    expect(result.images).toHaveLength(1);
    expect(result.resources).toHaveLength(1);
  });

  it('ships unknown binary files as octet-stream blobs', async () => {
    const result = await collectCloudAttachments(`open ${bin}`);
    expect(result.images).toHaveLength(0);
    expect(result.resources).toHaveLength(0);
    expect(result.blobs).toHaveLength(1);
    expect(result.blobs[0]!.mimeType).toBe('application/octet-stream');
  });

  it('classifies valid UTF-8 containing a NUL byte as binary', async () => {
    const lateNul = join(dir, 'late-nul.bin');
    writeFileSync(
      lateNul,
      Buffer.concat([
        Buffer.alloc(8192, 'a'),
        Buffer.from([0]),
        Buffer.from('tail'),
      ])
    );

    const result = await collectCloudAttachments(`open ${lateNul}`);
    expect(result.resources).toHaveLength(0);
    expect(result.blobs).toHaveLength(1);
    expect(result.blobs[0]!.mimeType).toBe('application/octet-stream');
  });

  it('ships PDFs as blob resources with the document MIME type', async () => {
    const pdf = join(dir, 'receipt.pdf');
    writeFileSync(pdf, Buffer.from('%PDF-1.4 fake'));
    const result = await collectCloudAttachments(`what is in ${pdf}`);
    expect(result.blobs).toHaveLength(1);
    expect(result.blobs[0]!.uri).toBe(pathToFileURL(pdf).href);
    expect(result.blobs[0]!.mimeType).toBe('application/pdf');
    expect(Buffer.from(result.blobs[0]!.blob, 'base64').toString()).toContain(
      '%PDF'
    );
  });

  it('does not size-cap attachments', async () => {
    const big = join(dir, 'big.png');
    writeFileSync(big, Buffer.alloc(11 * 1024 * 1024));
    const result = await collectCloudAttachments(`describe ${big}`);
    expect(result.images).toHaveLength(1);
    expect(result.images[0]!.sizeBytes).toBe(11 * 1024 * 1024);
  });

  it('handles home expansion without throwing', async () => {
    const result = await collectCloudAttachments(
      'read ~/definitely-not-a-real-file-xyz.txt'
    );
    expect(result.images).toHaveLength(0);
    expect(result.resources).toHaveLength(0);
  });

  it('stops collection when aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    const result = await collectCloudAttachments(
      `describe ${png}`,
      controller.signal
    );
    expect(result).toEqual({ images: [], resources: [], blobs: [] });
  });
});
