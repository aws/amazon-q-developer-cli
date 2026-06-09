/**
 * Constants for the dual-mode "interrupt behavior" feature.
 *
 * 1. The **persisted setting value** stored under `chat.defaultInterruptBehavior`
 *    and settable on the CLI via
 *    `kiro-cli settings set chat.defaultInterruptBehavior steer|queue`. These
 *    tokens are a documented, frozen external contract.
 *
 * 2. The **runtime mode** held in the in-store `activeInterruptMode` flag that
 *    routes input (steer = inject mid-turn, queue = buffer until turn ends).
 */

/** Interrupt mode tokens, shared by the persisted setting and runtime flag. */
export const InterruptMode = {
  /** Inject the message mid-turn at the next tool boundary. */
  STEER: 'steer',
  /** Buffer the message locally and send after the turn ends. */
  QUEUE: 'queue',
} as const;

export type InterruptMode = (typeof InterruptMode)[keyof typeof InterruptMode];

/** The default mode when the setting is absent or invalid. */
export const DEFAULT_INTERRUPT_MODE: InterruptMode = InterruptMode.STEER;

/**
 * Coerce a raw persisted setting value into a valid {@link InterruptMode},
 * falling back to the default (steer) for unknown or absent values.
 */
export function parseInterruptMode(
  value: string | null | undefined
): InterruptMode {
  return value === InterruptMode.QUEUE
    ? InterruptMode.QUEUE
    : InterruptMode.STEER;
}
