import { describe, expect, it } from "vitest";
import { rewriteImports, rewriteImportsWith } from "./pluginLoader";

/** 测试用解析器：仅 react / @zannen/plugin-sdk 视为白名单。 */
const testResolve = (specifier: string): string | null =>
  ({ react: "/test/vendor/react.js", "@zannen/plugin-sdk": "/test/vendor/sdk.js" })[specifier] ??
  null;

describe("rewriteImportsWith", () => {
  it("rewrites whitelisted specifiers (named/default/namespace imports)", () => {
    const source = [
      `import { useState } from "react";`,
      `import React from 'react';`,
      `import * as SDK from "@zannen/plugin-sdk";`,
    ].join("\n");
    expect(rewriteImportsWith(source, testResolve)).toBe(
      [
        `import { useState } from "/test/vendor/react.js";`,
        `import React from "/test/vendor/react.js";`,
        `import * as SDK from "/test/vendor/sdk.js";`,
      ].join("\n"),
    );
  });

  it("rewrites export-from and dynamic import()", () => {
    const source = [`export { useEffect } from "react";`, `const m = await import("react");`].join(
      "\n",
    );
    expect(rewriteImportsWith(source, testResolve)).toBe(
      [`export { useEffect } from "/test/vendor/react.js";`, `const m = await import("/test/vendor/react.js");`].join(
        "\n",
      ),
    );
  });

  it("keeps non-whitelisted specifiers unchanged", () => {
    const source = [
      `import "./styles.css";`,
      `import { helper } from "./local/helper.js";`,
      `import leftpad from "left-pad";`,
    ].join("\n");
    expect(rewriteImportsWith(source, testResolve)).toBe(source);
  });

  it("handles multi-line parenthesized import form", () => {
    const source = [
      `import {`,
      `  useState,`,
      `  useMemo,`,
      `} from "react";`,
      `const fm = await import(`,
      `  "react",`,
      `);`,
    ].join("\n");
    expect(rewriteImportsWith(source, testResolve)).toBe(
      [
        `import {`,
        `  useState,`,
        `  useMemo,`,
        `} from "/test/vendor/react.js";`,
        `const fm = await import(`,
        `  "/test/vendor/react.js",`,
        `);`,
      ].join("\n"),
    );
  });
});

describe("rewriteImports", () => {
  it("rewrites built-in VENDOR_MAP entries to vendor URLs (DEV form under vitest)", () => {
    const source = `import { createI18n } from "@zannen/plugin-sdk";\nimport React from "react";`;
    expect(rewriteImports(source)).toBe(
      `import { createI18n } from "/src/vendor/zannen-plugin-sdk.ts";\nimport React from "/src/vendor/react.ts";`,
    );
  });

  it("does not rewrite unregistered specifiers", () => {
    const source = `import { thing } from "some-random-pkg";`;
    expect(rewriteImports(source)).toBe(source);
  });
});
