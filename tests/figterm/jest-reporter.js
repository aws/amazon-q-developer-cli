// reporter.js

// jest can't transpile transformers on the fly so we help him
// https://github.com/facebook/jest/issues/10105

/* eslint-disable import/no-extraneous-dependencies */
const tsNode = require('ts-node/register/transpile-only');
const Reporter = require('./jest-reporter.ts');

module.exports = Reporter;
