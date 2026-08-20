import type {
  StorybookAssertions,
  StorybookCaptureDefinition,
} from './contracts.js';

export function mergeStoryAssertions(
  invariant: StorybookAssertions | undefined,
  capture: StorybookAssertions | undefined
): StorybookAssertions {
  return {
    visible: [...(invariant?.visible ?? []), ...(capture?.visible ?? [])],
    hidden: [...(invariant?.hidden ?? []), ...(capture?.hidden ?? [])],
    ordered: [...(invariant?.ordered ?? []), ...(capture?.ordered ?? [])],
    occurrences: {
      ...(invariant?.occurrences ?? {}),
      ...(capture?.occurrences ?? {}),
    },
    styled: [...(invariant?.styled ?? []), ...(capture?.styled ?? [])],
  };
}

const RENDERER_FAILURE_MARKERS = [
  'An error occurred in the <',
  'The above error occurred in the <',
] as const;

export function rendererFailureMessages(text: readonly string[]): string[] {
  const screen = text.join('\n');
  return RENDERER_FAILURE_MARKERS.filter((marker) =>
    screen.includes(marker)
  ).map((marker) => `renderer failure marker "${marker}"`);
}

function fallbackCaptureId(label: string): string {
  return (
    label
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '') || 'capture'
  );
}

export function validateCaptureId(id: string): void {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(id)) {
    throw new Error(`Invalid visual capture id "${id}"`);
  }
}

export function resolveCaptureDefinition(
  requestedId: string,
  definitions: Readonly<Record<string, StorybookCaptureDefinition>> | undefined
): { id: string; definition: StorybookCaptureDefinition } {
  if (!definitions) {
    const id = fallbackCaptureId(requestedId);
    validateCaptureId(id);
    return { id, definition: { label: requestedId } };
  }
  validateCaptureId(requestedId);
  const definition = definitions[requestedId];
  if (!definition) {
    throw new Error(
      `Visual journey captured undeclared state "${requestedId}"`
    );
  }
  return { id: requestedId, definition };
}

export function missingCaptureIds(
  definitions: Readonly<Record<string, StorybookCaptureDefinition>>,
  capturedIds: ReadonlySet<string>
): string[] {
  return Object.keys(definitions).filter((id) => !capturedIds.has(id));
}
