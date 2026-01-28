import { ReactNode } from 'react';
import { Text as InkText, TextProps } from 'ink';

/**
 * Text component - A wrapper around Ink's Text component that enforces consistent styling practices.
 *
 * This wrapper:
 * - Removes styling props (color, bold, italic, etc.) to prevent inconsistent usage
 * - Forces all styling to go through chalk functions for consistency
 * - Maintains terminal color capability detection
 * - Provides a clean, predictable API
 *
 * Usage: All styling should be done via chalk functions in children
 * Example: <Text>{useTextStyle('label')('Styled text')}</Text>
 * Example: <Text>{getColor('accent')('Colored text')}</Text>
 */

export interface CustomTextProps extends Omit<
  TextProps,
  'color' | 'bold' | 'italic' | 'underline' | 'strikethrough' | 'dimColor'
> {
  children: ReactNode;
}

export function Text({ children, ...props }: CustomTextProps) {
  return <InkText {...props}>{children}</InkText>;
}
