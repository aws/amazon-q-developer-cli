import chalk from 'chalk';
import * as path from 'path';
import { getConsoleOutput } from '@jest/console';
import { Config } from '@jest/types';
import { Reporter, AggregatedResult } from '@jest/reporters';
import { Test, TestResult } from '@jest/test-result';

const formatTime = (
  time: number,
  prefixPower = -3,
  padLeftLength = 0
): string => {
  const prefixes = ['n', 'μ', 'm', ''];
  const prefixIndex = Math.max(
    0,
    Math.min(
      Math.trunc(prefixPower / 3) + prefixes.length - 1,
      prefixes.length - 1
    )
  );
  return `${String(time).padStart(padLeftLength)} ${prefixes[prefixIndex]}s`;
};

const chalkHeader = (text: string, color: chalk.Chalk) =>
  chalk.supportsColor ? color(` ${text} `) : text;

const printDisplayName = (config: Config.ProjectConfig): string => {
  if (!config.displayName) return '';
  const { name, color } = config.displayName;
  const chosenColor = chalk.reset.inverse[color] || chalk.reset.inverse.white;
  return chalkHeader(name, chosenColor);
};

const formatTestPath = (
  config: Config.GlobalConfig | Config.ProjectConfig,
  testPath: Config.Path
): string => {
  const p = path.relative(
    (config as Config.ProjectConfig).cwd || config.rootDir,
    testPath
  );
  return chalk.dim(path.dirname(p) + path.sep) + chalk.bold(path.basename(p));
};

const TITLE_BULLET = chalk.bold('\u25cf ');
const LONG_TEST_COLOR = chalk.reset.bold.bgRed;
const FAIL = chalkHeader('FAIL', chalk.reset.inverse.bold.red);
const PASS = chalkHeader('PASS', chalk.reset.inverse.bold.green);

const getResultHeader = (
  result: TestResult,
  globalConfig: Config.GlobalConfig,
  projectConfig?: Config.ProjectConfig
): string => {
  const testPath = result.testFilePath;
  const formattedTestPath = formatTestPath(
    projectConfig || globalConfig,
    testPath
  );
  const status =
    result.numFailingTests > 0 || result.testExecError ? FAIL : PASS;

  const testDetail = [];

  if (result.perfStats?.slow) {
    const runTime = result.perfStats.runtime / 1000;

    testDetail.push(LONG_TEST_COLOR(formatTime(runTime, 0)));
  }

  if (result.memoryUsage) {
    const toMB = (bytes: number) => Math.floor(bytes / 1024 / 1024);
    testDetail.push(`${toMB(result.memoryUsage)} MB heap size`);
  }

  const projectDisplayName =
    projectConfig && projectConfig.displayName
      ? `${printDisplayName(projectConfig)} `
      : '';

  return `${status} ${projectDisplayName}${formattedTestPath}${
    testDetail.length ? ` (${testDetail.join(', ')})` : ''
  }`;
};

export const getSummary = (aggregatedResults: AggregatedResult): string => {
  const runTime = (Date.now() - aggregatedResults.startTime) / 1000;
  const suitesFailed = aggregatedResults.numFailedTestSuites;
  const suitesPassed = aggregatedResults.numPassedTestSuites;
  const suitesPending = aggregatedResults.numPendingTestSuites;
  const suitesRun = suitesFailed + suitesPassed;
  const suitesTotal = aggregatedResults.numTotalTestSuites;
  const testsFailed = aggregatedResults.numFailedTests;
  const testsPassed = aggregatedResults.numPassedTests;
  const testsPending = aggregatedResults.numPendingTests;
  const testsTodo = aggregatedResults.numTodoTests;
  const testsTotal = aggregatedResults.numTotalTests;

  const suites = `${
    chalk.bold('Test Suites: ') +
    (suitesFailed ? `${chalk.bold.red(`${suitesFailed} failed`)}, ` : '') +
    (suitesPending
      ? `${chalk.bold.yellow(`${suitesPending} skipped`)}, `
      : '') +
    (suitesPassed ? `${chalk.bold.green(`${suitesPassed} passed`)}, ` : '') +
    (suitesRun !== suitesTotal ? `${suitesRun} of ${suitesTotal}` : suitesTotal)
  } total`;

  const tests = `${
    chalk.bold('Tests:       ') +
    (testsFailed > 0 ? `${chalk.bold.red(`${testsFailed} failed`)}, ` : '') +
    (testsPending > 0
      ? `${chalk.bold.yellow(`${testsPending} skipped`)}, `
      : '') +
    (testsTodo > 0 ? `${chalk.bold.magenta(`${testsTodo} todo`)}, ` : '') +
    (testsPassed > 0 ? `${chalk.bold.green(`${testsPassed} passed`)}, ` : '')
  }${testsTotal} total`;

  const time = `${chalk.bold(`Time:`)}        ${formatTime(runTime, 0)}`;
  return [suites, tests, time].join('\n');
};

const TEST_SUMMARY_THRESHOLD = 20;

const getTestSummary = (
  contexts: Set<Test['context']>,
  globalConfig: Config.GlobalConfig
): string => {
  const getMatchingTestsInfo = () => {
    const prefix = globalConfig.findRelatedTests
      ? ' related to files matching '
      : ' matching ';

    return (
      chalk.dim(prefix) +
      new RegExp(globalConfig.testPathPattern, 'i').toString()
    );
  };

  let testInfo = '';

  if (globalConfig.runTestsByPath) {
    testInfo = chalk.dim(' within paths');
  } else if (globalConfig.onlyChanged) {
    testInfo = chalk.dim(' related to changed files');
  } else if (globalConfig.testPathPattern) {
    testInfo = getMatchingTestsInfo();
  }

  let nameInfo = '';

  if (globalConfig.runTestsByPath) {
    nameInfo = ` ${globalConfig.nonFlagArgs.map((p) => `"${p}"`).join(', ')}`;
  } else if (globalConfig.testNamePattern) {
    nameInfo = `${chalk.dim(' with tests matching ')}"${
      globalConfig.testNamePattern
    }"`;
  }

  const contextInfo =
    contexts.size > 1
      ? chalk.dim(' in ') + contexts.size + chalk.dim(' projects')
      : '';

  return (
    chalk.dim('Ran all test suites') +
    testInfo +
    nameInfo +
    contextInfo +
    chalk.dim('.')
  );
};

export default class CustomReporter
  implements Pick<Reporter, 'onTestResult' | 'onRunComplete'>
{
  globalConfig: Config.GlobalConfig;

  results: { test: Test; result: TestResult }[] = [];

  constructor(globalConfig: Config.GlobalConfig) {
    this.globalConfig = globalConfig;
  }

  onTestResult(test: Test, testResult: TestResult) {
    this.results.push({ test, result: testResult });
  }

  // eslint-disable-next-line class-methods-use-this
  onRunStart() {
    console.log('');
  }

  onRunComplete(
    contexts: Set<Test['context']>,
    aggregatedResults: AggregatedResult
  ) {
    const { numTotalTestSuites, testResults, wasInterrupted } =
      aggregatedResults;

    this.results.forEach(({ test, result }) => {
      if (!result.skipped) {
        console.log(
          getResultHeader(result, this.globalConfig, test.context.config)
        );
        if (result.console) {
          console.log(
            `  ${TITLE_BULLET}Console\n\n${getConsoleOutput(
              result.console,
              test.context.config,
              this.globalConfig
            )}`
          );
        }
        if (result.failureMessage) {
          console.log(result.failureMessage);
        }
      }
    });

    if (numTotalTestSuites) {
      const lastResult = testResults[testResults.length - 1];
      if (
        !this.globalConfig.verbose &&
        lastResult &&
        !lastResult.numFailingTests &&
        !lastResult.testExecError
      ) {
        console.log('');
      }

      const failedTests = aggregatedResults.numFailedTests;
      const runtimeErrors = aggregatedResults.numRuntimeErrorTestSuites;
      if (
        failedTests + runtimeErrors > 0 &&
        aggregatedResults.numTotalTestSuites > TEST_SUMMARY_THRESHOLD
      ) {
        console.log(chalk.bold('Summary of all failing tests'));
        aggregatedResults.testResults.forEach((testResult) => {
          const { failureMessage } = testResult;
          if (failureMessage) {
            console.log(
              `${getResultHeader(
                testResult,
                this.globalConfig
              )}\n${failureMessage}\n`
            );
          }
        });
        console.log('');
      }

      if (numTotalTestSuites) {
        let message = getSummary(aggregatedResults);

        if (!this.globalConfig.silent) {
          message += `\n${
            wasInterrupted
              ? chalk.bold.red('Test run was interrupted.')
              : getTestSummary(contexts, this.globalConfig)
          }`;
        }
        console.log(message);
      }
    }
  }
}
