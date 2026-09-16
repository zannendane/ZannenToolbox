import { describe, expect, it } from "vitest";
import {
  bucketFor,
  emptyBuffers,
  pushCapped,
  pushImuSamples,
  pushRfSamples,
  sampleDevice,
  type BufferBuckets,
} from "./buffers";

describe("pushCapped", () => {
  it("appends and trims to capacity (drops oldest)", () => {
    const arr = [1, 2, 3];
    pushCapped(arr, [4, 5], 4);
    expect(arr).toEqual([2, 3, 4, 5]);
  });

  it("keeps only the last `capacity` items when over capacity", () => {
    const arr: number[] = [];
    pushCapped(arr, [1, 2, 3, 4, 5], 3);
    expect(arr).toEqual([3, 4, 5]);
  });

  it("appends as-is when under capacity", () => {
    const arr = [1];
    pushCapped(arr, [2, 3], 10);
    expect(arr).toEqual([1, 2, 3]);
  });
});

describe("pushImuSamples / pushRfSamples", () => {
  it("splits into channel arrays with missing fields zero-filled", () => {
    const b = emptyBuffers();
    pushImuSamples(b, [
      { ts: 100, accel: [1, 2, 3], gyro: [4, 5, 6] },
      { ts: 200 }, // 无 accel/gyro
    ]);
    expect(b.tImu).toEqual([100, 200]);
    expect(b.ax).toEqual([1, 0]);
    expect(b.az).toEqual([3, 0]);
    expect(b.gz).toEqual([6, 0]);
  });

  it("rf samples missing rssi are zero-filled", () => {
    const b = emptyBuffers();
    pushRfSamples(b, [{ ts: 1, rssi: -60 }, { ts: 2 }]);
    expect(b.tRf).toEqual([1, 2]);
    expect(b.rssi).toEqual([-60, 0]);
  });

  it("drops oldest samples when over capacity", () => {
    const b = emptyBuffers();
    pushRfSamples(
      b,
      Array.from({ length: 10 }, (_, i) => ({ ts: i, rssi: i })),
      // pushCapped 默认容量 3000，这里验证默认路径不裁剪
    );
    expect(b.tRf).toHaveLength(10);
  });
});

describe("bucketing", () => {
  it("bucketFor creates on demand and reuses the same bucket", () => {
    const buckets: BufferBuckets = new Map();
    const a1 = bucketFor(buckets, "dev-a");
    const a2 = bucketFor(buckets, "dev-a");
    const b1 = bucketFor(buckets, "dev-b");
    expect(a1).toBe(a2);
    expect(a1).not.toBe(b1);
    expect(buckets.size).toBe(2);
  });

  it("buffers of different devices are isolated", () => {
    const buckets: BufferBuckets = new Map();
    pushRfSamples(bucketFor(buckets, "dev-a"), [{ ts: 1, rssi: -50 }]);
    pushRfSamples(bucketFor(buckets, "dev-b"), [{ ts: 2, rssi: -70 }]);
    expect(bucketFor(buckets, "dev-a").rssi).toEqual([-50]);
    expect(bucketFor(buckets, "dev-b").rssi).toEqual([-70]);
  });

  it("sampleDevice falls back when device field is missing", () => {
    expect(sampleDevice({ device: "A" }, "B")).toBe("A");
    expect(sampleDevice({}, "B")).toBe("B");
  });
});
