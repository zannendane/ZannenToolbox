/**
 * 共享 UI 基元：玻璃拟态卡片、按钮、徽标、开关、进度条等。
 *
 * 样式类定义在壳的全局 CSS（`zt-*`），插件与壳天然视觉一致；
 * 插件自有样式仅用于模块特定视图（终端、3D 画布等）。
 */

import { motion, type HTMLMotionProps } from "framer-motion";
import { forwardRef, type ReactNode } from "react";

/** 玻璃拟态卡片。 */
export function GlassCard({
  children,
  className = "",
  padded = true,
  ...rest
}: { children: ReactNode; className?: string; padded?: boolean } & HTMLMotionProps<"div">) {
  return (
    <motion.div
      className={`zt-card${padded ? " zt-card-pad" : ""} ${className}`}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      {...rest}
    >
      {children}
    </motion.div>
  );
}

/** 分节标题。 */
export function SectionTitle({ title, desc }: { title: string; desc?: string }) {
  return (
    <div className="zt-section-title">
      <h2>{title}</h2>
      {desc ? <p>{desc}</p> : null}
    </div>
  );
}

/** 按钮（扁平 + 微交互）。 */
export const Button = forwardRef<
  HTMLButtonElement,
  {
    children: ReactNode;
    variant?: "primary" | "ghost" | "danger";
    disabled?: boolean;
    onClick?: () => void;
    className?: string;
    title?: string;
  }
>(function Button({ children, variant = "ghost", disabled, onClick, className = "", title }, ref) {
  return (
    <motion.button
      ref={ref}
      className={`zt-btn zt-btn-${variant} ${className}`}
      whileHover={disabled ? undefined : { scale: 1.03 }}
      whileTap={disabled ? undefined : { scale: 0.96 }}
      transition={{ type: "spring", stiffness: 500, damping: 28 }}
      disabled={disabled}
      onClick={onClick}
      title={title}
    >
      {children}
    </motion.button>
  );
});

/** 状态徽标。 */
export function Badge({
  children,
  tone = "muted",
}: {
  children: ReactNode;
  tone?: "ok" | "warn" | "critical" | "muted" | "accent";
}) {
  return <span className={`zt-badge zt-badge-${tone}`}>{children}</span>;
}

/** 开关。 */
export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label ?? "toggle"}
      className={`zt-toggle${checked ? " is-on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <motion.span
        className="zt-toggle-thumb"
        animate={{ x: checked ? 16 : 0 }}
        transition={{ type: "spring", stiffness: 600, damping: 32 }}
      />
      {label ? <span className="zt-toggle-label">{label}</span> : null}
    </button>
  );
}

/** 进度条（determinate；indeterminate 传 null）。 */
export function Progress({ percent }: { percent: number | null }) {
  return (
    <div className={`zt-progress${percent === null ? " is-indeterminate" : ""}`}>
      <motion.div
        className="zt-progress-bar"
        animate={{ width: percent === null ? "40%" : `${Math.min(100, Math.max(0, percent))}%` }}
        transition={{ duration: 0.2 }}
      />
    </div>
  );
}

/** 加载指示。 */
export function Spinner({ size = 18 }: { size?: number }) {
  return <span className="zt-spinner" style={{ width: size, height: size }} aria-label="Loading" />;
}

/** 空状态占位。 */
export function EmptyState({ icon, title, desc }: { icon?: ReactNode; title: string; desc?: string }) {
  return (
    <div className="zt-empty">
      {icon ? <div className="zt-empty-icon">{icon}</div> : null}
      <div className="zt-empty-title">{title}</div>
      {desc ? <div className="zt-empty-desc">{desc}</div> : null}
    </div>
  );
}

/** 分段选择器。 */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="zt-segmented">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          className={`zt-segmented-item${opt.value === value ? " is-active" : ""}`}
          onClick={() => onChange(opt.value)}
        >
          {opt.value === value ? (
            <motion.span className="zt-segmented-thumb" layoutId={undefined} />
          ) : null}
          {opt.label}
        </button>
      ))}
    </div>
  );
}
