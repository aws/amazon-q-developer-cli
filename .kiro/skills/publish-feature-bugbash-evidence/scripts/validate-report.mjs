#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

function usage(message) {
  if (message) console.error(`Error: ${message}\n`);
  console.error(`Usage:
  node validate-report.mjs REPORT.html [options]

Options:
  --expect-sha VALUE
  --expect-branch VALUE
  --expect-pr VALUE
  --expect-stories N
  --expect-checks N
  --min-frames N
  --require-text VALUE       Repeatable
  --forbid-text VALUE        Repeatable
  --strict                   Treat warnings as errors
`);
  process.exit(2);
}

function parseInteger(flag, value) {
  if (!/^\d+$/.test(value ?? '')) {
    usage(`${flag} requires a non-negative integer`);
  }
  return Number(value);
}

function parseArgs(argv) {
  if (!argv.length || argv[0].startsWith('--')) {
    usage('REPORT.html is required');
  }

  const options = {
    reportPath: path.resolve(argv[0]),
    requiredText: [],
    forbiddenText: [],
    strict: false,
  };

  for (let index = 1; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === '--strict') {
      options.strict = true;
      continue;
    }

    const value = argv[++index];
    if (value === undefined) usage(`${flag} requires a value`);

    switch (flag) {
      case '--expect-sha':
        options.expectSha = value;
        break;
      case '--expect-branch':
        options.expectBranch = value;
        break;
      case '--expect-pr':
        options.expectPr = value;
        break;
      case '--expect-stories':
        options.expectStories = parseInteger(flag, value);
        break;
      case '--expect-checks':
        options.expectChecks = parseInteger(flag, value);
        break;
      case '--min-frames':
        options.minFrames = parseInteger(flag, value);
        break;
      case '--require-text':
        options.requiredText.push(value);
        break;
      case '--forbid-text':
        options.forbiddenText.push(value);
        break;
      default:
        usage(`unknown option ${flag}`);
    }
  }

  return options;
}

function resolvePlaywright() {
  const roots = [
    process.cwd(),
    path.join(process.cwd(), 'packages', 'terminal-harness'),
    path.join(process.cwd(), 'packages', 'tui'),
  ];
  const packageNames = ['playwright', '@playwright/test'];

  for (const root of roots) {
    const manifest = path.join(root, 'package.json');
    if (!fs.existsSync(manifest)) continue;

    const requireFromRoot = createRequire(manifest);
    for (const packageName of packageNames) {
      try {
        return requireFromRoot.resolve(packageName);
      } catch {
        // Try the next package or workspace root.
      }
    }
  }

  throw new Error(
    'Playwright was not found from the current repository. Install its existing test dependencies and run this script from the repository root.'
  );
}

function resolveBrowserExecutable(chromium) {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? '';
  const candidates = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
    chromium.executablePath(),
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
    path.join(
      home,
      'Applications',
      'Google Chrome.app',
      'Contents',
      'MacOS',
      'Google Chrome'
    ),
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    process.env.PROGRAMFILES
      ? path.join(
          process.env.PROGRAMFILES,
          'Google',
          'Chrome',
          'Application',
          'chrome.exe'
        )
      : undefined,
  ].filter(Boolean);

  return candidates.find((candidate) => fs.existsSync(candidate));
}

function addTextChecks(options, html, errors) {
  const required = [
    ['SHA', options.expectSha],
    ['branch', options.expectBranch],
    ['PR', options.expectPr],
    ...options.requiredText.map((value) => ['required text', value]),
  ];

  for (const [label, value] of required) {
    if (value && !html.includes(value)) {
      errors.push(`Missing ${label}: ${value}`);
    }
  }

  for (const value of options.forbiddenText) {
    if (html.includes(value)) {
      errors.push(`Forbidden stale text is present: ${value}`);
    }
  }
}

function duplicateValues(values) {
  const seen = new Set();
  const duplicates = new Set();

  for (const value of values) {
    if (seen.has(value)) duplicates.add(value);
    seen.add(value);
  }

  return [...duplicates];
}

const options = parseArgs(process.argv.slice(2));
if (!fs.existsSync(options.reportPath)) {
  usage(`report not found: ${options.reportPath}`);
}
if (path.extname(options.reportPath).toLowerCase() !== '.html') {
  usage('report must be an HTML file');
}

const html = fs.readFileSync(options.reportPath, 'utf8');
const errors = [];
const warnings = [];
addTextChecks(options, html, errors);

const playwrightEntry = resolvePlaywright();
const playwrightModule = await import(pathToFileURL(playwrightEntry).href);
const chromium =
  playwrightModule.chromium ?? playwrightModule.default?.chromium;
if (!chromium) {
  throw new Error(
    `Resolved Playwright entry does not export chromium: ${playwrightEntry}`
  );
}

const executablePath = resolveBrowserExecutable(chromium);
if (!executablePath) {
  throw new Error(
    'No Playwright or system Chromium executable was found. Install the repository Playwright browser or set PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH.'
  );
}

const browser = await chromium.launch({ headless: true, executablePath });
const viewports = [
  { name: 'desktop', width: 1440, height: 900 },
  { name: 'mobile', width: 390, height: 844 },
];
let canonicalCounts;

try {
  for (const viewport of viewports) {
    const page = await browser.newPage({ viewport });
    const pageErrors = [];
    const failedRequests = [];
    const externalRequests = [];
    const reportUrl = pathToFileURL(options.reportPath).href;

    page.on('pageerror', (error) => pageErrors.push(error.message));
    page.on('requestfailed', (request) => {
      failedRequests.push(
        `${request.url()}: ${request.failure()?.errorText ?? 'failed'}`
      );
    });
    page.on('request', (request) => {
      const url = request.url();
      if (
        url !== reportUrl &&
        !url.startsWith('data:') &&
        !url.startsWith('blob:') &&
        !url.startsWith('about:')
      ) {
        externalRequests.push(url);
      }
    });

    await page.goto(reportUrl, { waitUntil: 'load' });
    await page.waitForTimeout(100);

    const result = await page.evaluate(() => {
      const stories = [
        ...document.querySelectorAll('[data-user-story], article.story'),
      ];
      const checks = [
        ...document.querySelectorAll('[data-evidence-check], .checks li'),
      ];
      const frames = [
        ...document.querySelectorAll('[data-evidence-frame], .terminal-frame'),
      ];
      const statusOf = (element) => {
        const explicit = element.getAttribute('data-status')?.toLowerCase();
        if (explicit) return explicit;

        const statusElement = element.querySelector('.status');
        return statusElement?.textContent?.trim().toLowerCase() ?? '';
      };
      const hasStatus = (element, statuses) => {
        const status = statusOf(element);
        return (
          statuses.some((candidate) => status.includes(candidate)) ||
          statuses.some((candidate) => element.classList.contains(candidate))
        );
      };

      const blockedStories = stories.filter((element) =>
        hasStatus(element, ['fail', 'failed', 'skip', 'skipped', 'incomplete'])
      );
      const blockedChecks = checks.filter((element) =>
        hasStatus(element, ['fail', 'failed', 'skip', 'skipped', 'incomplete'])
      );
      const storyIds = stories.map(
        (element) =>
          element.getAttribute('data-user-story') ??
          element.getAttribute('id') ??
          ''
      );
      const frameLabels = frames.map(
        (element) =>
          element.getAttribute('data-evidence-frame') ??
          element.getAttribute('aria-label') ??
          element
            .closest('details')
            ?.querySelector(':scope > summary')
            ?.textContent?.trim() ??
          ''
      );
      const emptyFrames = frames
        .map((element, index) => ({
          index: index + 1,
          label: frameLabels[index] || `frame-${index + 1}`,
          textLength: element.textContent?.trim().length ?? 0,
          visualElements: element.querySelectorAll('img, svg, canvas').length,
          width: element.getBoundingClientRect().width,
          height: element.getBoundingClientRect().height,
        }))
        .filter(
          (frame) =>
            (frame.textLength === 0 && frame.visualElements === 0) ||
            frame.width === 0 ||
            frame.height === 0
        );
      const assetReferences = [
        ...document.querySelectorAll(
          'script[src], link[href], img[src], iframe[src], source[src], video[src], audio[src]'
        ),
      ]
        .map(
          (element) =>
            element.getAttribute('src') ?? element.getAttribute('href')
        )
        .filter(Boolean)
        .filter(
          (value) =>
            !value.startsWith('data:') &&
            !value.startsWith('blob:') &&
            !value.startsWith('#')
        );
      const brokenAnchors = [...document.querySelectorAll("a[href^='#']")]
        .map((anchor) => anchor.getAttribute('href'))
        .filter(
          (href) =>
            href &&
            href !== '#' &&
            !document.getElementById(decodeURIComponent(href.slice(1)))
        );

      return {
        title: document.title.trim(),
        storyCount: stories.length,
        checkCount: checks.length,
        frameCount: frames.length,
        blockedStoryCount: blockedStories.length,
        blockedCheckCount: blockedChecks.length,
        missingStoryIdCount: storyIds.filter((value) => !value).length,
        storyIds: storyIds.filter(Boolean),
        missingFrameLabelCount: frameLabels.filter((value) => !value).length,
        frameLabels: frameLabels.filter(Boolean),
        emptyFrames,
        assetReferences,
        brokenAnchors,
        baseTarget: document.querySelector('base')?.target ?? '',
        documentWidth: document.documentElement.scrollWidth,
        viewportWidth: document.documentElement.clientWidth,
      };
    });

    const viewportCounts = {
      stories: result.storyCount,
      checks: result.checkCount,
      frames: result.frameCount,
    };
    if (canonicalCounts) {
      if (
        viewportCounts.stories !== canonicalCounts.stories ||
        viewportCounts.checks !== canonicalCounts.checks ||
        viewportCounts.frames !== canonicalCounts.frames
      ) {
        errors.push(
          `${viewport.name}: evidence counts differ from the desktop render`
        );
      }
    } else {
      canonicalCounts = viewportCounts;
    }

    if (!result.title) {
      errors.push(`${viewport.name}: report title is empty`);
    }
    if (result.blockedStoryCount > 0) {
      errors.push(
        `${viewport.name}: ${result.blockedStoryCount} failed, skipped, or incomplete story element(s)`
      );
    }
    if (result.blockedCheckCount > 0) {
      errors.push(
        `${viewport.name}: ${result.blockedCheckCount} failed, skipped, or incomplete check element(s)`
      );
    }
    if (result.missingStoryIdCount > 0) {
      errors.push(
        `${viewport.name}: ${result.missingStoryIdCount} story element(s) lack a stable ID`
      );
    }
    if (result.missingFrameLabelCount > 0) {
      errors.push(
        `${viewport.name}: ${result.missingFrameLabelCount} frame element(s) lack a stable label`
      );
    }

    const duplicateStoryIds = duplicateValues(result.storyIds);
    if (duplicateStoryIds.length > 0) {
      errors.push(
        `${viewport.name}: duplicate story IDs: ${duplicateStoryIds.join(', ')}`
      );
    }
    const duplicateFrameLabels = duplicateValues(result.frameLabels);
    if (duplicateFrameLabels.length > 0) {
      errors.push(
        `${viewport.name}: duplicate frame labels: ${duplicateFrameLabels.join(
          ', '
        )}`
      );
    }
    if (result.emptyFrames.length > 0) {
      errors.push(
        `${viewport.name}: empty or hidden frames: ${result.emptyFrames
          .map((frame) => frame.label)
          .join(', ')}`
      );
    }
    if (result.assetReferences.length > 0) {
      errors.push(
        `${viewport.name}: report is not self-contained: ${[
          ...new Set(result.assetReferences),
        ].join(', ')}`
      );
    }
    if (result.brokenAnchors.length > 0) {
      errors.push(
        `${viewport.name}: broken same-page anchors: ${[
          ...new Set(result.brokenAnchors),
        ].join(', ')}`
      );
    }
    if (result.documentWidth > result.viewportWidth) {
      errors.push(
        `${viewport.name}: horizontal overflow ${result.documentWidth}px > ${result.viewportWidth}px`
      );
    }
    if (pageErrors.length > 0) {
      errors.push(`${viewport.name}: page errors: ${pageErrors.join(' | ')}`);
    }
    if (failedRequests.length > 0) {
      errors.push(
        `${viewport.name}: failed requests: ${failedRequests.join(' | ')}`
      );
    }
    if (externalRequests.length > 0) {
      errors.push(
        `${viewport.name}: external subresource requests: ${[
          ...new Set(externalRequests),
        ].join(', ')}`
      );
    }
    if (result.baseTarget !== '_blank') {
      warnings.push('Missing <base target="_blank">');
    }

    await page.close();
  }
} finally {
  await browser.close();
}

if (canonicalCounts.stories === 0) {
  errors.push('No user stories found');
}
if (canonicalCounts.checks === 0) {
  errors.push('No evidence checks found');
}
if (canonicalCounts.frames === 0) {
  errors.push('No evidence frames found');
}
if (
  options.expectStories !== undefined &&
  canonicalCounts.stories !== options.expectStories
) {
  errors.push(
    `Story count mismatch: expected ${options.expectStories}, found ${canonicalCounts.stories}`
  );
}
if (
  options.expectChecks !== undefined &&
  canonicalCounts.checks !== options.expectChecks
) {
  errors.push(
    `Check count mismatch: expected ${options.expectChecks}, found ${canonicalCounts.checks}`
  );
}
if (
  options.minFrames !== undefined &&
  canonicalCounts.frames < options.minFrames
) {
  errors.push(
    `Frame count too low: expected at least ${options.minFrames}, found ${canonicalCounts.frames}`
  );
}

const uniqueWarnings = [...new Set(warnings)];
const uniqueErrors = [...new Set(errors)];
console.log(`Report: ${options.reportPath}`);
console.log(
  `Counts: ${canonicalCounts.stories} stories, ${canonicalCounts.checks} checks, ${canonicalCounts.frames} frames`
);
console.log('Viewports checked: desktop 1440x900, mobile 390x844');
for (const warning of uniqueWarnings) console.log(`WARN: ${warning}`);
for (const error of uniqueErrors) console.error(`ERROR: ${error}`);

if (uniqueErrors.length > 0 || (options.strict && uniqueWarnings.length > 0)) {
  process.exit(1);
}

console.log('PASS: evidence report validation completed');
