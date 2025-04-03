import { defineConfig } from "eslint/config";
import unusedImports from "eslint-plugin-unused-imports";
import _import from "eslint-plugin-import";
import prettier from "eslint-plugin-prettier";
import { fixupPluginRules } from "@eslint/compat";
import globals from "globals";
import tsParser from "@typescript-eslint/parser";
import path from "node:path";
import { fileURLToPath } from "node:url";
import js from "@eslint/js";
import { FlatCompat } from "@eslint/eslintrc";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const compat = new FlatCompat({
    baseDirectory: __dirname,
    recommendedConfig: js.configs.recommended,
    allConfig: js.configs.all
});

export default defineConfig([{
    extends: compat.extends(
        "eslint:recommended",
        "prettier",
        "plugin:@typescript-eslint/eslint-recommended",
        "plugin:@typescript-eslint/recommended",
    ),

    plugins: {
        "unused-imports": unusedImports,
        import: fixupPluginRules(_import),
        prettier,
    },

    languageOptions: {
        globals: {
            ...globals.browser,
            ...globals.commonjs,
            ...globals.node,
            Atomics: "readonly",
            SharedArrayBuffer: "readonly",
        },

        parser: tsParser,
        ecmaVersion: 2018,
        sourceType: "module",

        parserOptions: {
            ecmaFeatures: {
                experimentalObjectRestSpread: true,
                experimentalDecorators: true,
                jsx: true,
            },

            project: "./tsconfig.withTests.json",
            warnOnUnsupportedTypeScriptVersion: false,
        },
    },

    rules: {
        radix: "error",
        eqeqeq: "error",
        "@typescript-eslint/no-explicit-any": "off",
        "@typescript-eslint/no-non-null-assertion": "off",
        semi: ["warn", "always"],
        "@typescript-eslint/no-unused-vars": "off",
        "unused-imports/no-unused-imports": "warn",

        "unused-imports/no-unused-vars": ["warn", {
            vars: "all",
            varsIgnorePattern: "^_",
            args: "after-used",
            argsIgnorePattern: "^_",
        }],

        "import/no-default-export": "error",
        "prettier/prettier": "warn",

        "no-constant-condition": ["error", {
            checkLoops: false,
        }],

        "no-console": ["warn", {
            allow: ["log", "warn", "error", "info", "clear"],
        }],
    },
}]);