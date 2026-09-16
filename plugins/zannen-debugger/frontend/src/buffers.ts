/**
 * 时间轴数据缓冲：按设备分桶的定长样本缓冲（纯函数，可单测）。
 *
 * 高频路径纪律：缓冲活在 React state 之外（ref 持有的普通对象），
 * 视图层经 rAF 读取本模块产出的数组驱动 uPlot。
 */

export const TIMELINE_CAPACITY = 3000;

export interface ImuSample {
  ts: number;
  accel?: [number, number, number];
  gyro?: [number, number, number];
  quat?: [number, number, number, number];
  device?: string;
}

export interface RfSample {
  ts: number;
  rssi?: number;
  device?: string;
}

export interface Buffers {
  tImu: number[];
  ax: number[];
  ay: number[];
  az: number[];
  gx: number[];
  gy: number[];
  gz: number[];
  tRf: number[];
  rssi: number[];
}

export function emptyBuffers(): Buffers {
  return { tImu: [], ax: [], ay: [], az: [], gx: [], gy: [], gz: [], tRf: [], rssi: [] };
}

/** 追加并裁剪到容量上限（丢最旧）。 */
export function pushCapped(arr: number[], values: number[], capacity = TIMELINE_CAPACITY): void {
  arr.push(...values);
  if (arr.length > capacity) arr.splice(0, arr.length - capacity);
}

export function pushImuSamples(b: Buffers, samples: ImuSample[]): void {
  const t: number[] = [], ax: number[] = [], ay: number[] = [], az: number[] = [],
    gx: number[] = [], gy: number[] = [], gz: number[] = [];
  for (const s of samples) {
    t.push(s.ts);
    ax.push(s.accel?.[0] ?? 0);
    ay.push(s.accel?.[1] ?? 0);
    az.push(s.accel?.[2] ?? 0);
    gx.push(s.gyro?.[0] ?? 0);
    gy.push(s.gyro?.[1] ?? 0);
    gz.push(s.gyro?.[2] ?? 0);
  }
  pushCapped(b.tImu, t);
  pushCapped(b.ax, ax);
  pushCapped(b.ay, ay);
  pushCapped(b.az, az);
  pushCapped(b.gx, gx);
  pushCapped(b.gy, gy);
  pushCapped(b.gz, gz);
}

export function pushRfSamples(b: Buffers, samples: RfSample[]): void {
  pushCapped(b.tRf, samples.map((s) => s.ts));
  pushCapped(b.rssi, samples.map((s) => s.rssi ?? 0));
}

/** 样本归属设备：后端已注入 label；缺失时回退到当前活跃会话。 */
export function sampleDevice(s: { device?: string }, fallback: string): string {
  return s.device ?? fallback;
}

/** 设备分桶：key 为设备 label。 */
export type BufferBuckets = Map<string, Buffers>;

export function bucketFor(buckets: BufferBuckets, device: string): Buffers {
  let b = buckets.get(device);
  if (!b) {
    b = emptyBuffers();
    buckets.set(device, b);
  }
  return b;
}
