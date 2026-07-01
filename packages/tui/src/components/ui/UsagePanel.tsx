import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/Panel.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import type { UsageData } from '../../stores/app-store.js';

interface UsagePanelProps {
  data: UsageData | null;
  onClose: () => void;
  onTabSwitch?: () => void;
}

function UsageProgressBar({
  percentage,
  width,
}: {
  percentage: number;
  width: number;
}) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  const colorKey =
    percentage >= 100 ? 'error' : percentage >= 90 ? 'warning' : 'brand';
  const color = getColor(colorKey).hex;
  const rawEmptyColor = getColor('muted').hex;
  const emptyColor = rawEmptyColor === 'inherit' ? undefined : rawEmptyColor;

  const filled =
    percentage > 0
      ? Math.max(1, Math.round((Math.min(percentage, 100) / 100) * width))
      : 0;
  const empty = width - filled;

  return (
    <Text>
      <InkText color={color}>{glyphs.bar.repeat(filled)}</InkText>
      <InkText color={emptyColor}>{glyphs.bar.repeat(empty)}</InkText>
    </Text>
  );
}

export function UsagePanel({ data, onClose, onTabSwitch }: UsagePanelProps) {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const brand = getColor('brand');

  const barWidth = Math.max(20, termWidth - 30);

  if (!data) {
    return (
      <Panel
        title="/usage"
        onClose={onClose}
        onTabSwitch={onTabSwitch}
        showTabHint={true}
        tabHintLabel="to switch to /context"
      >
        <Text>{secondary('Loading usage data...')}</Text>
      </Panel>
    );
  }

  // Defensive: older agents (e.g. an out-of-date KAS) may omit addOnCredits.
  const addOnCredits = data.addOnCredits ?? [];

  return (
    <Panel
      title="/usage"
      onClose={onClose}
      onTabSwitch={onTabSwitch}
      showTabHint={true}
      tabHintLabel="to switch to /context"
    >
      <Box>
        <Text>
          {primary.bold('Estimated Usage')}
          {secondary(` | resets on ${data.billingCycleReset}`)}
          {data.planName !== 'Unknown' ? (
            <>
              {secondary(' | ')}
              {brand(data.planName)}
            </>
          ) : (
            ''
          )}
        </Text>
      </Box>
      {data.usageBreakdowns.map((item, i) => {
        const pct = item.limit > 0 ? (item.used / item.limit) * 100 : 0;
        return (
          <Box key={i} flexDirection="column">
            <Box justifyContent="space-between">
              <Text>
                {primary(item.displayName)}{' '}
                {item.hasLimit
                  ? secondary(
                      `(${item.used.toFixed(2)} of ${item.limit} covered in plan)`
                    )
                  : secondary(`(${item.used.toFixed(2)} used)`)}
              </Text>
            </Box>
            {item.hasLimit && (
              <Box marginTop={1}>
                <UsageProgressBar percentage={pct} width={barWidth} />
                <Text> {secondary(`${pct.toFixed(1)}%`)}</Text>
              </Box>
            )}
          </Box>
        );
      })}

      {!data.isEnterprise &&
        (data.overageCapable || addOnCredits.length > 0) && (
          <Box flexDirection="column" marginTop={1}>
            <Text>{primary('Additional credits')}</Text>
            {addOnCredits.length === 0 ? (
              <Text>{secondary('Add-on credits available for purchase')}</Text>
            ) : (
              <>
                {addOnCredits
                  .filter((p) => p.isActive)
                  .map((p, i) => (
                    <Text key={`active-${i}`}>
                      {`${p.used.toFixed(2)} of ${Math.round(p.total)} used${
                        p.expiresAt ? `, expires ${p.expiresAt}` : ''
                      }`}
                    </Text>
                  ))}
                {(() => {
                  const inactive = addOnCredits.filter((p) => !p.isActive);
                  if (inactive.length === 0) return null;
                  const used = inactive.reduce((s, p) => s + p.used, 0);
                  const total = inactive.reduce((s, p) => s + p.total, 0);
                  return (
                    <Text>
                      {secondary(
                        `${used.toFixed(2)} of ${Math.round(total)} credits across ${inactive.length} pack${
                          inactive.length === 1 ? '' : 's'
                        }`
                      )}
                    </Text>
                  );
                })()}
              </>
            )}
          </Box>
        )}

      {data.bonusCredits.length > 0 && (
        <Box flexDirection="column" marginTop={1}>
          <Text>{primary('Bonus Credits:')}</Text>
          {data.bonusCredits.map((credit, i) => (
            <Text key={i}>
              {secondary(
                `  ${credit.name}: ${credit.used}/${credit.total} (expires in ${credit.daysUntilExpiry} days)`
              )}
            </Text>
          ))}
        </Box>
      )}

      <Box marginTop={1}>
        <Text>
          {data.isEnterprise ? (
            secondary(
              'Since your account is through your organization, for account management please contact your account administrator.'
            )
          ) : (
            <>
              {secondary(
                data.overageCapable
                  ? 'To manage your plan or purchase add-on credits navigate to '
                  : 'To manage your plan navigate to '
              )}
              {brand('app.kiro.dev/account/usage')}
            </>
          )}
        </Text>
      </Box>
    </Panel>
  );
}
