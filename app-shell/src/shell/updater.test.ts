import { describe, expect, it } from "vitest";
import { compareSemver } from "./updater";

describe("compareSemver", () => {
  it("equal versions return 0", () => {
    expect(compareSemver("1.2.3", "1.2.3")).toBe(0);
    expect(compareSemver("0.0.0", "0.0.0")).toBe(0);
    expect(compareSemver("1.2", "1.2.0")).toBe(0);
  });

  it("ascending compare returns 1", () => {
    expect(compareSemver("1.2.4", "1.2.3")).toBe(1);
    expect(compareSemver("1.3.0", "1.2.9")).toBe(1);
    expect(compareSemver("2.0.0", "1.9.9")).toBe(1);
  });

  it("descending compare returns -1", () => {
    expect(compareSemver("1.2.3", "1.2.4")).toBe(-1);
    expect(compareSemver("1.2.9", "1.3.0")).toBe(-1);
    expect(compareSemver("0.9.9", "1.0.0")).toBe(-1);
  });

  it("ignores pre-release suffix", () => {
    expect(compareSemver("1.2.3-beta", "1.2.3")).toBe(0);
    expect(compareSemver("1.2.3", "1.2.3-alpha.1")).toBe(0);
    expect(compareSemver("1.2.4-rc.1", "1.2.3")).toBe(1);
    expect(compareSemver("0.9.0", "1.0.0-alpha")).toBe(-1);
  });
});
