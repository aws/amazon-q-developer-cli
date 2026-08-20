import { getActiveGlyphs } from '../hooks/useGlyphs.js';
import type { CommandContext } from './types.js';
import type { AvailableCommand, CommandResult } from '../types/commands.js';
import {
  getVerboseDisplay,
  getVerboseFilters,
  getTuiVerboseDisplay,
  getTuiVerboseFilters,
  setVerboseConfig,
  validateTokens,
  VERBOSE_CATEGORIES,
  expandFilterBaseline,
  applyDensityPreset,
  shouldShowToolOutput,
  sameDisplay,
  DENSITY_PRESETS,
  getDensityPresetDisplay,
  getDensityPresetFilters,
  DEFAULT_DISPLAY,
  sameFilters,
  type ToolArgsMode,
  type ThinkingDisplayMode,
  type DensityPreset,
} from '../lite/verbose.js';

type TruncationField =
  | 'argsLines'
  | 'argsChars'
  | 'outputLines'
  | 'outputChars';

// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'handleVerbosity' has a complexity of 79. Maximum allowed is 30.; refactor before extending
// LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 113 to the 30 allowed.; refactor before extending
// eslint-disable-next-line complexity, sonarjs/cognitive-complexity
export function handleVerbosity(
  _result: CommandResult | null,
  ctx: CommandContext,
  cmd: AvailableCommand,
  args: string
): boolean {
  if (
    ctx.getUiMode?.() !== 'lite' &&
    process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1'
  ) {
    ctx.showAlert('/verbosity is not available in this build', 'error', 3000);
    return true;
  }

  const glyphs = getActiveGlyphs();

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

  const isTuiMode = ctx.getUiMode?.() !== 'lite';
  const surface = isTuiMode ? 'tui' : 'lite';
  const getDisplay = isTuiMode ? getTuiVerboseDisplay : getVerboseDisplay;
  const getFilters = isTuiMode ? getTuiVerboseFilters : getVerboseFilters;
  const updateConfig = (patch: Parameters<typeof setVerboseConfig>[0]) =>
    setVerboseConfig(patch, surface);
  const argsSummary = (mode: ToolArgsMode): string =>
    isTuiMode ? (mode === 'off' ? 'off' : 'on') : mode;

  // `null` when the saved shape matches no preset (hand-toggled custom).
  // Match both display and filters so hand-toggled configurations stay custom.
  const detectActivePreset = (): DensityPreset | null => {
    const cur = getDisplay();
    const curFilters = getFilters();
    for (const preset of DENSITY_PRESETS) {
      if (!sameDisplay(cur, getDensityPresetDisplay(preset, surface))) continue;
      if (!sameFilters(curFilters, getDensityPresetFilters(preset, surface)))
        continue;
      return preset;
    }
    return null;
  };

  const fmtFilters = (f: string[]) => {
    if (f.length === 0) return 'none';
    if (f.length === 1 && f[0] === 'all') return 'all';
    return f.join(', ');
  };
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
  const showStatus = (msg?: string) => {
    const curFilters = getFilters();
    if (msg) {
      ctx.announceSystem(
        `${msg} ${glyphs.smallDot} filters: ${fmtFiltersForAnnounce(curFilters)}`
      );
      return;
    }
    const density = detectActivePreset() ?? 'custom';
    ctx.announceSystem(
      `verbosity ${glyphs.smallDot} filters: ${fmtFiltersForAnnounce(curFilters)} ${glyphs.smallDot} density: ${density}`
    );
  };

  const fmtCap = (
    cap: number | null,
    unit: 'lines' | 'chars' = 'lines'
  ): string => (cap == null || cap <= 0 ? 'unlimited' : `${cap} ${unit}`);

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

  // Keep in sync with openTopMenu option order for ESC-back focus.
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
    const display = getDisplay();
    const curFilters = getFilters();
    const preset = detectActivePreset();
    const presetLabel = preset ?? 'custom';
    const toolSummary = `args: ${argsSummary(display.toolArgsMode)} ${glyphs.smallDot} reasoning: ${display.showToolReasoning ? 'on' : 'off'} ${glyphs.smallDot} elapsed: ${display.showElapsed ? 'on' : 'off'}`;
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
    const fullOutputOn = shouldShowToolOutput('subagent', curFilters);
    if (fullOutputOn) subSummaryParts.push('full output');
    const subSummary =
      subSummaryParts.length === 0
        ? '(all hidden)'
        : subSummaryParts.join(` ${glyphs.smallDot} `);
    const outSummary = fmtFilters(curFilters);
    const truncSummary = `args ${fmtCap(display.argsMaxLines)}/${fmtCap(display.argsMaxChars, 'chars')} ${glyphs.smallDot} output ${fmtCap(display.outputMaxLines)}/${fmtCap(display.outputMaxChars, 'chars')}`;

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
        // Lite renders thinking as a single block (no collapsed/expanded
        // distinction), so it gets a plain on/off toggle; only the TUI cycles.
        isTuiMode
          ? {
              value: 'set:thinkingDisplay:cycle',
              label: 'Thinking',
              description: `[${display.thinkingDisplay}] ${glyphs.smallDot} expanded/collapsed/off`,
              group: 'Sections',
            }
          : {
              value: 'set:thinkingDisplay:toggle',
              label: 'Thinking',
              description: onOff(display.thinkingDisplay !== 'off'),
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

  const PRESET_DESC: Record<DensityPreset, string> = {
    lean: 'inline arg chip, no reasoning, full elapsed',
    default: `reasoning + block args, subagent detail ${isTuiMode ? 'off' : 'on'}`,
    full: `1:1 of what the parent agent sees ${glyphs.smallDot} all filters on ${glyphs.smallDot} no truncation`,
  };

  const openDensityMenu = (initialIndex = 0) => {
    setReturn(null);
    const active = detectActivePreset();
    const options: Array<{
      value: string;
      label: string;
      description: string;
    }> = DENSITY_PRESETS.map((p) => ({
      value: `density:apply:${p}`,
      label: p,
      description:
        active === p
          ? `[active] ${glyphs.smallDot} ${PRESET_DESC[p]}`
          : PRESET_DESC[p],
    }));
    options.push({
      value: 'menu:config',
      label: 'custom',
      description:
        active == null
          ? `[active] ${glyphs.smallDot} tweak individual settings`
          : 'tweak individual settings',
    });
    openMenuWith(options, initialIndex, 'density');
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
      heading: `Tool args ${glyphs.smallDot} lines`,
      unit: 'lines',
      get: (d) => d.argsMaxLines,
    },
    argsChars: {
      heading: `Tool args ${glyphs.smallDot} chars`,
      unit: 'chars',
      get: (d) => d.argsMaxChars,
    },
    outputLines: {
      heading: `Tool output ${glyphs.smallDot} lines`,
      unit: 'lines',
      get: (d) => d.outputMaxLines,
    },
    outputChars: {
      heading: `Tool output ${glyphs.smallDot} chars per line`,
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
  type MenuKey = 'tool' | 'subagent' | 'truncation' | 'output';
  const MENUS: Record<MenuKey, () => MenuRow[]> = {
    tool: () => {
      const display = getDisplay();
      const toggle = (value: string, label: string, on: boolean): MenuRow => ({
        value,
        label,
        description: onOff(on),
        group: 'Per-tool toggles',
      });
      const argModeRow = (mode: ToolArgsMode): MenuRow => ({
        value: `set:toolArgsMode:${mode}`,
        label: `Args: ${mode}`,
        description: display.toolArgsMode === mode ? '[active]' : '',
        group: 'Args display',
      });
      const argsOn = display.toolArgsMode !== 'off';
      const argsRows: MenuRow[] = isTuiMode
        ? [
            {
              value: 'set:toolArgsMode:toggle',
              label: 'Tool args',
              description: onOff(argsOn),
              group: 'Args display',
            },
          ]
        : [argModeRow('off'), argModeRow('inline'), argModeRow('block')];
      return [
        toggle(
          'set:showToolReasoning',
          'Reasoning ("why")',
          display.showToolReasoning
        ),
        toggle('set:showElapsed', 'Elapsed time', display.showElapsed),
        toggle(
          'set:showWriteDiffs:tool',
          'Write diffs',
          display.showWriteDiffs
        ),
        // "Keep output in scrollback" only governs the TUI's expandable tool
        // bodies; lite always persists its (capped) output, so hide the toggle.
        ...(isTuiMode
          ? [
              toggle(
                'set:persistOutput',
                'Keep output in scrollback',
                display.persistOutput
              ),
            ]
          : []),
        ...argsRows,
      ];
    },
    subagent: () => {
      const curFilters = getFilters();
      const sub = getDisplay().subagent;
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
      const fullOutputOn = shouldShowToolOutput('subagent', curFilters);
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
    truncation: () => {
      const display = getDisplay();
      const rows: Array<[TruncationField, string, string]> = [
        ['argsLines', `Args ${glyphs.smallDot} lines`, 'Tool args'],
        ['argsChars', `Args ${glyphs.smallDot} chars per value`, 'Tool args'],
        ['outputLines', `Output ${glyphs.smallDot} lines`, 'Tool output'],
        [
          'outputChars',
          `Output ${glyphs.smallDot} chars per line`,
          'Tool output',
        ],
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
      const curFilters = getFilters();
      const isAll =
        curFilters.includes('all') &&
        !curFilters.some((token) => token.startsWith('-'));
      const masterLabel = isAll ? 'none' : 'all';
      const masterDesc = isAll
        ? `[active] ${glyphs.smallDot} every tool ${glyphs.smallDot} press to clear`
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
          description: onOff(shouldShowToolOutput(category, curFilters)),
          group: 'Filter',
        })),
      ];
    },
  };

  const openMenu = (key: MenuKey) => {
    const backRoute = `menu:top:${key}`;
    setReturn(backRoute);
    openMenuWith(
      [
        ...MENUS[key](),
        {
          value: backRoute,
          label: `${glyphs.arrowLeft} back`,
          description: '',
        },
      ],
      0,
      key
    );
  };

  const openTruncationEditor = (which: TruncationField) => {
    setReturn('menu:truncation');
    const display = getDisplay();
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

  const toggleFilterToken = (token: string) => {
    const current = getFilters();
    if (
      current.includes('all') &&
      current.some((value) => value.startsWith('-'))
    ) {
      const exclusion = `-${token}`;
      const next = current.filter((value) => value !== exclusion);
      if (next.length === current.length) next.push(exclusion);
      updateConfig({ filters: next });
      return;
    }
    const filterSet = new Set(expandFilterBaseline(current));
    if (filterSet.has(token)) filterSet.delete(token);
    else filterSet.add(token);
    updateConfig({ filters: Array.from(filterSet) });
  };

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
    const editMatch = trimmed.match(
      /^menu:truncation:(argsLines|argsChars|outputLines|outputChars):edit$/
    );
    if (editMatch) {
      openTruncationEditor(editMatch[1] as TruncationField);
      return true;
    }
  }

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
    updateConfig({ filters: ['all'] });
    showStatus();
    return true;
  }
  if (trimmed === 'off') {
    updateConfig({ filters: [] });
    showStatus();
    return true;
  }
  if (trimmed === 'status') {
    showStatus();
    return true;
  }
  if (trimmed === 'all' || trimmed === 'filter:all') {
    if (trimmed === 'all') {
      updateConfig({ filters: ['all'] });
      showStatus('verbosity: filters reset');
      return true;
    }
    const curFilters = getFilters();
    const wasAll = curFilters.length === 1 && curFilters[0] === 'all';
    updateConfig({ filters: wasAll ? [] : ['all'] });
    openMenu('output');
    return true;
  }
  if (trimmed === 'reset') {
    applyDensityPreset('default', surface);
    ctx.announceSystem('verbosity: reset to defaults');
    openTopMenu();
    return true;
  }

  if (DENSITY_PRESETS.includes(trimmed as DensityPreset)) {
    applyDensityPreset(trimmed as DensityPreset, surface);
    ctx.announceSystem(`verbosity: density set to ${trimmed}`);
    return true;
  }

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
    applyDensityPreset(preset as DensityPreset, surface);
    ctx.announceSystem(`verbosity: density set to ${preset}`);
    if (densityApplyMatch || densityMenuMatch) {
      ctx.setActiveCommand(null);
      ctx.setVerboseReturnOnEscape(null);
    }
    return true;
  }

  if (trimmed.startsWith('set:')) {
    const rest = trimmed.slice('set:'.length);
    const display = getDisplay();
    const TOOL_BOOL_TOGGLES: Record<
      string,
      'showToolReasoning' | 'showElapsed' | 'showWriteDiffs' | 'persistOutput'
    > = {
      showToolReasoning: 'showToolReasoning',
      showElapsed: 'showElapsed',
      'showWriteDiffs:tool': 'showWriteDiffs',
      persistOutput: 'persistOutput',
    };
    const toolToggleField = TOOL_BOOL_TOGGLES[rest];
    if (toolToggleField) {
      updateConfig({
        display: { [toolToggleField]: !display[toolToggleField] },
      });
      openMenu('tool');
      return true;
    }
    const thinkingMatch = rest.match(
      /^thinkingDisplay:(cycle|toggle|off|collapsed|expanded)$/
    );
    if (thinkingMatch) {
      const arg = thinkingMatch[1]!;
      const order: ThinkingDisplayMode[] = ['expanded', 'collapsed', 'off'];
      let next: ThinkingDisplayMode;
      if (arg === 'cycle') {
        next =
          order[(order.indexOf(display.thinkingDisplay) + 1) % order.length]!;
      } else if (arg === 'toggle') {
        // Lite's binary switch: off ↔ expanded (its only rendered states).
        next = display.thinkingDisplay === 'off' ? 'expanded' : 'off';
      } else {
        next = arg as ThinkingDisplayMode;
      }
      updateConfig({ display: { thinkingDisplay: next } });
      openTopMenu('thinking');
      return true;
    }
    if (rest === 'showTasks') {
      updateConfig({ display: { showTasks: !display.showTasks } });
      openTopMenu('tasks');
      return true;
    }
    if (rest === 'toolArgsMode:toggle') {
      const next: ToolArgsMode =
        display.toolArgsMode === 'off' ? 'inline' : 'off';
      updateConfig({ display: { toolArgsMode: next } });
      openMenu('tool');
      return true;
    }
    const argMatch = rest.match(/^toolArgsMode:(off|inline|block)$/);
    if (argMatch) {
      const mode = argMatch[1] as ToolArgsMode;
      updateConfig({ display: { toolArgsMode: mode } });
      openMenu('tool');
      return true;
    }
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
        value = Number.isFinite(n) && n > 0 ? n : null;
      }
      updateConfig({ display: { [field]: value } });
      openMenu('truncation');
      return true;
    }
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
      updateConfig({ display: { subagent: { [key]: !cur } } });
      openMenu('subagent');
      return true;
    }
    if (rest === 'subagent:fullOutput') {
      toggleFilterToken('subagent');
      openMenu('subagent');
      return true;
    }
    ctx.showAlert(`Unknown toggle: ${rest}`, 'error', 3000);
    return true;
  }

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
    const savedFilters = getFilters();
    const current = expandFilterBaseline(savedFilters);
    let nextFilters: string[];
    if (verb === 'only') {
      nextFilters = accepted;
    } else if (
      savedFilters.includes('all') &&
      savedFilters.some((token) => token.startsWith('-'))
    ) {
      const set = new Set(savedFilters);
      for (const token of accepted) {
        const exclusion = `-${token}`;
        if (verb === 'add') set.delete(exclusion);
        else set.add(exclusion);
      }
      nextFilters = Array.from(set);
    } else if (verb === 'add') {
      const set = new Set(current);
      for (const t of accepted) set.add(t);
      nextFilters = Array.from(set);
    } else {
      const drop = new Set(accepted);
      nextFilters = current.filter((t) => !drop.has(t));
    }
    updateConfig({ filters: nextFilters });
    const tail =
      rejected.length > 0 ? ` (ignored: ${rejected.join(', ')})` : '';
    const warn =
      unknown.length > 0
        ? ` ${glyphs.smallDot} warning: ${unknown.join(', ')} ${unknown.length === 1 ? `doesn't` : `don't`} match any known tool or category`
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
