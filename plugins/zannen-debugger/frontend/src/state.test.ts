import { describe, expect, it } from "vitest";
import { devicePath, type DeviceEntry } from "./state";

function dev(partial: Partial<DeviceEntry>): DeviceEntry {
  return { id: "x", label: "X", transports: [], capabilities: [], ...partial };
}

describe("devicePath", () => {
  it("prefers serial path", () => {
    expect(
      devicePath(
        dev({ transports: ["serial", "ble"], extra: { path: "/dev/cu.usb1", address: "AA:BB" } }),
      ),
    ).toBe("/dev/cu.usb1");
  });

  it("uses ble: prefix when only a BLE address exists", () => {
    expect(devicePath(dev({ transports: ["ble"], extra: { address: "AA:BB:CC" } }))).toBe(
      "ble:AA:BB:CC",
    );
  });

  it("mock devices use the serial path", () => {
    expect(devicePath(dev({ transports: ["serial"], extra: { path: "mock://zannen-smol" } }))).toBe(
      "mock://zannen-smol",
    );
  });

  it("returns null when both are missing", () => {
    expect(devicePath(dev({ transports: ["ble"], extra: {} }))).toBeNull();
    expect(devicePath(dev({ transports: [] }))).toBeNull();
  });
});
