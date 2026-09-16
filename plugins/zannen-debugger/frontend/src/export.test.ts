import { describe, expect, it } from "vitest";
import { emptyBuffers, pushImuSamples, pushRfSamples } from "./buffers";
import { csvCell, formatLogLines, imuCsv, rssiCsv, toCsv, withSuffix } from "./export";

describe("csvCell / toCsv", () => {
  it("plain values pass through", () => {
    expect(csvCell(42)).toBe("42");
    expect(csvCell("abc")).toBe("abc");
  });

  it("null/undefined produce empty fields", () => {
    expect(csvCell(null)).toBe("");
    expect(csvCell(undefined)).toBe("");
  });

  it("fields with comma/quote/newline are quoted with doubled quotes", () => {
    expect(csvCell("a,b")).toBe('"a,b"');
    expect(csvCell('say "hi"')).toBe('"say ""hi"""');
    expect(csvCell("x\ny")).toBe('"x\ny"');
  });

  it("toCsv outputs header + rows, LF-terminated", () => {
    expect(toCsv(["a", "b"], [[1, 2], [3, 4]])).toBe("a,b\n1,2\n3,4\n");
  });
});

describe("imuCsv / rssiCsv", () => {
  it("imu buffer serializes into fixed columns", () => {
    const b = emptyBuffers();
    pushImuSamples(b, [{ ts: 100, accel: [0.1, 0.2, 0.3], gyro: [1, 2, 3] }]);
    expect(imuCsv(b)).toBe("ts_ms,ax,ay,az,gx,gy,gz\n100,0.1,0.2,0.3,1,2,3\n");
  });

  it("rssi buffer serializes into two columns", () => {
    const b = emptyBuffers();
    pushRfSamples(b, [
      { ts: 5, rssi: -61 },
      { ts: 6, rssi: -58 },
    ]);
    expect(rssiCsv(b)).toBe("ts_ms,rssi\n5,-61\n6,-58\n");
  });

  it("empty buffer outputs header only", () => {
    expect(imuCsv(emptyBuffers())).toBe("ts_ms,ax,ay,az,gx,gy,gz\n");
  });
});

describe("withSuffix", () => {
  it("replaces the .csv suffix", () => {
    expect(withSuffix("/tmp/a.csv", ".imu.csv")).toBe("/tmp/a.imu.csv");
  });

  it("appends directly when no .csv suffix present", () => {
    expect(withSuffix("/tmp/a", ".rssi.csv")).toBe("/tmp/a.rssi.csv");
  });
});

describe("formatLogLines", () => {
  const lines = [
    { dir: "rx" as const, text: "hello", ts: "12:00:00.001" },
    { dir: "tx" as const, text: '{"cmd":"ping"}', ts: "12:00:01.002" },
    { dir: "sys" as const, text: "[session closed]", ts: "12:00:02.003" },
  ];

  it("exports with timestamps (matching on-screen display)", () => {
    expect(formatLogLines(lines, true)).toBe(
      '[12:00:00.001] RX hello\n[12:00:01.002] TX {"cmd":"ping"}\n[12:00:02.003] SYS [session closed]\n',
    );
  });

  it("omits the timestamp prefix when disabled", () => {
    expect(formatLogLines(lines, false)).toBe('RX hello\nTX {"cmd":"ping"}\nSYS [session closed]\n');
  });

  it("empty log outputs empty string", () => {
    expect(formatLogLines([], true)).toBe("");
  });
});
