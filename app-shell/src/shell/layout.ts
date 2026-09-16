/**
 * 窗口布局尺寸（前端单一来源）。
 *
 * 主菜单窄竖 / 子级工作区两套逻辑尺寸，扩缩保持窗口中心不动。
 * 注意：tauri.conf.json 的初始窗口尺寸（420×840）为静态配置，调整时须同步。
 */

export const HOME_WINDOW = { width: 420, height: 840 } as const;
export const PLUGIN_WINDOW = { width: 1040, height: 720 } as const;
