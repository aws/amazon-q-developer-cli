import React, { useMemo, useState } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { Panel } from '../../ui/panel/Panel.js';
import { Menu } from '../../ui/menu/Menu.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { MessageRole, useAppStore } from '../../../stores/app-store.js';
import { useSessionConversation } from '../../../stores/session-conversations.js';
import type {
  ApprovalRequestInfo,
  ConsentContext,
  PermissionOption,
  TrustOption,
} from '../../../types/agent-events.js';
import { deriveShellTrustOptions } from '../../../utils/shell-trust-options.js';

const TRUST_ENTRY_ID = 'allow_always';

// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'ApprovalPanel' has a complexity of 34. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
export const ApprovalPanel = React.memo(function ApprovalPanel({
  approval,
  width,
}: {
  approval: ApprovalRequestInfo;
  width?: number;
}) {
  const respondToApproval = useAppStore((state) => state.respondToApproval);
  const agentEngine = useAppStore((state) => state.agentEngine);
  const conversationMessages = useSessionConversation(approval.sessionId ?? '');
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  const [page, setPage] = useState<'default' | 'trust' | 'kas-scope'>(
    'default'
  );
  const [focusedIndex, setFocusedIndex] = useState(0);
  const [trustScope, setTrustScope] = useState<
    'session' | 'workspace' | 'global'
  >('session');

  const toolName = useMemo(() => {
    const toolMsg = conversationMessages.find(
      (m) =>
        m.role === MessageRole.ToolUse && m.id === approval.toolCall.toolCallId
    );
    return toolMsg && toolMsg.role === MessageRole.ToolUse
      ? toolMsg.name
      : 'Tool';
  }, [approval.toolCall.toolCallId, conversationMessages]);

  const options = approval.permissionOptions;
  const trustOptions: TrustOption[] = approval.trustOptions ?? [];
  const consentContext: ConsentContext | undefined = approval.consentContext;
  const hasTrustPage = trustOptions.length > 0;
  const alwaysOpt = options.find(
    (o) => o.kind === TRUST_ENTRY_ID || o.optionId === TRUST_ENTRY_ID
  );
  const hasAllowAlways = !!alwaysOpt;

  const { gatedResource, exactResource, patternResource } =
    deriveShellTrustOptions({
      capability: consentContext?.capability,
      resource: consentContext?.resource,
      triggeringResource: consentContext?.triggeringResource,
    });
  const scopeLabels = {
    session: 'session',
    workspace: 'workspace',
    global: 'always',
  };
  const resourceLabel = gatedResource
    ? gatedResource.length > 50
      ? `"${gatedResource.slice(0, 47)}..."`
      : `"${gatedResource}"`
    : undefined;
  const kasScopeItems = [
    ...(resourceLabel && exactResource
      ? [
          {
            label: `Trust ${resourceLabel}`,
            description: `exact match ${glyphs.smallDot} ${scopeLabels[trustScope]}`,
            resource: exactResource,
          },
        ]
      : []),
    ...(patternResource && patternResource !== gatedResource
      ? [
          {
            label: `Trust "${patternResource}"`,
            description: `pattern ${glyphs.smallDot} ${scopeLabels[trustScope]}`,
            resource: patternResource,
          },
        ]
      : []),
    {
      label: `Trust entire tool${toolName ? ` (${toolName})` : ''}`,
      description: scopeLabels[trustScope],
      wholeCapability: true,
    },
  ];
  const hasKasScopePage =
    hasAllowAlways &&
    !hasTrustPage &&
    agentEngine === 'kas' &&
    !!consentContext &&
    kasScopeItems.length > 0;

  const sortedOptions = useMemo(() => {
    const order: Record<string, number> = {
      allow_once: 0,
      allow_always: 1,
      allow_all_session: 2,
      reject_always: 3,
      reject_once: 4,
    };
    return [...options].sort(
      (a, b) =>
        (order[a.kind] ?? order[a.optionId] ?? 3) -
        (order[b.kind] ?? order[b.optionId] ?? 3)
    );
  }, [options]);

  const optionLabels: Record<string, string> = {
    allow_once: 'Yes, single permission',
    allow_always: 'Trust, always allow in this session',
    allow_all_session: 'Trust, allow all for this session',
    reject_once: 'No',
    reject_always: 'Never',
  };

  const optionLabel = (opt: PermissionOption) =>
    optionLabels[opt.kind] ?? optionLabels[opt.optionId] ?? opt.name;

  const defaultMenuItems = sortedOptions.map((opt) => ({
    label: optionLabel(opt),
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

  const menuItems =
    page === 'trust'
      ? trustMenuItems
      : page === 'kas-scope'
        ? kasScopeItems
        : defaultMenuItems;
  const focusedOnTrust =
    page === 'default' &&
    sortedOptions[focusedIndex]?.kind === TRUST_ENTRY_ID &&
    (hasTrustPage || hasKasScopePage);

  const findByKindOrId = (id: string) =>
    sortedOptions.find((o) => o.kind === id || o.optionId === id);

  useKeypress((input, key) => {
    if (key.ctrl || key.meta) return;
    const lower = input.toLowerCase();
    if (page === 'kas-scope' && lower === 's') {
      setTrustScope((prev) =>
        prev === 'session'
          ? 'workspace'
          : prev === 'workspace'
            ? 'global'
            : 'session'
      );
      return;
    }
    if (page !== 'default') return;
    if (lower === 't' && alwaysOpt) {
      if (hasTrustPage) {
        setPage('trust');
        setFocusedIndex(0);
      } else if (hasKasScopePage) {
        setPage('kas-scope');
        setFocusedIndex(0);
      } else {
        respondToApproval(alwaysOpt.optionId, approval);
      }
      return;
    }
    const kind =
      lower === 'y' ? 'allow_once' : lower === 'n' ? 'reject_once' : '';
    if (!kind) return;
    const opt = findByKindOrId(kind);
    if (opt) respondToApproval(opt.optionId, approval);
  });

  const handleSelect = (item: { label: string }) => {
    if (page === 'default') {
      const opt = sortedOptions.find((o) => optionLabel(o) === item.label);
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
      if (opt) respondToApproval(opt.optionId, approval);
    } else if (page === 'kas-scope') {
      if (!alwaysOpt) return;
      const selected = kasScopeItems.find((i) => i.label === item.label);
      const scopeValue = trustScope === 'global' ? 'user' : trustScope;
      respondToApproval(alwaysOpt.optionId, approval, {
        kasScope: scopeValue,
        ...(selected?.resource ? { kasResource: selected.resource } : {}),
        ...(selected?.wholeCapability ? { kasWholeCapability: true } : {}),
      });
    } else {
      if (!alwaysOpt) return;
      if (item.label === ENTIRE_TOOL_LABEL) {
        respondToApproval(
          alwaysOpt.optionId,
          approval,
          agentEngine === 'kas' ? { kasWholeCapability: true } : undefined
        );
        return;
      }
      const selected = trustOptions.find((t) => t.label === item.label);
      if (selected) {
        respondToApproval(alwaysOpt.optionId, approval, {
          trustOption: selected,
        });
      }
    }
  };

  const title =
    page === 'trust'
      ? `${toolName} requires approval ${glyphs.smallDot} trust options`
      : page === 'kas-scope'
        ? `${toolName} requires approval ${glyphs.smallDot} trust [${scopeLabels[trustScope]}] (s to cycle)`
        : `${toolName} requires approval`;

  return (
    <Panel
      title={title}
      width={width}
      onClose={() => {
        if (page === 'trust' || page === 'kas-scope') {
          setPage('default');
          setFocusedIndex(0);
        } else {
          const opt = sortedOptions.find((o) => o.optionId === 'reject_once');
          if (opt) respondToApproval(opt.optionId, approval);
        }
      }}
      hideTitleDivider={true}
      footerLeft={
        focusedOnTrust ? (
          <Text>
            {primary('Enter')} {secondary('to see more options')}
          </Text>
        ) : undefined
      }
    >
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
      {page === 'default' && (
        <Box marginTop={1}>
          <Text>
            {secondary(
              `y approve ${glyphs.smallDot} n deny ${glyphs.smallDot} t trust`
            )}
          </Text>
        </Box>
      )}
    </Panel>
  );
});
