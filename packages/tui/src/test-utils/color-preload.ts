/**
 * Test preload: pin the color level for the whole test process, because chalk
 * auto-detects level 0 when stdout is piped and colour assertions would then
 * pass or fail depending on the runner's TTY and on module load order.
 */

// Written to the environment (rather than assigned on the instance) so the level
// is in place before the shared chalk instance is built. An outer value wins.
process.env.KIRO_TUI_FORCE_COLOR ??= '3';
