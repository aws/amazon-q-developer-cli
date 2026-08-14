/**
 * KAS handler for `/sessions` — opens the session dashboard (local sessions)
 * or delegates to the /chat cloud-session flow when in a cloud session.
 */
import type { KasHandler } from './index.js';
import { runEffect } from '../effects.js';
import { handleChat } from './chat.js';

export const handleSessions: KasHandler = async (cmd, args, ctx, options) => {
  // Bare `/sessions` always opens the dashboard — identical in and out of a
  // cloud session. Cloud-specific subcommands (new/save/load) still route
  // through the /chat flow, which owns those.
  const isBare = (args ?? '').trim() === '';
  if (ctx.cloudSessionActive && !isBare) {
    return handleChat(cmd, args, ctx, options);
  }
  // Open the session dashboard.
  runEffect(cmd, null, ctx, args);
};
