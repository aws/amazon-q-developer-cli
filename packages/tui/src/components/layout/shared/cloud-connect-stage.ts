/**
 * Which boot indicator the cloud connecting screen shows before the session
 * exists. A rejected `agent_connect` leaves `cloudProviderChecked` false, so a
 * "show the spinner while unchecked" rule alone spins forever next to the error
 * alert. Treat a failed connect as terminal: show a failed row and no spinner.
 *
 *  - `'failed'`: the connect rejected — render a terminal failed row, no spinner.
 *  - `'connecting'`: still connecting (provider probe not yet resolved).
 *  - `'checked'`: the probe resolved — the welcome/checklist takes over.
 */
export type CloudConnectStage = 'failed' | 'connecting' | 'checked';

export function cloudConnectStage(
  cloudProviderChecked: boolean,
  agentConnectStatus: string | undefined
): CloudConnectStage {
  if (agentConnectStatus === 'failed') return 'failed';
  if (!cloudProviderChecked) return 'connecting';
  return 'checked';
}
