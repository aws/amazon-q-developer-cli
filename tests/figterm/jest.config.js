module.exports = {
  preset: 'ts-jest/presets/js-with-ts',
  testPathIgnorePatterns: ['/node_modules/', '/mocks/', '/parserCorpus/'],
  coveragePathIgnorePatterns: ['/mocks/', '/parserCorpus/'],
  testEnvironment: 'jsdom',
  reporters: ['./jest-reporter.js'],
};
