import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../../src/constants/agents';

export { KAS_DEFAULT_AGENT_ID, KAS_DEFAULT_AGENT_NAME };

export const DEFAULT_KAS_MODE = {
  id: KAS_DEFAULT_AGENT_ID,
  name: KAS_DEFAULT_AGENT_NAME,
} as const;

export function defaultKasModes(currentModeId = KAS_DEFAULT_AGENT_ID) {
  return {
    currentModeId,
    availableModes: [{ ...DEFAULT_KAS_MODE }],
  };
}
