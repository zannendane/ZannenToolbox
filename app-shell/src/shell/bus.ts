/**
 * 总线桥接：订阅宿主 `zannen-bus`，把设备生命周期事件同步进壳状态。
 * 插件视图经 SDK 的 onBus 自行订阅数据主题。
 */

import { listen } from "@tauri-apps/api/event";
import { useShellStore, type DeviceInfoDto } from "./store";

interface BusEnvelope {
  topic: string;
  payload: unknown;
}

export async function initBusBridge(): Promise<void> {
  await listen<BusEnvelope>("zannen-bus", (event) => {
    const { topic, payload } = event.payload;
    const store = useShellStore.getState();
    switch (topic) {
      case "device.found":
      case "device.update":
        store.upsertDevice(payload as DeviceInfoDto);
        break;
      case "device.lost":
        store.removeDevice((payload as { id: string }).id);
        break;
      default:
        break;
    }
  });
}
