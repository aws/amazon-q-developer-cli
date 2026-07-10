/**
 * V2 (CLI) tool → V3 (KAS) selectors — one source of truth for the `tools` tag
 * mapping (migrate.ts) and the `permissions.rules` capability mapping
 * (permissions.ts). `v3Capability` is absent for built-ins KAS runs without a
 * policy check (e.g. knowledge/todo_list).
 *
 * Source of truth for the V2 names: the tool set the Rust loaders accept —
 * `crates/agent/src/agent/tools` (V2/ACP) and `chat_cli/src/cli/chat/tools`
 * (V1). Keep this table in sync when a tool is added/renamed there.
 */
export const V2_TOOL_TO_V3_TABLE: {
  v2Names: string[];
  v3Tag: string;
  v3Capability?: string;
}[] = [
  {
    v2Names: ['read', 'fs_read', 'fsRead'],
    v3Tag: 'read',
    v3Capability: 'fs_read',
  },
  { v2Names: ['grep'], v3Tag: 'read', v3Capability: 'fs_read' },
  { v2Names: ['glob'], v3Tag: 'read', v3Capability: 'fs_read' },
  { v2Names: ['code'], v3Tag: 'read', v3Capability: 'fs_read' },
  { v2Names: ['introspect'], v3Tag: 'read', v3Capability: 'fs_read' },
  {
    v2Names: ['write', 'fs_write', 'fsWrite'],
    v3Tag: 'write',
    v3Capability: 'fs_write',
  },
  {
    v2Names: ['shell', 'execute_bash', 'execute_cmd', 'executeCmd'],
    v3Tag: 'shell',
    v3Capability: 'shell',
  },
  { v2Names: ['web_fetch'], v3Tag: 'web', v3Capability: 'web_fetch' },
  { v2Names: ['web_search'], v3Tag: 'web', v3Capability: 'web_search' },
  {
    v2Names: ['subagent', 'use_subagent', 'agent_crew'],
    v3Tag: 'subagent',
    v3Capability: 'subagent',
  },
  { v2Names: ['knowledge'], v3Tag: 'knowledge' },
  { v2Names: ['task', 'todo_list', 'todo'], v3Tag: 'todo_list' },
];

export const V3_TAG_BY_V2_NAME = new Map<string, string>(
  V2_TOOL_TO_V3_TABLE.flatMap((r) =>
    r.v2Names.map((n) => [n, r.v3Tag] as const)
  )
);

/** V2 tool name → its V3 permissions capability (absent for no-policy tools). */
export const V3_CAP_BY_V2_NAME = new Map<string, string>(
  V2_TOOL_TO_V3_TABLE.flatMap((r) =>
    r.v3Capability ? r.v2Names.map((n) => [n, r.v3Capability!] as const) : []
  )
);

/** All accepted V2 names for the same tool as `name` (or just `name` if unknown). */
export function aliasesOfV2Name(name: string): readonly string[] {
  const entry = V2_TOOL_TO_V3_TABLE.find((r) => r.v2Names.includes(name));
  return entry ? entry.v2Names : [name];
}
