#!/usr/bin/env node
/**
 * Parses CHANGELOG.md (Keep a Changelog format) and emits
 * app-shell/src/release-notes.json with structured sections per version.
 * The upgrade welcome page aggregates notes for all versions newer than
 * the previously installed one.
 * Usage: node scripts/gen-release-notes.mjs
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "CHANGELOG.md");
const OUT = join(ROOT, "app-shell/src/release-notes.json");

const CATEGORIES = new Set(["Added", "Changed", "Deprecated", "Removed", "Fixed", "Security"]);

const lines = readFileSync(SRC, "utf-8").split("\n");
const versions = [];
let current = null;
let section = null;

for (const line of lines) {
  const versionMatch = line.match(/^## \[([0-9.]+)\](?:\s*-\s*(\d{4}-\d{2}-\d{2}))?/);
  if (versionMatch) {
    current = { version: versionMatch[1], date: versionMatch[2] ?? "", sections: [] };
    versions.push(current);
    section = null;
    continue;
  }
  if (!current) continue;
  const catMatch = line.match(/^### (.+)$/);
  if (catMatch) {
    const title = catMatch[1].trim();
    if (CATEGORIES.has(title)) {
      // Merge repeated sections of the same category within one version.
      section = current.sections.find((s) => s.title === title);
      if (!section) {
        section = { title, items: [] };
        current.sections.push(section);
      }
    } else {
      // Non-standard heading: reset section so its items never leak into the
      // previous category (previously "### 测试" items landed under "Fixed").
      console.warn(`[gen-release-notes] v${current.version}: non-standard section "${title}" (items skipped)`);
      section = null;
    }
    continue;
  }
  const itemMatch = line.match(/^\s*[-*]\s+(.+)/);
  if (itemMatch && section) {
    section.items.push(itemMatch[1].trim());
  }
}

writeFileSync(OUT, JSON.stringify({ versions: versions.filter((v) => v.sections.length > 0) }, null, 2) + "\n");
console.log(`release notes: ${versions.length} version(s) -> ${OUT}`);
