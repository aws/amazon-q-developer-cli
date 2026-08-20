import type {
  StorybookAssertions,
  StorybookParameters,
  StorybookViewport,
} from './contracts.js';

export function certifyVisualStory(
  readyText: string,
  assertions: StorybookAssertions = { visible: [readyText] },
  coversVisualStates: readonly string[] = [],
  viewport?: StorybookViewport,
  environment?: Readonly<Record<string, string>>
): StorybookParameters {
  return {
    ...(coversVisualStates.length > 0 ? { coversVisualStates } : {}),
    certification: {
      suite: 'visual-stories',
      readyText,
      assertions: {
        visible: assertions.visible ?? [readyText],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
      ...(viewport ? { viewport } : {}),
      ...(environment ? { environment } : {}),
    },
  };
}
