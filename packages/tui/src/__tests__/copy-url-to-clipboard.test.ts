import { describe, expect, it, mock } from 'bun:test';
import {
  createCopyUrlToClipboardCapability,
  OPEN_EXTERNAL_URL_METHOD,
  type CopyUrlToClipboardResponse,
} from '../capabilities/copy-url-to-clipboard';

const URL = 'https://example.com/oauth?state=abc&client_id=xyz';

describe('copy-url-to-clipboard capability', () => {
  it('registers under the openExternalUrl wire method', () => {
    const cap = createCopyUrlToClipboardCapability(() => true);
    expect(cap.method).toBe(OPEN_EXTERNAL_URL_METHOD);
    expect(cap.method).toBe('_kiro/openExternalUrl');
    expect(cap.key).toBe('openExternalUrl');
  });

  it('copies the URL to the clipboard and reports success', async () => {
    const copyToClipboard = mock(() => true);
    const cap = createCopyUrlToClipboardCapability(copyToClipboard);

    const res = (await cap.handler({ url: URL })) as CopyUrlToClipboardResponse;

    expect(copyToClipboard).toHaveBeenCalledWith(URL);
    expect(res.success).toBe(true);
  });

  it('reports failure when the clipboard copy fails', async () => {
    const copyToClipboard = mock(() => false);
    const cap = createCopyUrlToClipboardCapability(copyToClipboard);

    const res = (await cap.handler({ url: URL })) as CopyUrlToClipboardResponse;

    expect(res.success).toBe(false);
  });
});
