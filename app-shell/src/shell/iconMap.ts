/**
 * 侧边栏图标静态映射。
 *
 * lucide 的 `icons` 全量命名空间会让打包器放弃 tree-shaking（+450KB），
 * 因此插件清单可引用的图标采用显式白名单；新增图标时在此登记。
 */

import {
  Activity,
  ArrowDownToLine,
  Box,
  Bug,
  CircleArrowUp,
  Cpu,
  Languages,
  Package,
  PartyPopper,
  Puzzle,
  RefreshCw,
  Rocket,
  RotateCcw,
  Server,
  Settings,
  Sparkles,
  Stethoscope,
  Terminal,
  Wrench,
  type LucideIcon,
} from "lucide-react";

const ICON_MAP: Record<string, LucideIcon> = {
  activity: Activity,
  "arrow-down-to-line": ArrowDownToLine,
  box: Box,
  bug: Bug,
  "circle-arrow-up": CircleArrowUp,
  cpu: Cpu,
  languages: Languages,
  package: Package,
  "party-popper": PartyPopper,
  puzzle: Puzzle,
  "refresh-cw": RefreshCw,
  rocket: Rocket,
  "rotate-ccw": RotateCcw,
  server: Server,
  settings: Settings,
  sparkles: Sparkles,
  stethoscope: Stethoscope,
  terminal: Terminal,
  wrench: Wrench,
};

export function iconFor(name: string | undefined): LucideIcon {
  return (name && ICON_MAP[name]) || Puzzle;
}
