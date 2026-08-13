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

  it('copies the URL without invoking the fallback and reports success', async () => {
    const copyToClipboard = mock(() => true);
    const onCopyFailure = mock(() => {});
    const cap = createCopyUrlToClipboardCapability(
      copyToClipboard,
      onCopyFailure
    );

    const res = (await cap.handler({ url: URL })) as CopyUrlToClipboardResponse;

    expect(copyToClipboard).toHaveBeenCalledWith(URL);
    expect(onCopyFailure).not.toHaveBeenCalled();
    expect(res.success).toBe(true);
  });

  it('invokes one sensitive-URL fallback and reports failure', async () => {
    const copyToClipboard = mock(() => false);
    const onCopyFailure = mock(() => {});
    const cap = createCopyUrlToClipboardCapability(
      copyToClipboard,
      onCopyFailure
    );

    const res = (await cap.handler({ url: URL })) as CopyUrlToClipboardResponse;

    expect(onCopyFailure).toHaveBeenCalledTimes(1);
    expect(onCopyFailure).toHaveBeenCalledWith(
      `Clipboard copy failed. Open this session-specific OAuth URL manually; do not share it:\n${URL}`
    );
    expect(res.success).toBe(false);
  });
});
