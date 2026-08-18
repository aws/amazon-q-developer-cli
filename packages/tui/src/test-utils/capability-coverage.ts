import { access, readFile } from 'node:fs/promises';
import { relative, resolve } from 'node:path';

export interface CapabilityCoverageEntry {
  id: string;
  area: string;
  description: string;
  evidence: string[];
}

export interface CapabilityCoverageManifest {
  schemaVersion: 1;
  suite: string;
  description: string;
  denominator: string;
  capabilities: CapabilityCoverageEntry[];
}

export interface CapabilityCoverageArea {
  area: string;
  covered: number;
  total: number;
  percent: number;
}

export interface CapabilityCoverageReport {
  schemaVersion: 1;
  suite: string;
  description: string;
  denominator: string;
  covered: number;
  total: number;
  percent: number;
  areas: CapabilityCoverageArea[];
  uncovered: CapabilityCoverageEntry[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.every((entry) => typeof entry === 'string' && entry.length > 0)
  );
}

function parseEvidenceReference(reference: string): {
  file: string;
  test: string;
} {
  const separator = reference.indexOf('#');
  if (separator <= 0 || separator === reference.length - 1) {
    throw new Error(`Invalid capability evidence reference: ${reference}`);
  }
  return {
    file: reference.slice(0, separator),
    test: reference.slice(separator + 1),
  };
}

function parseEntry(value: unknown, index: number): CapabilityCoverageEntry {
  if (
    !isRecord(value) ||
    typeof value.id !== 'string' ||
    !value.id ||
    typeof value.area !== 'string' ||
    !value.area ||
    typeof value.description !== 'string' ||
    !value.description ||
    !isStringArray(value.evidence)
  ) {
    throw new Error(`Invalid capability at index ${index}`);
  }
  for (const evidence of value.evidence) {
    parseEvidenceReference(evidence);
  }
  return {
    id: value.id,
    area: value.area,
    description: value.description,
    evidence: value.evidence,
  };
}

export function parseCapabilityCoverageManifest(
  value: unknown
): CapabilityCoverageManifest {
  if (
    !isRecord(value) ||
    value.schemaVersion !== 1 ||
    typeof value.suite !== 'string' ||
    !value.suite ||
    typeof value.description !== 'string' ||
    !value.description ||
    typeof value.denominator !== 'string' ||
    !value.denominator ||
    !Array.isArray(value.capabilities) ||
    value.capabilities.length === 0
  ) {
    throw new Error('Invalid capability coverage manifest');
  }

  const capabilities = value.capabilities.map(parseEntry);
  const ids = new Set<string>();
  for (const capability of capabilities) {
    if (ids.has(capability.id)) {
      throw new Error(`Duplicate capability id: ${capability.id}`);
    }
    ids.add(capability.id);
  }

  return {
    schemaVersion: 1,
    suite: value.suite,
    description: value.description,
    denominator: value.denominator,
    capabilities,
  };
}

function decodeXmlAttribute(value: string): string {
  return value
    .replaceAll('&quot;', '"')
    .replaceAll('&apos;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&');
}

function xmlAttribute(tag: string, name: string): string | undefined {
  const prefix = ` ${name}="`;
  const start = tag.indexOf(prefix);
  if (start < 0) return undefined;
  const valueStart = start + prefix.length;
  const end = tag.indexOf('"', valueStart);
  return end < 0 ? undefined : decodeXmlAttribute(tag.slice(valueStart, end));
}

export function parsePassedTestEvidence(junitXml: string): ReadonlySet<string> {
  const passed = new Set<string>();
  let cursor = 0;
  while (cursor < junitXml.length) {
    const start = junitXml.indexOf('<testcase ', cursor);
    if (start < 0) break;
    const tagEnd = junitXml.indexOf('>', start);
    if (tagEnd < 0) break;

    const openingTag = junitXml.slice(start, tagEnd + 1);
    const selfClosing = openingTag.endsWith('/>');
    const closingTag = selfClosing
      ? tagEnd
      : junitXml.indexOf('</testcase>', tagEnd + 1);
    if (closingTag < 0) break;

    const body = selfClosing ? '' : junitXml.slice(tagEnd + 1, closingTag);
    const file = xmlAttribute(openingTag, 'file');
    const name = xmlAttribute(openingTag, 'name');
    if (
      file !== undefined &&
      name !== undefined &&
      !body.includes('<failure') &&
      !body.includes('<error') &&
      !body.includes('<skipped')
    ) {
      passed.add(`${file.replaceAll('\\', '/')}#${name}`);
    }
    cursor = selfClosing ? tagEnd + 1 : closingTag + '</testcase>'.length;
  }
  return passed;
}

export async function loadPassedTestEvidence(
  junitPath: string
): Promise<ReadonlySet<string>> {
  return parsePassedTestEvidence(await readFile(junitPath, 'utf8'));
}

export async function loadCapabilityCoverageManifest(
  manifestPath: string
): Promise<CapabilityCoverageManifest> {
  const content = await readFile(manifestPath, 'utf8');
  return parseCapabilityCoverageManifest(JSON.parse(content) as unknown);
}

export async function validateCapabilityEvidence(
  manifest: CapabilityCoverageManifest,
  evidenceRoot: string,
  passedTests: ReadonlySet<string>
): Promise<void> {
  const packageRoot = resolve(evidenceRoot);
  for (const capability of manifest.capabilities) {
    for (const evidence of capability.evidence) {
      const { file } = parseEvidenceReference(evidence);
      const evidencePath = resolve(packageRoot, file);
      if (relative(packageRoot, evidencePath).startsWith('..')) {
        throw new Error(
          `Evidence for ${capability.id} escapes the package root: ${file}`
        );
      }
      try {
        await access(evidencePath);
      } catch {
        throw new Error(
          `Evidence for ${capability.id} does not exist: ${file}`
        );
      }
      if (!passedTests.has(evidence)) {
        throw new Error(
          `Evidence for ${capability.id} did not pass in the test report: ${evidence}`
        );
      }
    }
  }
}

export function calculateCapabilityCoverage(
  manifest: CapabilityCoverageManifest
): CapabilityCoverageReport {
  const byArea = new Map<string, { covered: number; total: number }>();
  const uncovered: CapabilityCoverageEntry[] = [];
  let covered = 0;

  for (const capability of manifest.capabilities) {
    const area = byArea.get(capability.area) ?? { covered: 0, total: 0 };
    area.total += 1;
    if (capability.evidence.length > 0) {
      covered += 1;
      area.covered += 1;
    } else {
      uncovered.push(capability);
    }
    byArea.set(capability.area, area);
  }

  const total = manifest.capabilities.length;
  return {
    schemaVersion: 1,
    suite: manifest.suite,
    description: manifest.description,
    denominator: manifest.denominator,
    covered,
    total,
    percent: percentage(covered, total),
    areas: [...byArea].map(([area, counts]) => ({
      area,
      ...counts,
      percent: percentage(counts.covered, counts.total),
    })),
    uncovered,
  };
}

export function formatCapabilityCoverageMarkdown(
  report: CapabilityCoverageReport
): string {
  const lines = [
    `## ${report.suite} capability coverage`,
    '',
    `${report.covered}/${report.total} capabilities (${report.percent.toFixed(1)}%).`,
    '',
    `Denominator: ${report.denominator}`,
    '',
    '| Area | Covered | Total | Coverage |',
    '| --- | ---: | ---: | ---: |',
    ...report.areas.map(
      (area) =>
        `| ${area.area} | ${area.covered} | ${area.total} | ${area.percent.toFixed(1)}% |`
    ),
    '',
    '### Uncovered capabilities',
    '',
    ...(report.uncovered.length > 0
      ? report.uncovered.map(
          (capability) => `- \`${capability.id}\`: ${capability.description}`
        )
      : ['None.']),
    '',
    '> This is capability coverage, not source-code line coverage.',
    '',
  ];
  return lines.join('\n');
}

function percentage(covered: number, total: number): number {
  return total === 0 ? 0 : (covered / total) * 100;
}
