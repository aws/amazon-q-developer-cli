import { describe, it, expect } from 'bun:test';
import { handleRepo } from '../kas-handlers/repo';
import { createMockCommandContext } from './test-helpers';
import { KasCommandName, type KasCommand } from '../../kas-commands';

const cmd = { name: KasCommandName.Repo } as KasCommand;

function ctxWithSource(
  source: unknown,
  kiroExtra: Record<string, unknown> = {}
) {
  return createMockCommandContext({
    kiro: { getRepoProviderSource: () => source, ...kiroExtra } as any,
  });
}

describe('handleRepo (/repo)', () => {
  it('is a no-op when the session is not a cloud session (dispatch gate)', async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({ providers: [] }),
      listSourceProviderResources: async () => undefined,
    });
    ctx.cloudSessionActive = false;
    await handleRepo(cmd, '', ctx);
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('attaches directly for `/repo owner/name`, skipping the picker', async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({ providers: [] }),
      listSourceProviderResources: async () => undefined,
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, ' owner/repo ', ctx);
    expect(ctx._spies.sendMessage).toHaveBeenCalledWith(
      'Clone the repository owner/repo into the workspace.'
    );
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
  });

  it('opens the picker with fetched resources when a provider is connected', async () => {
    const resources = [{ providerType: 'GITHUB', name: 'kiro/app' }];
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({
        providers: [
          {
            providerType: 'GITHUB',
            displayName: 'GitHub',
            connectionStatus: 'connected',
          },
        ],
      }),
      listSourceProviderResources: async () => ({ resources }),
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, '', ctx);
    expect(ctx._spies.setShowRepoPicker).toHaveBeenCalledWith(true, resources);
  });

  it('shows the web-portal handoff when no provider is connected', async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({
        providers: [
          {
            providerType: 'GITHUB',
            displayName: 'GitHub',
            connectionStatus: 'not_connected',
            setupUrl: 'https://kiro.dev/settings/providers',
          },
        ],
      }),
      listSourceProviderResources: async () => undefined,
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, '', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalled();
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
  });

  it('shows the generic connect message when the disconnected provider has no setup URL', async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({
        providers: [
          {
            providerType: 'GITHUB',
            displayName: 'GitHub',
            connectionStatus: 'not_connected',
          },
        ],
      }),
      listSourceProviderResources: async () => undefined,
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, '', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No connected source provider. Connect one in the Kiro web portal, then try again.',
      'warning',
      6000
    );
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
  });

  it('falls back to direct-attach guidance when provider listing is unavailable', async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => undefined,
      listSourceProviderResources: async () => undefined,
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, '', ctx);
    const [message, status] = (ctx._spies.showAlert as any).mock.calls[0];
    expect(message).toContain('/repo <owner/name>');
    expect(status).toBe('warning');
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
  });

  it("shows 'no repositories' when the connected provider returns an empty page", async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({
        providers: [
          {
            providerType: 'GITHUB',
            displayName: 'GitHub',
            connectionStatus: 'connected',
          },
        ],
      }),
      listSourceProviderResources: async () => ({ resources: [] }),
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, '', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No repositories available to attach.',
      'warning',
      5000
    );
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
  });

  it('falls back to direct-attach guidance when the resource page is unavailable', async () => {
    const ctx = ctxWithSource({
      listSourceProviders: async () => ({
        providers: [
          {
            providerType: 'GITHUB',
            displayName: 'GitHub',
            connectionStatus: 'connected',
          },
        ],
      }),
      listSourceProviderResources: async () => undefined,
    });
    ctx.cloudSessionActive = true;
    await handleRepo(cmd, '', ctx);
    const [message, status] = (ctx._spies.showAlert as any).mock.calls[0];
    expect(message).toContain('/repo <owner/name>');
    expect(status).toBe('warning');
    expect(ctx._spies.setShowRepoPicker).not.toHaveBeenCalled();
  });
});
