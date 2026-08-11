/**
 * KAS handler for `/sessions` — opens the session dashboard (local sessions)
 * or delegates to the /chat cloud-session flow when in a cloud session.
 */
import type { KasHandler } from './index.js';
import { runEffect } from '../effects.js';
import { handleChat } from './chat.js';

export const handleSessions: KasHandler = async (cmd, args, ctx, options) => {
  // In a cloud session, /sessions is the cloud-session picker (same as /chat).
  if (ctx.cloudSessionActive) {
    return handleChat(cmd, args, ctx, options);
  }
  // Local: open the session dashboard.
  runEffect(cmd, null, ctx, args);
};
