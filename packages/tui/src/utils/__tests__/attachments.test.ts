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
import {
  collectAttachments,
  mayReferenceImagePath,
  findLocalPaths,
  ALL_ATTACHMENT_KINDS,
  IMAGE_ATTACHMENT_KINDS,
  resolveImagePath,
} from '../attachments.js';

/** A cloud session's kinds: images, documents, text, and binaries. */
const cloudAttachments = (text: string, signal?: AbortSignal) =>
  collectAttachments(text, { kinds: ALL_ATTACHMENT_KINDS, signal });

/** An ordinary session's kinds: images only. */
const imageAttachments = async (text: string, signal?: AbortSignal) =>
  (await collectAttachments(text, { kinds: IMAGE_ATTACHMENT_KINDS, signal }))
    .images;

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
  dir = mkdtempSync(join(tmpdir(), 'kiro-attachments-'));
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

  describe('a quoted span is one candidate', () => {
    it('attaches nothing when the quoted path does not exist', async () => {
      // The prefix inside the quotes is a real file, but the user named
      // "<png> copy" — a different file, and one that is not there.
      expect(await findLocalPaths(`look at "${png} copy"`)).toEqual([]);
    });

    it('still finds an unquoted path elsewhere on the same line', async () => {
      expect(
        await findLocalPaths(`compare "${png} copy" against ${png}`)
      ).toEqual([png]);
    });

    it('attaches nothing for a quoted directory-plus-suffix', async () => {
      expect(await findLocalPaths(`"${dir} backup/shot.png"`)).toEqual([]);
    });
  });

  describe('quotes of one kind may contain the other', () => {
    let apostrophe: string;

    beforeAll(() => {
      mkdirSync(join(dir, "Bob's Photos"), { recursive: true });
      apostrophe = join(dir, "Bob's Photos", 'cat.png');
      writeFileSync(apostrophe, TINY_PNG);
    });

    it('finds a double-quoted path containing an apostrophe', async () => {
      // The apostrophe is part of the name, not the closing quote.
      expect(await findLocalPaths(`"${apostrophe}"`)).toEqual([apostrophe]);
    });

    it('finds a single-quoted path containing an apostrophe', async () => {
      expect(await findLocalPaths(`'${apostrophe}'`)).toEqual([apostrophe]);
    });

    it('finds it unquoted too', async () => {
      expect(await findLocalPaths(`look at ${apostrophe} please`)).toEqual([
        apostrophe,
      ]);
    });

    it('is unbothered by an apostrophe in the surrounding prose', async () => {
      expect(await findLocalPaths(`what's in "${png}"?`)).toEqual([png]);
    });
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

describe('collectAttachments (all kinds)', () => {
  it('returns an image payload for an image path', async () => {
    const result = await cloudAttachments(`describe ${png}`);
    expect(result.images).toHaveLength(1);
    expect(result.images[0]!.mimeType).toBe('image/png');
    expect(result.images[0]!.width).toBe(1);
    expect(Buffer.from(result.images[0]!.base64, 'base64')).toEqual(TINY_PNG);
    expect(result.resources).toHaveLength(0);
  });

  it('returns a text resource for a text path with spaces', async () => {
    const result = await cloudAttachments(`"${spacedTxt}" what is the word?`);
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
    const result = await cloudAttachments(`compare ${png} with ${txt}`);
    expect(result.images).toHaveLength(1);
    expect(result.resources).toHaveLength(1);
  });

  it('ships unknown binary files as octet-stream blobs', async () => {
    const result = await cloudAttachments(`open ${bin}`);
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

    const result = await cloudAttachments(`open ${lateNul}`);
    expect(result.resources).toHaveLength(0);
    expect(result.blobs).toHaveLength(1);
    expect(result.blobs[0]!.mimeType).toBe('application/octet-stream');
  });

  it('ships PDFs as blob resources with the document MIME type', async () => {
    const pdf = join(dir, 'receipt.pdf');
    writeFileSync(pdf, Buffer.from('%PDF-1.4 fake'));
    const result = await cloudAttachments(`what is in ${pdf}`);
    expect(result.blobs).toHaveLength(1);
    expect(result.blobs[0]!.uri).toBe(pathToFileURL(pdf).href);
    expect(result.blobs[0]!.mimeType).toBe('application/pdf');
    expect(Buffer.from(result.blobs[0]!.blob, 'base64').toString()).toContain(
      '%PDF'
    );
  });

  it('does not size-cap attachments', async () => {
    // Deliberately uncapped: the relay and the model enforce their own limits,
    // and a guess here would refuse files they would have accepted.
    const big = join(dir, 'big.png');
    writeFileSync(big, Buffer.alloc(11 * 1024 * 1024));
    const result = await cloudAttachments(`describe ${big}`);
    expect(result.images).toHaveLength(1);
    expect(result.images[0]!.sizeBytes).toBe(11 * 1024 * 1024);
  });

  it('handles home expansion without throwing', async () => {
    const result = await cloudAttachments(
      'read ~/definitely-not-a-real-file-xyz.txt'
    );
    expect(result.images).toHaveLength(0);
    expect(result.resources).toHaveLength(0);
  });

  it('stops collection when aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    const result = await cloudAttachments(`describe ${png}`, controller.signal);
    expect(result).toEqual({ images: [], resources: [], blobs: [] });
  });
});

describe('collectAttachments (images only)', () => {
  it('attaches a referenced image path', async () => {
    const images = await imageAttachments(`describe ${png}`);
    expect(images).toHaveLength(1);
    expect(images[0]!.mimeType).toBe('image/png');
    expect(images[0]!.path).toBe(png);
    expect(images[0]!.base64).toBe(TINY_PNG.toString('base64'));
    expect(images[0]!.sizeBytes).toBe(TINY_PNG.byteLength);
  });

  it('reads PNG dimensions', async () => {
    const images = await imageAttachments(png);
    expect(images[0]!.width).toBe(1);
    expect(images[0]!.height).toBe(1);
  });

  it('attaches an image path containing spaces', async () => {
    const images = await imageAttachments(`what is in ${spacedPng}?`);
    expect(images).toHaveLength(1);
    expect(images[0]!.path).toBe(spacedPng);
  });

  it('attaches multiple referenced images', async () => {
    const images = await imageAttachments(`compare ${png} with ${spacedPng}`);
    expect(images.map((image) => image.path).sort()).toEqual(
      [png, spacedPng].sort()
    );
  });

  it('ignores a text file, unlike the cloud collector', async () => {
    expect(await imageAttachments(`read ${txt}`)).toEqual([]);
    // Same input still produces a resource on the cloud path.
    const cloud = await cloudAttachments(`read ${txt}`);
    expect(cloud.resources).toHaveLength(1);
  });

  it('ignores a binary file', async () => {
    expect(await imageAttachments(`what is ${bin}`)).toEqual([]);
  });

  it('picks only the image out of a mixed prompt', async () => {
    const images = await imageAttachments(
      `compare ${png} with ${txt} and ${bin}`
    );
    expect(images).toHaveLength(1);
    expect(images[0]!.path).toBe(png);
  });

  it('returns nothing when no path is referenced', async () => {
    expect(await imageAttachments('just a question')).toEqual([]);
  });

  it('does not throw on a non-existent path', async () => {
    expect(await imageAttachments('see ~/definitely-not-real-xyz.png')).toEqual(
      []
    );
  });

  it('stops collection when aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    expect(
      await imageAttachments(`describe ${png}`, controller.signal)
    ).toEqual([]);
  });
});

describe('mayReferenceImagePath', () => {
  it('is true for each supported image extension', () => {
    for (const ext of ['png', 'jpeg', 'jpg', 'gif', 'webp']) {
      expect(mayReferenceImagePath(`see /tmp/a.${ext}`)).toBe(true);
    }
  });

  it('is case-insensitive', () => {
    expect(mayReferenceImagePath('/tmp/Shot.PNG')).toBe(true);
  });

  it('is false for a plain question', () => {
    expect(mayReferenceImagePath('what does this function do?')).toBe(false);
  });

  it('is false for a slash command, which the path scanner would otherwise probe', () => {
    expect(mayReferenceImagePath('/config')).toBe(false);
    expect(mayReferenceImagePath('/model sonnet')).toBe(false);
  });

  it('is false for non-image file references', () => {
    expect(mayReferenceImagePath('read /tmp/notes.txt')).toBe(false);
    expect(mayReferenceImagePath('open /tmp/report.pdf')).toBe(false);
  });

  it('is true when whitespace splits the extension, which still resolves', () => {
    // findLocalPaths matches directory entries with whitespace stripped, so
    // this text resolves to a real image; a false here would drop it silently.
    expect(mayReferenceImagePath('/tmp/shot.pn g')).toBe(true);
    expect(mayReferenceImagePath('/tmp/shot.p n g')).toBe(true);
  });

  it('never suppresses a scan that would have found an image', async () => {
    // The guard is only useful if false implies an empty collection.
    for (const text of [
      `describe ${png}`,
      `what is in ${spacedPng}?`,
      `compare ${png} with ${txt}`,
      `describe ${png.replace('.png', '.pn g')}`,
      `describe ${png.replace('shot', 'sh ot')}`,
    ]) {
      const images = await imageAttachments(text);
      if (images.length > 0) expect(mayReferenceImagePath(text)).toBe(true);
    }
  });
});

describe('collectAttachments kind selection', () => {
  it('returns nothing for an empty kind set', async () => {
    const result = await collectAttachments(`describe ${png}`, {
      kinds: new Set(),
    });
    expect(result).toEqual({ images: [], resources: [], blobs: [] });
  });

  it('excludes documents when only images are wanted', async () => {
    const doc = join(dir, 'kinds-receipt.pdf');
    writeFileSync(doc, Buffer.from('%PDF-1.4\n%%EOF\n'));
    expect(await imageAttachments(`what is in ${doc}`)).toEqual([]);
    // The same path is still a blob for a caller that wants documents.
    const cloud = await cloudAttachments(`what is in ${doc}`);
    expect(cloud.blobs).toHaveLength(1);
    expect(cloud.blobs[0]!.mimeType).toBe('application/pdf');
  });

  it('skips an unreadable image without failing the collection', async () => {
    const unreadable = join(dir, 'locked.png');
    writeFileSync(unreadable, TINY_PNG);
    chmodSync(unreadable, 0o000);
    try {
      expect(await imageAttachments(`describe ${unreadable}`)).toEqual([]);
      // A readable image alongside it is still collected.
      const both = await imageAttachments(`${unreadable} and ${png}`);
      expect(both.map((image) => image.path)).toEqual([png]);
    } finally {
      chmodSync(unreadable, 0o644);
    }
  });
});

describe('resolveImagePath', () => {
  it('resolves an existing image path with its type and size', () => {
    const resolved = resolveImagePath(png);
    expect(resolved).toMatchObject({
      path: png,
      mimeType: 'image/png',
      sizeBytes: TINY_PNG.byteLength,
    });
  });

  it('tolerates surrounding whitespace', () => {
    expect(resolveImagePath(`  ${png}  `)?.path).toBe(png);
  });

  it('resolves a path containing spaces', () => {
    expect(resolveImagePath(spacedPng)?.path).toBe(spacedPng);
  });

  it('returns null for a non-image file', () => {
    expect(resolveImagePath(txt)).toBeNull();
  });

  it('returns null for a path that does not exist', () => {
    expect(resolveImagePath(join(dir, 'absent.png'))).toBeNull();
  });

  it('returns null for a directory with an image extension', () => {
    const dirPath = join(dir, 'not-a-file.png');
    mkdirSync(dirPath, { recursive: true });
    expect(resolveImagePath(dirPath)).toBeNull();
  });

  it('returns null for relative paths and multi-line text', () => {
    expect(resolveImagePath('shot.png')).toBeNull();
    expect(resolveImagePath('./shot.png')).toBeNull();
    expect(resolveImagePath(`${png}\n${png}`)).toBeNull();
  });

  it('returns null for empty input', () => {
    expect(resolveImagePath('')).toBeNull();
    expect(resolveImagePath('   ')).toBeNull();
  });

  it('agrees with the scan: a resolved path is one the scan attaches', async () => {
    // The chip promises an attachment, so the two must not disagree.
    const resolved = resolveImagePath(png);
    expect(resolved).not.toBeNull();
    const images = await imageAttachments(`describe ${resolved!.path}`);
    expect(images.map((image) => image.path)).toEqual([png]);
  });

  it('repairs a screenshot name whose narrow no-break space became a space', async () => {
    // Pasting a macOS screenshot path often loses U+202F before AM/PM. The
    // send-time scan repairs it, so the chip must too — otherwise the prompt
    // shows a bare path for a file that does arrive.
    const NNBSP = '\u202f';
    const real = join(dir, `Screenshot 2026-07-24 at 9.15.00${NNBSP}AM.png`);
    writeFileSync(real, TINY_PNG);
    const typed = join(dir, 'Screenshot 2026-07-24 at 9.15.00 AM.png');

    expect(resolveImagePath(typed)?.path).toBe(real);
    // The repaired path is what the chip emits, and it attaches as itself.
    expect((await imageAttachments(typed)).map((image) => image.path)).toEqual([
      real,
    ]);
  });

  it('chips a large image, matching the uncapped scan', () => {
    // Chip and scan must agree on what attaches; neither imposes a size limit.
    const big = join(dir, 'chip-large.png');
    writeFileSync(big, Buffer.alloc(11 * 1024 * 1024));
    expect(resolveImagePath(big)?.sizeBytes).toBe(11 * 1024 * 1024);
  });
});
