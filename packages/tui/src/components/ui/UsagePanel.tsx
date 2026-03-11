import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/Panel.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';

interface UsagePanelProps {
  data: UsageData | null;
  onClose: () => void;
  onTabSwitch?: () => void;
}

interface UsageBreakdownItem {
  displayName: string;
  used: number;
  limit: number;
  percentage: number;
  currentOverages: number;
  overageRate: number;
  overageCharges: number;
  currency: string;
}

interface BonusCredit {
  name: string;
  used: number;
  total: number;
  daysUntilExpiry: number;
}

interface UsageData {
  planName: string;
  overagesEnabled: boolean;
  isEnterprise: boolean;
  usageBreakdowns: UsageBreakdownItem[];
  bonusCredits: BonusCredit[];
}

function UsageProgressBar({
  percentage,
  width,
}: {
  percentage: number;
  width: number;
}) {
  const { colors } = useTheme();

  let color = colors.brand.truecolor ?? colors.brand.named ?? 'blue';
  if (percentage >= 100)
    color = colors.error.truecolor ?? colors.error.named ?? 'red';
  else if (percentage >= 90)
    color = colors.warning.truecolor ?? colors.warning.named ?? 'yellow';

  const emptyColor = colors.muted.truecolor ?? colors.muted.named ?? 'gray';

  const filled =
    percentage > 0
      ? Math.max(1, Math.round((Math.min(percentage, 100) / 100) * width))
      : 0;
  const empty = width - filled;

  return (
    <Text>
      <InkText color={color}>{'█'.repeat(filled)}</InkText>
      <InkText color={emptyColor}>{'█'.repeat(empty)}</InkText>
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
      >
        <Text>{secondary('Loading usage data...')}</Text>
      </Panel>
    );
  }

  return (
    <Panel
      title="/usage"
      onClose={onClose}
      onTabSwitch={onTabSwitch}
      showTabHint={true}
    >
      {data.usageBreakdowns.map((item, i) => {
        const pct = item.limit > 0 ? (item.used / item.limit) * 100 : 0;
        return (
          <Box key={i} flexDirection="column" marginBottom={1}>
            <Box justifyContent="space-between">
              <Text>
                {primary(item.displayName)}{' '}
                {secondary(
                  `(${item.used.toFixed(2)} of ${item.limit} covered in plan)`
                )}
              </Text>
              <Text>{secondary(`${pct.toFixed(1)}%`)}</Text>
            </Box>
            <UsageProgressBar percentage={pct} width={barWidth} />
          </Box>
        );
      })}

      <Box marginTop={1} marginBottom={1}>
        <Text>
          {primary('Overages: ')}
          {data.overagesEnabled ? (
            <>
              {primary('Enabled')}{' '}
              {secondary(
                `billed at $${data.usageBreakdowns[0]?.overageRate.toFixed(2)} per request`
              )}
            </>
          ) : (
            secondary('Disabled')
          )}
        </Text>
      </Box>

      {data.overagesEnabled && data.usageBreakdowns[0] && (
        <Box flexDirection="column" marginBottom={1}>
          <Text>
            {secondary(
              `Credits used: ${data.usageBreakdowns[0].currentOverages}`
            )}
          </Text>
          <Text>
            {secondary(
              `Est. cost: $${data.usageBreakdowns[0].overageCharges.toFixed(2)} ${data.usageBreakdowns[0].currency}`
            )}
          </Text>
        </Box>
      )}

      {data.bonusCredits.length > 0 && (
        <Box flexDirection="column" marginTop={1} marginBottom={1}>
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

      <Box marginTop={2}>
        <Text>
          {secondary('To manage your plan or configure overages navigate to ')}
          {brand('app.kiro.dev/account/usage')}
        </Text>
      </Box>

      {data.isEnterprise && (
        <Box marginTop={1}>
          <Text>
            {secondary(
              'Since your account is through your organization, contact your admin for billing details.'
            )}
          </Text>
        </Box>
      )}
    </Panel>
  );
}
