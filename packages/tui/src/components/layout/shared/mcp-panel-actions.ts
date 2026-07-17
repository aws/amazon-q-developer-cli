import type { Kiro } from '../../../kiro.js';
import type { McpServerInfo } from '../../../stores/app-store.js';

interface RunMcpPanelActionOptions {
  kiro: Pick<Kiro, 'executeCommand'>;
  value: string;
  refreshValue: string;
  panelMode: string;
  setShowMcpPanel: (
    show: boolean,
    servers?: McpServerInfo[],
    mode?: string
  ) => void;
}

export async function runMcpPanelAction({
  kiro,
  value,
  refreshValue,
  panelMode,
  setShowMcpPanel,
}: RunMcpPanelActionOptions): Promise<void> {
  await kiro.executeCommand({
    command: 'mcp',
    args: { subcommand: value },
  });

  const result = await kiro.executeCommand({
    command: 'mcp',
    args: { subcommand: refreshValue },
  });
  if (result?.data) {
    const data = result.data as {
      servers?: McpServerInfo[];
      mode?: string;
    };
    setShowMcpPanel(true, data.servers ?? [], data.mode ?? panelMode);
  }
}
