import type { VisualCoverage } from './visual-coverage.js';

export interface VisualEvidenceCoverage {
  coverage: VisualCoverage;
  collectionError?: string;
}

export function finalizeVisualEvidence(
  plannedCoverage: VisualCoverage,
  collectCapturedCoverage: () => VisualCoverage,
  persist: (result: VisualEvidenceCoverage) => void
): VisualEvidenceCoverage {
  let result: VisualEvidenceCoverage;
  try {
    result = { coverage: collectCapturedCoverage() };
  } catch (error) {
    result = {
      coverage: plannedCoverage,
      collectionError: error instanceof Error ? error.message : String(error),
    };
  }
  persist(result);
  return result;
}
