import React, { useState } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { Panel } from './panel/Panel.js';
import { Menu } from './menu/Menu.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { PromptInput } from '../chat/prompt-bar/PromptInput.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useApprovalState, useConversationState } from '../../stores/selectors';
import {
  type PermissionOption,
  type TrustOption,
  type ConsentContext,
} from '../../types/agent-events';
import { MessageRole, useAppStore } from '../../stores/app-store.js';
import { deriveShellTrustOptions } from '../../utils/shell-trust-options.js';

interface ApprovalRequestProps {
  onDrillInSubmit: (value: string) => void;
}

const TRUST_ENTRY_ID = 'allow_always';

export const ApprovalRequest: React.FC<ApprovalRequestProps> = ({
  onDrillInSubmit,
}) => {
  const {
    pendingApproval,
    approvalMode: mode,
    respondToApproval,
    cancelApproval,
    setApprovalMode,
    sessionId: mainSessionId,
    sessions,
  } = useApprovalState();
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const { messages } = useConversationState();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  const approvalSessionId = pendingApproval?.sessionId;
  const subagentName =
    approvalSessionId && approvalSessionId !== mainSessionId
      ? sessions.get(approvalSessionId)?.name
      : undefined;

  const [focusedIndex, setFocusedIndex] = useState(0);
  const [page, setPage] = useState<'default' | 'trust' | 'kas-scope'>(
    'default'
  );

  const options: PermissionOption[] = pendingApproval
    ? pendingApproval.permissionOptions
    : [];
  const trustOptions: TrustOption[] = pendingApproval?.trustOptions ?? [];
  const consentContext: ConsentContext | undefined =
    pendingApproval?.consentContext;
  const agentEngine = useAppStore((s) => s.agentEngine);
  const hasTrustPage = trustOptions.length > 0;
  const hasKasScopePage = !hasTrustPage && agentEngine === 'kas';
  // The wire optionId for the allow_always option (e.g. 'always-accept' from KAS)
  const TRUST_OPTION_ID =
    options.find((o) => o.kind === TRUST_ENTRY_ID)?.optionId ?? 'always-accept';

  const optionLabels: Record<string, string> = {
    allow_once: 'Yes, single permission',
    allow_always: 'Trust, always allow in this session',
    allow_all_session: 'Trust, allow all for this session',
    reject_once: 'No (Tab to edit)',
  };

  const defaultMenuItems = options.map((opt) => ({
    label: optionLabels[opt.optionId] ?? opt.name,
    description: '',
  }));

  const ENTIRE_TOOL_LABEL = 'Entire tool';
  const trustMenuItems = [
    ...trustOptions.map((t) => ({
      label: t.label,
      description: t.display,
    })),
    { label: ENTIRE_TOOL_LABEL, description: '' },
  ];

  const resource = consentContext?.resource;
  const capability = consentContext?.capability;
  const toolName2 = (() => {
    const msg = messages.find(
      (m) =>
        m.role === MessageRole.ToolUse &&
        m.id === pendingApproval?.toolCall.toolCallId
    );
    return msg && msg.role === MessageRole.ToolUse ? msg.name : undefined;
  })();

  // Compound shell commands (e.g. "git status && echo done") are gated by the
  // agent one sub-command at a time: the whole command arrives as `resource`,
  // but the segment requiring consent right now is `triggeringResource`. Trust
  // options must target THAT segment — otherwise neither the whole-command
  // exact match nor the "git *" pattern can authorize a later segment like
  // `echo done`, so the policy re-asks it forever. `gatedResource` falls back
  // to `resource` for single / non-compound requests.
  const { gatedResource, exactResource, patternResource } =
    deriveShellTrustOptions({
      capability,
      resource,
      triggeringResource: consentContext?.triggeringResource,
    });
  // The pattern (e.g. "echo *") offered for the gated sub-command.
  const baseCommand = patternResource;
  const resourceLabel = gatedResource
    ? gatedResource.length > 50
      ? `"${gatedResource.slice(0, 47)}…"`
      : `"${gatedResource}"`
    : undefined;

  const [trustScope, setTrustScope] = useState<
    'session' | 'workspace' | 'global'
  >('session');
  const scopeLabels = {
    session: 'session',
    workspace: 'workspace',
    global: 'always',
  };

  const kasScopeItems = [
    ...(resourceLabel
      ? [
          {
            label: `Trust ${resourceLabel}`,
            description: `exact match · ${scopeLabels[trustScope]}`,
          },
        ]
      : []),
    ...(baseCommand && baseCommand !== gatedResource
      ? [
          {
            label: `Trust "${baseCommand}"`,
            description: `pattern · ${scopeLabels[trustScope]}`,
          },
        ]
      : []),
    {
      label: `Trust entire tool${toolName2 ? ` (${toolName2})` : ''}`,
      description: scopeLabels[trustScope],
    },
  ];

  const menuItems =
    page === 'trust'
      ? trustMenuItems
      : page === 'kas-scope'
        ? kasScopeItems
        : defaultMenuItems;

  const focusedOnTrust =
    page === 'default' &&
    options[focusedIndex]?.kind === TRUST_ENTRY_ID &&
    (hasTrustPage || hasKasScopePage);

  // Right arrow → drill-in, Left arrow → back (same as Esc)
  useKeypress((input, key) => {
    if (!pendingApproval) return;
    if (key.rightArrow && mode === 'dropdown') {
      setApprovalMode('drill-in');
    } else if (key.leftArrow) {
      if (mode === 'drill-in') {
        setApprovalMode('dropdown');
      } else if (
        mode === 'dropdown' &&
        (page === 'trust' || page === 'kas-scope')
      ) {
        setPage('default');
        setFocusedIndex(0);
      } else if (mode === 'dropdown') {
        cancelApproval();
      }
    } else if (input === 's' && page === 'kas-scope') {
      setTrustScope((prev) =>
        prev === 'session'
          ? 'workspace'
          : prev === 'workspace'
            ? 'global'
            : 'session'
      );
    }
  });

  if (!pendingApproval) return null;

  const toolMsg = messages.find(
    (m) =>
      m.role === MessageRole.ToolUse &&
      m.id === pendingApproval.toolCall.toolCallId
  );
  const toolName =
    toolMsg && toolMsg.role === MessageRole.ToolUse
      ? toolMsg.name
      : (pendingApproval.toolCall.title ?? 'Tool');

  const prefix = subagentName ? `${subagentName} > ` : '';
  // Some permission requests aren't tool approvals — notably user_input
  // questions (e.g. spec design questions), which reuse the requestPermission
  // channel but carry no tool. KAS stamps `_meta.kiro.toolId` on real tool
  // approvals and omits it for questions, so a missing toolId means "question":
  // render the (markdown) question text as the body and title it "Question"
  // rather than "<title> requires approval".
  const isQuestion = agentEngine === 'kas' && !pendingApproval.toolId;
  const questionText = pendingApproval.toolCall.title ?? '';
  const title = isQuestion
    ? `${prefix}Question`
    : mode === 'drill-in'
      ? `${prefix}${toolName} requires approval · Modify request`
      : page === 'trust'
        ? `${prefix}${toolName} requires approval · trust options`
        : page === 'kas-scope'
          ? `${prefix}${toolName} requires approval · trust [${scopeLabels[trustScope]}] (s to cycle)`
          : `${prefix}${toolName} requires approval`;

  const handleClose = () => {
    if (mode === 'drill-in') {
      setApprovalMode('dropdown');
    } else if (page === 'trust' || page === 'kas-scope') {
      setPage('default');
      setFocusedIndex(0);
    } else {
      // Top-level Esc/leftArrow: interrupt the agent in addition to
      // cancelling this approval. cancelMessage() calls cancelApproval()
      // internally, so this also clears the pending approval and any
      // queued ones — and aborts the agent's current turn so the user
      // gets the prompt back to type a new instruction.
      cancelMessage();
    }
  };

  const handleTabSwitch = () => {
    if (mode === 'dropdown') {
      setApprovalMode('drill-in');
    } else {
      setApprovalMode('dropdown');
    }
  };

  const handleSelect = (item: { label: string }) => {
    if (page === 'default') {
      const opt = options.find(
        (o) => (optionLabels[o.optionId] ?? o.name) === item.label
      );
      if (opt?.kind === TRUST_ENTRY_ID && hasTrustPage) {
        setPage('trust');
        setFocusedIndex(0);
        return;
      }
      if (opt?.kind === TRUST_ENTRY_ID && hasKasScopePage) {
        setPage('kas-scope');
        setFocusedIndex(0);
        return;
      }
      if (opt) respondToApproval(opt.optionId);
    } else if (page === 'kas-scope') {
      const scopeValue = trustScope === 'global' ? 'user' : trustScope;
      if (resourceLabel && item.label === `Trust ${resourceLabel}`) {
        respondToApproval(TRUST_OPTION_ID, undefined, {
          kasScope: scopeValue,
          kasResource: exactResource,
        });
      } else if (baseCommand && item.label === `Trust "${baseCommand}"`) {
        respondToApproval(TRUST_OPTION_ID, undefined, {
          kasScope: scopeValue,
          kasResource: baseCommand,
        });
      } else {
        // Entire tool — no resource filter
        respondToApproval(TRUST_OPTION_ID, undefined, { kasScope: scopeValue });
      }
    } else {
      if (item.label === ENTIRE_TOOL_LABEL) {
        respondToApproval('allow_always');
        return;
      }
      const selected = trustOptions.find((t) => t.label === item.label);
      if (selected) {
        respondToApproval('allow_always', undefined, { trustOption: selected });
      }
    }
  };

  let footerLeft: React.ReactNode | undefined;
  if (isQuestion) {
    // A user_input question is multiple-choice only — no edit affordance — so
    // the single (outer) footer carries just the navigate/select hints.
    footerLeft = (
      <Text>
        {primary('↑↓')} {secondary('to navigate')}
        {secondary(' · ')}
        {primary('↵')} {secondary('to select')}
      </Text>
    );
  } else if (mode === 'dropdown' && focusedOnTrust) {
    footerLeft = (
      <Text>
        {primary('Enter')} {secondary('to see more options')}
      </Text>
    );
  } else if (mode === 'dropdown') {
    footerLeft = (
      <Text>
        {primary('↑↓')} {secondary('to navigate')}
        {secondary(' · ')}
        {primary('↵')} {secondary('to select')}
        {secondary(' · ')}
        {primary('Tab')} {secondary('to edit')}
      </Text>
    );
  }

  return (
    <Panel
      title={title}
      onClose={handleClose}
      onTabSwitch={
        !isQuestion && mode === 'dropdown' ? handleTabSwitch : undefined
      }
      showTabHint={false}
      hideTitleDivider={true}
      footerLeft={footerLeft}
      closeHintLabel={isQuestion ? 'to cancel' : undefined}
    >
      <Box flexDirection="column">
        {isQuestion && questionText && (
          <Box marginBottom={1}>
            <MarkdownRenderer content={questionText} color={primary} />
          </Box>
        )}
        {consentContext &&
          (consentContext.capability || consentContext.resource) && (
            <Box marginBottom={1}>
              <Text>
                {secondary(
                  `${consentContext.capability ?? ''}${consentContext.capability && consentContext.resource ? ` ${glyphs.arrow} ` : ''}${consentContext.resource ?? ''}`
                )}
              </Text>
            </Box>
          )}
        {mode === 'dropdown' && (
          <Menu
            key={page}
            items={menuItems}
            onSelect={handleSelect}
            onHighlight={(item) => {
              const idx = menuItems.findIndex((m) => m.label === item.label);
              if (idx >= 0) setFocusedIndex(idx);
            }}
            showSelectedIndicator={true}
          />
        )}
        {mode === 'drill-in' && (
          <PromptInput
            onSubmit={onDrillInSubmit}
            isProcessing={false}
            placeholder="add your feedback..."
          />
        )}
      </Box>
    </Panel>
  );
};
