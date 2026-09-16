import { describe, expect, it } from "vitest";
import { dotKind, SPEAKING_WINDOW_MS, WARN_WINDOW_MS } from "./indicator";

const base = {
  now: 10_000,
  speaking: false,
  lastAudioAt: 0,
  warnUntil: 0,
};

describe("dotKind", () => {
  it("prioritizes error over everything", () => {
    expect(
      dotKind({ ...base, state: "error", speaking: true, lastAudioAt: 10_000, warnUntil: 99_999 }),
    ).toBe("error");
  });

  it("warn flashes during the warning window", () => {
    expect(dotKind({ ...base, state: "listening", warnUntil: 10_000 + WARN_WINDOW_MS })).toBe(
      "warn",
    );
  });

  it("idle when session is idle", () => {
    expect(dotKind({ ...base, state: "idle" })).toBe("idle");
  });

  it("speaking only within the freshness window", () => {
    expect(
      dotKind({ ...base, state: "listening", speaking: true, lastAudioAt: 10_000 - 100 }),
    ).toBe("speaking");
    expect(
      dotKind({
        ...base,
        state: "listening",
        speaking: true,
        lastAudioAt: 10_000 - SPEAKING_WINDOW_MS - 1,
      }),
    ).toBe("silent");
  });

  it("silent when listening without speech", () => {
    expect(dotKind({ ...base, state: "listening" })).toBe("silent");
    expect(dotKind({ ...base, state: "processing" })).toBe("silent");
  });
});
