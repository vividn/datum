process.env.TZ = 'UTC';

export default {
  preset: 'ts-jest/presets/default-esm',

  clearMocks: true,
  resetMocks: true,
  restoreMocks: true,

  coverageDirectory: "coverage",

  coveragePathIgnorePatterns: ["/node_modules/", "/src/views/datumViews/"],

  moduleNameMapper: {
    "axios": "axios/dist/node/axios.cjs",
    '^(\\.{1,2}/.*)\\.js$': '$1',
  },

  modulePathIgnorePatterns: [
    "<rootDir>/build/"
  ],

  setupFiles: ["<rootDir>/jest.setup.mjs"],

  setupFilesAfterEnv: [],

  testEnvironment: "node",

  transformIgnorePatterns: [
    "/node_modules/",
    "\\.pnp\\.[^\\/]+$"
  ],

  prettierPath: "prettier-2",

  extensionsToTreatAsEsm: ['.ts'],

  transform: {
    '^.+\\.ts$': ['ts-jest', {
      useESM: true,
      tsconfig: {
        types: ['node', 'jest'],
        verbatimModuleSyntax: false,
      },
    }],
  },
};
