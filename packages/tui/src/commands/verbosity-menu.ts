import { Settings } from '../constants/settings.js';
import type { CommandContext } from './types.js';
import type { AvailableCommand, CommandResult } from '../types/commands.js';
import {
  getVerboseConfig,
  getVerboseDisplay,
  setVerboseConfig,
  validateTokens,
  VERBOSE_CATEGORIES,
  applyDensityPreset,
  sameDisplay,
  DENSITY_PRESETS,
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  DEFAULT_DISPLAY,
  sameFilters,
  type ToolArgsMode,
  type DensityPreset,
} from '../lite/verbose.js';

/** One of the four /verbosity truncation knobs. Char caps apply per-value
 *  (chip line, individual string values inside block args, single output
 *  rows); line caps apply to the number of visual rows below the tool name. */
type TruncationField =
  | 'argsLines'
  | 'argsChars'
  | 'outputLines'
  | 'outputChars';

/**
 * /verbosity: configure lite-mode rendering. Interactive sectioned drilldown
 * (top → density preset + per-section sub-menus) plus a preserved power-user
 * CLI form: on|off, status, all, only|add|remove <list>, density <preset>,
 * reset. Rejected outside lite mode — the renderer hooks only run in
 * <LiteLayout>.
 */
export function handleVerbosity(
  _result: CommandResult | null,
  ctx: CommandContext,
  cmd: AvailableCommand,
  args: string
): boolean {
  if (ctx.getUiMode?.() !== 'lite') {
    ctx.showAlert('/verbosity is only available in lite mode', 'error', 3000);
    return true;
  }

  // Resolve the canonical /verbosity command so CommandMenu's
  // `command.name === '/verbosity'` checks fire whether reached by direct
  // typing or `/settings verbosity` (else the name is `/settings`). Falls
  // back to `cmd` when not registered (tests that only register settings).
  const verbosityCmd =
    ctx.slashCommands.find((c) => c.name === '/verbosity') ?? cmd;

  // Case folding for command matching. Internal-dispatch forms (`menu:*`,
  // `set:*`, ...) are written by the menu with exact casing, so preserve
  // them. Otherwise lowercase only the first word (so `ON`/`Density` match
  // the routing verbs) while keeping later filter tokens' case (MCP tool
  // names are case-sensitive).
  const rawTrimmed = args.trim();
  const isInternalDispatch =
    /^(menu|set|category|filter|reset):/.test(rawTrimmed) &&
    !/\s/.test(rawTrimmed);
  let trimmed: string;
  if (isInternalDispatch) {
    trimmed = rawTrimmed;
  } else {
    const firstWs = rawTrimmed.search(/\s/);
    trimmed =
      firstWs === -1
        ? rawTrimmed.toLowerCase()
        : rawTrimmed.slice(0, firstWs).toLowerCase() +
          rawTrimmed.slice(firstWs);
  }
  const cfg = getVerboseConfig();

  // `null` when the saved shape matches no preset (hand-toggled custom).
  // `default` and `full` share a display config, so the filter list
  // (DENSITY_FILTERS — `['all']` for full, `[]` for the rest) disambiguates.
  const detectActivePreset = (): DensityPreset | null => {
    const curCfg = getVerboseConfig();
    const cur = curCfg.display ?? DEFAULT_DISPLAY;
    for (const preset of DENSITY_PRESETS) {
      if (!sameDisplay(cur, DENSITY_DISPLAY[preset])) continue;
      if (!sameFilters(curCfg.filters, DENSITY_FILTERS[preset])) continue;
      return preset;
    }
    return null;
  };

  const fmtFilters = (f: string[]) => {
    if (f.length === 0) return 'none';
    if (f.length === 1 && f[0] === 'all') return 'all';
    return f.join(', ');
  };
  // Collapses with a `+N more` count when the joined form would wrap (lite's
  // word-break splits on char count, not commas). Budget from terminal width.
  const fmtFiltersForAnnounce = (f: string[]) => {
    const joined = f.join(', ');
    if (f.length <= 1) return fmtFilters(f);
    const cols = process.stdout.columns ?? 120;
    const budget = Math.max(40, cols - 30);
    if (joined.length <= budget) return joined;
    const head: string[] = [];
    let used = 0;
    for (const t of f) {
      const next = used === 0 ? t.length : used + 2 + t.length;
      if (next > budget - 12) break;
      head.push(t);
      used = next;
    }
    const remaining = f.length - head.length;
    const headStr = head.length > 0 ? head.join(', ') + ', ' : '';
    return `${f.length} (${headStr}... +${remaining} more)`;
  };
  // Status announcements no longer carry an ON/OFF prefix — the filter list
  // is the source of truth, and an empty list communicates "off" on its own.
  const showStatus = (msg?: string) => {
    const cur = getVerboseConfig();
    if (msg) {
      ctx.announceSystem(
        `${msg} · filters: ${fmtFiltersForAnnounce(cur.filters)}`
      );
      return;
    }
    const density = detectActivePreset() ?? 'custom';
    ctx.announceSystem(
      `verbosity · filters: ${fmtFiltersForAnnounce(cur.filters)} · density: ${density}`
    );
  };

  // Both `null` and any non-positive value mean "unbounded" — never render a
  // 0 or negative value as a plausible cap.
  const fmtCap = (
    cap: number | null,
    unit: 'lines' | 'chars' = 'lines'
  ): string => (cap == null || cap <= 0 ? 'unlimited' : `${cap} ${unit}`);

  // Stash the keyed parent route consumed by CommandMenu's ESC handler.
  // `null` exits; `menu:top:<key>` goes up one level.
  const setReturn = (route: string | null) => {
    ctx.setVerboseReturnOnEscape?.(route);
  };

  const openMenuWith = (
    options: Array<{
      value: string;
      label: string;
      description?: string;
      group?: string;
    }>,
    initialIndex = 0,
    previewKey?: string
  ) => {
    ctx.setActiveCommand({
      command: {
        ...verbosityCmd,
        meta: {
          ...verbosityCmd.meta,
          inputType: 'selection' as const,
          searchable: false,
        },
      },
      options,
      initialIndex,
      previewKey,
    });
  };

  const onOff = (b: boolean) => (b ? '[on]' : '[off]');

  // Top-menu row index per submenu key — ESC-back lands the cursor on the
  // row the user descended from. Must stay in sync with `openTopMenu`'s
  // option order. `thinking`/`tasks` are inline toggle rows, not submenus,
  // but are included so re-opens after toggling don't jump to row 0.
  const TOP_ROW_BY_KEY: Record<string, number> = {
    density: 0,
    tool: 1,
    subagent: 2,
    thinking: 3,
    tasks: 4,
    output: 5,
    truncation: 6,
  };

  const openTopMenu = (fromKey?: string) => {
    setReturn(null);
    const cur = getVerboseConfig();
    // getVerboseDisplay so the menu reflects the unified chat.showThinking
    // value from cli.json (the modern-TUI side), not the possibly-stale
    // lite_verbose.json copy. Filter rows below still read `cur.filters`.
    const display = getVerboseDisplay();
    const preset = detectActivePreset();
    const presetLabel = preset ?? 'custom';
    const toolSummary = `args: ${display.toolArgsMode} · reasoning: ${display.showToolReasoning ? 'on' : 'off'} · elapsed: ${display.showElapsed ? 'on' : 'off'}`;
    // Surfaces only user-meaningful knobs; `roles`/`prompts`/`deps` nest
    // under the step list and move in lockstep with it.
    const subSummaryParts: string[] = [];
    if (display.subagent.pipeline) {
      const sublist: string[] = [];
      if (display.subagent.prompts) sublist.push('instructions');
      if (display.subagent.roles) sublist.push('roles');
      const stepLabel =
        sublist.length > 0 ? `steps + ${sublist.join(' + ')}` : 'steps';
      subSummaryParts.push(stepLabel);
    }
    if (ctx.agentEngine === 'kas') {
      subSummaryParts.push('response');
    } else if (display.subagent.responses) {
      subSummaryParts.push('summary');
    }
    const fullOutputOn =
      cur.filters.includes('all') || cur.filters.includes('subagent');
    if (fullOutputOn) subSummaryParts.push('full output');
    const subSummary =
      subSummaryParts.length === 0
        ? '(all hidden)'
        : subSummaryParts.join(' · ');
    const outSummary = fmtFilters(cur.filters);
    const truncSummary = `args ${fmtCap(display.argsMaxLines)}/${fmtCap(display.argsMaxChars, 'chars')} · output ${fmtCap(display.outputMaxLines)}/${fmtCap(display.outputMaxChars, 'chars')}`;

    openMenuWith(
      [
        {
          value: 'menu:density',
          label: 'Density preset',
          description: presetLabel,
          group: 'Density',
        },
        {
          value: 'menu:tool',
          label: 'Tool calls',
          description: toolSummary,
          group: 'Sections',
        },
        {
          value: 'menu:subagent',
          label: 'Subagent',
          description: subSummary,
          group: 'Sections',
        },
        {
          value: 'set:showThinkingContent',
          label: 'Thinking content',
          description: onOff(display.showThinkingContent),
          group: 'Sections',
        },
        {
          value: 'set:showTasks',
          label: 'Task list',
          description: onOff(display.showTasks),
          group: 'Sections',
        },
        {
          value: 'menu:output',
          label: 'Show output',
          description: outSummary,
          group: 'Sections',
        },
        {
          value: 'menu:truncation',
          label: 'Truncation',
          description: truncSummary,
          group: 'Sections',
        },
      ],
      fromKey ? (TOP_ROW_BY_KEY[fromKey] ?? 0) : 0,
      'top'
    );
  };

  // Shared by the density menu rows and the confirm submenu so the confirm
  // title matches the selected row without drift.
  const PRESET_DESC: Record<DensityPreset, string> = {
    minimal: 'name only · no args, no reasoning',
    lean: 'inline arg chip, no reasoning, full elapsed',
    default: 'reasoning + block args + full subagent (out-of-the-box)',
    full: '1:1 of what the parent agent sees · all filters on · no truncation',
  };

  // Density menu — smart-entry point when a preset is active. Selecting a
  // preset routes to the `menu:density:confirm:<preset>` gate, not a commit.
  const openDensityMenu = (initialIndex = 0) => {
    // ESC fully exits — no parent above the entry point.
    setReturn(null);
    const active = detectActivePreset();
    const options: Array<{
      value: string;
      label: string;
      description: string;
    }> = DENSITY_PRESETS.map((p) => ({
      value: `menu:density:confirm:${p}`,
      label: p,
      description:
        active === p ? `[active] · ${PRESET_DESC[p]}` : PRESET_DESC[p],
    }));
    options.push({
      value: 'menu:config',
      label: 'custom',
      description:
        active == null
          ? '[active] · tweak individual settings'
          : 'tweak individual settings',
    });
    openMenuWith(options, initialIndex, 'density');
  };

  // Cancel comes first so the default cursor lands on a safe row; Yes commits
  // (display + filters) and re-opens the density menu.
  const openPresetConfirmMenu = (which: DensityPreset) => {
    setReturn('menu:density');
    openMenuWith(
      [
        {
          value: 'menu:density',
          label: 'Cancel',
          description: '',
          group: `Confirm preset: ${which}`,
        },
        {
          value: `density:apply:${which}`,
          label: `Yes, switch to ${which}`,
          description: PRESET_DESC[which],
          group: `Confirm preset: ${which}`,
        },
        { value: 'menu:density', label: '← back', description: '' },
      ],
      0,
      // Preview pane reuses the 'density' fixture set for the confirm gate.
      'density'
    );
  };

  const TRUNC_FIELDS: Record<
    TruncationField,
    {
      heading: string;
      unit: 'lines' | 'chars';
      get: (d: typeof DEFAULT_DISPLAY) => number | null;
    }
  > = {
    argsLines: {
      heading: 'Tool args · lines',
      unit: 'lines',
      get: (d) => d.argsMaxLines,
    },
    argsChars: {
      heading: 'Tool args · chars per value',
      unit: 'chars',
      get: (d) => d.argsMaxChars,
    },
    outputLines: {
      heading: 'Tool output · lines',
      unit: 'lines',
      get: (d) => d.outputMaxLines,
    },
    outputChars: {
      heading: 'Tool output · chars per line',
      unit: 'chars',
      get: (d) => d.outputMaxChars,
    },
  };

  type MenuRow = {
    value: string;
    label: string;
    description?: string;
    group?: string;
  };
  // Each entry is a thunk so rows read live display/config when opened.
  type MenuKey = 'tool' | 'subagent' | 'truncation' | 'output';
  const MENUS: Record<MenuKey, () => MenuRow[]> = {
    tool: () => {
      const display = getVerboseDisplay();
      const argModeRow = (mode: ToolArgsMode): MenuRow => ({
        value: `set:toolArgsMode:${mode}`,
        label: `Args: ${mode}`,
        description: display.toolArgsMode === mode ? '[active]' : '',
        group: 'Args display',
      });
      return [
        {
          value: 'set:showToolReasoning',
          label: 'Reasoning ("why")',
          description: onOff(display.showToolReasoning),
          group: 'Per-tool toggles',
        },
        {
          value: 'set:showElapsed',
          label: 'Elapsed time',
          description: onOff(display.showElapsed),
          group: 'Per-tool toggles',
        },
        {
          value: 'set:showWriteDiffs:tool',
          label: 'Write diffs',
          description: onOff(display.showWriteDiffs),
          group: 'Per-tool toggles',
        },
        argModeRow('off'),
        argModeRow('inline'),
        argModeRow('block'),
      ];
    },
    // prompts/roles only emit when the master step list is on (the renderer
    // wraps both in `if (sub.pipeline && ...)`), so drop them when pipeline is
    // off — they'd be dead toggles. fullOutput piggybacks on the `subagent`
    // filter token (mirrors what `output` does for tool bars).
    subagent: () => {
      const cur = getVerboseConfig();
      const sub = (cur.display ?? DEFAULT_DISPLAY).subagent;
      const row = (key: keyof typeof sub, label: string): MenuRow => ({
        value: `set:subagent:${key}`,
        label,
        description: onOff(sub[key]),
        group: 'Subagent display',
      });
      const stepRows = sub.pipeline
        ? [
            row('prompts', 'Show step instructions'),
            row('roles', 'Show step role labels'),
          ]
        : [];
      const responseRows =
        ctx.agentEngine === 'kas'
          ? []
          : [row('responses', 'Show response summary')];
      const fullOutputOn =
        cur.filters.includes('all') || cur.filters.includes('subagent');
      return [
        row('pipeline', 'Show subagent steps'),
        ...stepRows,
        ...responseRows,
        {
          value: 'set:subagent:fullOutput',
          label: 'Show full output (verbose)',
          description: onOff(fullOutputOn),
          group: 'Subagent display',
        },
      ];
    },
    // Each row routes to the numeric editor (CommandMenu renders it when
    // previewKey ends with `:edit`).
    truncation: () => {
      const display = getVerboseDisplay();
      const rows: Array<[TruncationField, string, string]> = [
        ['argsLines', 'Args · lines', 'Tool args'],
        ['argsChars', 'Args · chars per value', 'Tool args'],
        ['outputLines', 'Output · lines', 'Tool output'],
        ['outputChars', 'Output · chars per line', 'Tool output'],
      ];
      return rows.map(([field, label, group]) => {
        const { unit, get } = TRUNC_FIELDS[field];
        return {
          value: `menu:truncation:${field}:edit`,
          label,
          description: fmtCap(get(display), unit),
          group,
        };
      });
    },
    output: () => {
      const cur = getVerboseConfig();
      const isAll = cur.filters.includes('all');
      const filterSet = new Set(cur.filters);
      // Master row label flips to read as the action it fires: when every
      // tool is on it says "none" (press clears), inverse when off.
      const masterLabel = isAll ? 'none' : 'all';
      const masterDesc = isAll
        ? '[active] · every tool · press to clear'
        : 'turn every tool on';
      return [
        {
          value: 'filter:all',
          label: masterLabel,
          description: masterDesc,
          group: 'Filter',
        },
        ...VERBOSE_CATEGORIES.map((category) => ({
          value: `category:${category}`,
          label: category,
          description: onOff(isAll || filterSet.has(category)),
          group: 'Filter',
        })),
      ];
    },
  };

  const openMenu = (key: MenuKey) => {
    // ESC returns to the top menu on this section's row (`menu:top:<key>`);
    // the trailing `← back` row encodes the same route.
    const backRoute = `menu:top:${key}`;
    setReturn(backRoute);
    openMenuWith(
      [...MENUS[key](), { value: backRoute, label: '← back', description: '' }],
      0,
      key
    );
  };

  // The single menu row is a placeholder — CommandMenu renders
  // VerbosityTruncationEditor when previewKey ends with `:edit`.
  const openTruncationEditor = (which: TruncationField) => {
    // Esc from the editor returns to the Truncation submenu, NOT the top.
    setReturn('menu:truncation');
    const display = getVerboseDisplay();
    const { heading, unit, get } = TRUNC_FIELDS[which];
    openMenuWith(
      [
        {
          value: `menu:truncation:${which}:edit`,
          label: heading,
          description: fmtCap(get(display), unit),
          group: heading,
        },
      ],
      0,
      `truncation:${which}:edit`
    );
  };

  // Toggle a single filter token, expanding the implicit `['all']` set into
  // the explicit category list first so dropping one token doesn't leave the
  // user with everything still on. Shared by the per-category rows and the
  // subagent full-output toggle (which piggybacks on the `subagent` token).
  const toggleFilterToken = (token: string) => {
    const curFilters = getVerboseConfig().filters;
    const baseline = curFilters.includes('all')
      ? Array.from(VERBOSE_CATEGORIES)
      : [...curFilters];
    const filterSet = new Set(baseline);
    if (filterSet.has(token)) filterSet.delete(token);
    else filterSet.add(token);
    setVerboseConfig({ filters: Array.from(filterSet) });
  };

  // ── Routing ────────────────────────────────────────────────────────────

  // Bare /verbosity: smart entry — density menu when a preset is active
  // (common case), else the config menu. `menu:density` / `menu:config`
  // let internal dispatch reach either one explicitly.
  if (trimmed === '' || trimmed === 'config') {
    if (trimmed === 'config') {
      openTopMenu();
    } else if (detectActivePreset() != null) {
      openDensityMenu();
    } else {
      openTopMenu();
    }
    return true;
  }
  if (trimmed === 'menu:config') {
    openTopMenu();
    return true;
  }
  // `menu:top:<key>` lands the cursor on the row for that submenu rather
  // than resetting to row 0 (used by ESC-back and the `← back` rows).
  if (trimmed.startsWith('menu:top:')) {
    const fromKey = trimmed.slice('menu:top:'.length);
    openTopMenu(fromKey);
    return true;
  }
  if (trimmed === 'menu:density') {
    openDensityMenu();
    return true;
  }
  {
    const confirmMatch = trimmed.match(/^menu:density:confirm:([a-z]+)$/);
    if (confirmMatch) {
      const preset = confirmMatch[1] as DensityPreset;
      if (!DENSITY_PRESETS.includes(preset)) {
        ctx.showAlert(`Unknown density preset: ${preset}`, 'error', 3000);
        return true;
      }
      openPresetConfirmMenu(preset);
      return true;
    }
  }
  {
    const editMatch = trimmed.match(
      /^menu:truncation:(argsLines|argsChars|outputLines|outputChars):edit$/
    );
    if (editMatch) {
      openTruncationEditor(editMatch[1] as TruncationField);
      return true;
    }
  }

  // Section-menu aliases: internal `menu:<section>` dispatch plus friendly
  // forms users type. `density` is intentionally absent — bare `density` is
  // the CLI set-preset form, and the density menu is the smart-entry default.
  {
    const MENU_ALIASES: Record<string, MenuKey> = {
      'menu:tool': 'tool',
      tool: 'tool',
      tools: 'tool',
      'tool calls': 'tool',
      'menu:subagent': 'subagent',
      subagent: 'subagent',
      subagents: 'subagent',
      'menu:output': 'output',
      output: 'output',
      'menu:truncation': 'truncation',
      truncation: 'truncation',
    };
    const key = MENU_ALIASES[trimmed];
    if (key) {
      openMenu(key);
      return true;
    }
  }

  if (trimmed === 'on') {
    setVerboseConfig({ filters: ['all'] });
    showStatus();
    return true;
  }
  if (trimmed === 'off') {
    setVerboseConfig({ filters: [] });
    showStatus();
    return true;
  }
  if (trimmed === 'status') {
    showStatus();
    return true;
  }
  if (trimmed === 'all' || trimmed === 'filter:all') {
    // CLI `all` is a one-shot reset to every-on; the menu's `filter:all`
    // row is a toggle (every-on flips to every-off, else flips to every-on).
    if (trimmed === 'all') {
      setVerboseConfig({ filters: ['all'] });
      showStatus('verbosity: filters reset');
      return true;
    }
    const cur = getVerboseConfig();
    const wasAll = cur.filters.length === 1 && cur.filters[0] === 'all';
    setVerboseConfig({ filters: wasAll ? [] : ['all'] });
    openMenu('output');
    return true;
  }
  // CLI `reset` is a power-user shortcut for the `default` preset; bypasses
  // the menu confirmation since the user opted in by typing the verb.
  if (trimmed === 'reset') {
    applyDensityPreset('default');
    ctx.announceSystem('verbosity: reset to defaults');
    openTopMenu();
    return true;
  }

  // Bare preset name (e.g. `/verbosity full`) — same path as
  // `/verbosity density <preset>` (the menu surfaces these as first-class).
  if (DENSITY_PRESETS.includes(trimmed as DensityPreset)) {
    applyDensityPreset(trimmed as DensityPreset);
    ctx.announceSystem(`verbosity: density set to ${trimmed}`);
    return true;
  }

  // CLI `density <preset>` commits immediately; menu form is
  // `density:apply:<preset>` (post-confirmation Yes); `density:<preset>` is
  // a CLI shortcut. All apply the preset's display AND filter list.
  const densityCliMatch = trimmed.match(/^density(?:\s+(.+))?$/);
  const densityMenuMatch = trimmed.match(/^density:([a-z]+)$/);
  const densityApplyMatch = trimmed.match(/^density:apply:([a-z]+)$/);
  if (densityCliMatch || densityMenuMatch || densityApplyMatch) {
    const preset = (
      densityApplyMatch?.[1] ??
      densityMenuMatch?.[1] ??
      densityCliMatch?.[1] ??
      ''
    ).trim();
    if (!preset) {
      ctx.showAlert(
        `density needs a preset: ${DENSITY_PRESETS.join(', ')}`,
        'error',
        4000
      );
      return true;
    }
    if (!DENSITY_PRESETS.includes(preset as DensityPreset)) {
      ctx.showAlert(`Unknown density preset: ${preset}`, 'error', 3000);
      return true;
    }
    applyDensityPreset(preset as DensityPreset);
    ctx.announceSystem(`verbosity: density set to ${preset}`);
    // Picking a preset is a finish action — close the overlay so the user
    // lands at the prompt. An open menu after commit read as "did that do
    // anything?". The verb form (`density <preset>`) had no open menu.
    if (densityApplyMatch || densityMenuMatch) {
      ctx.setActiveCommand(null);
      ctx.setVerboseReturnOnEscape(null);
    }
    return true;
  }

  // Display flag toggles: set:<key> flips the boolean. Args mode uses the
  // explicit `set:toolArgsMode:<value>` form because it's a 3-state.
  if (trimmed.startsWith('set:')) {
    const rest = trimmed.slice('set:'.length);
    // getVerboseDisplay so toggles compute off the unified value (cli.json
    // override), not the possibly-stale verbose-config.json copy.
    const display = getVerboseDisplay();
    const TOOL_BOOL_TOGGLES: Record<
      string,
      'showToolReasoning' | 'showElapsed' | 'showWriteDiffs'
    > = {
      showToolReasoning: 'showToolReasoning',
      showElapsed: 'showElapsed',
      'showWriteDiffs:tool': 'showWriteDiffs',
    };
    const toolToggleField = TOOL_BOOL_TOGGLES[rest];
    if (toolToggleField) {
      setVerboseConfig({
        display: { ...display, [toolToggleField]: !display[toolToggleField] },
      });
      openMenu('tool');
      return true;
    }
    if (rest === 'showThinkingContent') {
      const newVal = !display.showThinkingContent;
      setVerboseConfig({
        display: {
          ...display,
          showThinkingContent: newVal,
        },
      });
      // Sync to ACP/Rust side; best-effort (lite toggle stands if RPC fails).
      ctx.kiro.setSetting(Settings.CHAT_SHOW_THINKING, newVal).catch(() => {});
      openTopMenu('thinking');
      return true;
    }
    if (rest === 'showTasks') {
      setVerboseConfig({
        display: { ...display, showTasks: !display.showTasks },
      });
      openTopMenu('tasks');
      return true;
    }
    const argMatch = rest.match(/^toolArgsMode:(off|inline|block)$/);
    if (argMatch) {
      const mode = argMatch[1] as ToolArgsMode;
      setVerboseConfig({ display: { ...display, toolArgsMode: mode } });
      openMenu('tool');
      return true;
    }
    // Truncation cap setters: `set:<field>:<value>` where value is `null`
    // (unlimited) or a positive integer. Non-positive/non-numeric values
    // collapse to `null` so a corrupt value can't truncate everything to 0.
    const capMatch = rest.match(
      /^(argsMaxLines|outputMaxLines|argsMaxChars|outputMaxChars):(null|\d+)$/
    );
    if (capMatch) {
      const field = capMatch[1] as
        | 'argsMaxLines'
        | 'outputMaxLines'
        | 'argsMaxChars'
        | 'outputMaxChars';
      const raw = capMatch[2]!;
      let value: number | null;
      if (raw === 'null') {
        value = null;
      } else {
        const n = parseInt(raw, 10);
        // Non-positive values are not user-selectable from the menu, but
        // guard for them on CLI typed input.
        value = Number.isFinite(n) && n > 0 ? n : null;
      }
      setVerboseConfig({ display: { ...display, [field]: value } });
      // Return to the truncation submenu so the user sees the updated cap.
      openMenu('truncation');
      return true;
    }
    // `deps` is CLI-only for now: it is persisted and used by presets, but
    // hidden from the menu because dependency labels are nested under steps.
    const subMatch = rest.match(
      /^subagent:(pipeline|prompts|roles|deps|responses)$/
    );
    if (subMatch) {
      const key = subMatch[1] as keyof typeof display.subagent;
      if (ctx.agentEngine === 'kas' && key === 'responses') {
        ctx.showAlert(
          'KAS subagents provide responses, not response summaries',
          'warning',
          3000
        );
        openMenu('subagent');
        return true;
      }
      const cur = display.subagent[key];
      setVerboseConfig({
        display: {
          ...display,
          subagent: { ...display.subagent, [key]: !cur },
        },
      });
      openMenu('subagent');
      return true;
    }
    // fullOutput piggybacks on the `subagent` filter token (same gate the
    // renderer uses for the verbose `full output:` body).
    if (rest === 'subagent:fullOutput') {
      toggleFilterToken('subagent');
      openMenu('subagent');
      return true;
    }
    ctx.showAlert(`Unknown toggle: ${rest}`, 'error', 3000);
    return true;
  }

  // Multi-token subcommands: only/add/remove. The first word is the verb,
  // remaining whitespace-separated tokens are the filter list. We validate
  // tokens up front so typos surface as a warning instead of silently
  // landing in the saved config.
  const subMatch = trimmed.match(/^(only|add|remove)\b\s*(.*)$/);
  if (subMatch) {
    const verb = subMatch[1] as 'only' | 'add' | 'remove';
    const rest = (subMatch[2] ?? '').trim();
    if (!rest) {
      ctx.showAlert(
        `/verbosity ${verb} needs at least one filter token`,
        'error',
        3000
      );
      return true;
    }
    const tokens = rest.split(/\s+/);
    const { accepted, rejected, unknown } = validateTokens(tokens);
    if (accepted.length === 0) {
      ctx.showAlert(
        `No valid tokens in: ${rejected.join(', ')}`,
        'error',
        4000
      );
      return true;
    }
    const current = cfg.filters.includes('all') ? [] : [...cfg.filters];
    let nextFilters: string[];
    if (verb === 'only') {
      nextFilters = accepted;
    } else if (verb === 'add') {
      const set = new Set(current);
      for (const t of accepted) set.add(t);
      nextFilters = Array.from(set);
    } else {
      const drop = new Set(accepted);
      nextFilters = current.filter((t) => !drop.has(t));
    }
    setVerboseConfig({ filters: nextFilters });
    const tail =
      rejected.length > 0 ? ` (ignored: ${rejected.join(', ')})` : '';
    // Soft-warn on unknown tokens (typos, unrecognized categories) on the
    // remove path too — a user removing a misspelled tool name should
    // know the input didn't match anything saved.
    const warn =
      unknown.length > 0
        ? ` · warning: ${unknown.join(', ')} ${unknown.length === 1 ? `doesn't` : `don't`} match any known tool or category`
        : '';
    showStatus(`verbosity: filters updated${tail}${warn}`);
    return true;
  }

  if (trimmed.startsWith('category:')) {
    const cat = trimmed.slice('category:'.length);
    if (!VERBOSE_CATEGORIES.includes(cat as any)) {
      ctx.showAlert(`Unknown category: ${cat}`, 'error', 3000);
      return true;
    }
    toggleFilterToken(cat);
    // Re-open the output sub-menu so the user can keep toggling categories.
    openMenu('output');
    return true;
  }

  ctx.showAlert(
    `Unknown /verbosity subcommand: ${trimmed}. Try /verbosity, /verbosity on|off|status|all, /verbosity density <preset>, /verbosity only|add|remove <list>.`,
    'error',
    6000
  );
  return true;
}
