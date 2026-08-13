// Retain's shared visual primitives.
//
// These exist because the audit found the same handful of shapes rebuilt with
// slightly different padding, radius and shadow on every screen. Each component
// here replaces a pattern that already appeared at least three times; nothing
// speculative lives in this file.
//
// The rule they encode: geometry and elevation come from tokens, never from
// literal Tailwind values. If a screen needs a radius that isn't --r-sm/md/lg/xl,
// that's a signal the design is drifting, not that a new value is needed.

import type { CSSProperties, ReactNode } from "react";

import { cx } from "./ui";

// ---------------------------------------------------------------------------
// Surfaces
// ---------------------------------------------------------------------------

type Elevation = "flat" | "raised" | "floating";

/**
 * A panel. Three elevations, matching the token scale.
 *
 * `flat` sits *in* the background (sections, list containers), `raised` sits on
 * it (cards), `floating` hovers above it with glass and blur (docks, command
 * bars, modals). Most things should be flat — the audit's "cards inside cards"
 * problem came from treating raised as the default.
 */
export function Surface({
  as: Tag = "div",
  elevation = "flat",
  className,
  children,
  ...rest
}: {
  as?: "div" | "section" | "aside";
  elevation?: Elevation;
  className?: string;
  children: ReactNode;
} & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <Tag
      className={cx(
        elevation === "floating" ? "glass" : elevation === "raised" ? "surface-2" : "surface-1",
        elevation === "floating" && "rounded-[var(--r-xl)]",
        className,
      )}
      {...rest}
    >
      {children}
    </Tag>
  );
}

/**
 * A section heading with optional trailing controls.
 *
 * Sentence case, not uppercase. The old `SectionTitle` was uppercase-tracked
 * everywhere, which at this density reads as shouting and flattens hierarchy —
 * everything looked equally important.
 */
export function SectionHeader({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children?: ReactNode;
}) {
  return (
    <div className="mb-3 flex items-baseline gap-3">
      <h2 className="text-[15px] font-semibold tracking-[-0.01em] text-[var(--ink)]">{title}</h2>
      {hint && <span className="text-[12.5px] text-[var(--ink-faint)]">{hint}</span>}
      {children && <div className="ml-auto flex items-center gap-2">{children}</div>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Data display
// ---------------------------------------------------------------------------

/**
 * A number and its label — instrumentation, not a spreadsheet cell.
 *
 * The value dominates and the label is quiet and small; that ratio is what
 * makes a figure read as a reading off an instrument rather than a table entry.
 */
export function Metric({
  value,
  label,
  size = "md",
  accent,
  icon,
}: {
  value: ReactNode;
  label: string;
  size?: "sm" | "md" | "lg" | "hero";
  /** Tints the value only. Used sparingly — a streak, an urgent countdown. */
  accent?: string;
  icon?: ReactNode;
}) {
  const scale = {
    sm: "text-[19px]",
    md: "text-[26px]",
    lg: "text-[38px]",
    hero: "text-[clamp(44px,7vw,68px)]",
  }[size];

  return (
    <div className="min-w-0">
      <div
        className={cx(
          "tabular flex items-center gap-2 font-medium leading-none tracking-[-0.03em]",
          scale,
        )}
        style={accent ? { color: accent } : undefined}
      >
        {icon}
        <span className="truncate">{value}</span>
      </div>
      <div className="mt-2 truncate text-[12px] text-[var(--ink-faint)]">{label}</div>
    </div>
  );
}

/**
 * A subject's colour, name, or both.
 *
 * Subject colours are the only hues in the app besides the accent, which is
 * what lets them carry meaning. This keeps their treatment identical wherever
 * they appear.
 */
export function SubjectPill({
  name,
  colour,
  size = "md",
  dotOnly = false,
}: {
  name: string;
  colour: string;
  size?: "sm" | "md";
  dotOnly?: boolean;
}) {
  const dot = (
    <span
      className="shrink-0 rounded-full"
      style={{
        width: size === "sm" ? 6 : 7,
        height: size === "sm" ? 6 : 7,
        background: colour,
        // A faint halo so the dot reads on both light and dark surfaces.
        boxShadow: `0 0 0 3px color-mix(in srgb, ${colour} 16%, transparent)`,
      }}
    />
  );

  if (dotOnly) return dot;

  return (
    <span
      className={cx(
        "inline-flex min-w-0 items-center gap-2 rounded-full border",
        size === "sm" ? "px-2 py-0.5 text-[11.5px]" : "px-2.5 py-1 text-[12.5px]",
      )}
      style={{
        borderColor: `color-mix(in srgb, ${colour} 24%, transparent)`,
        background: `color-mix(in srgb, ${colour} 9%, transparent)`,
        color: "var(--ink-dim)",
      }}
    >
      {dot}
      <span className="truncate">{name}</span>
    </span>
  );
}

/**
 * A progress ring, in the Apple Health register rather than a web progress bar.
 *
 * The track is drawn at low opacity of the same colour rather than grey, so a
 * ring at 5% still reads as belonging to its subject.
 */
export function ProgressRing({
  value,
  size = 76,
  stroke = 7,
  colour = "var(--accent)",
  children,
}: {
  /** 0–1. Values above 1 are clamped; the ring never over-fills. */
  value: number;
  size?: number;
  stroke?: number;
  colour?: string;
  children?: ReactNode;
}) {
  const clamped = Math.max(0, Math.min(1, value));
  const r = (size - stroke) / 2;
  const circumference = 2 * Math.PI * r;

  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90" aria-hidden>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          stroke={colour}
          opacity={0.14}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          stroke={colour}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - clamped)}
          style={{ transition: "stroke-dashoffset 600ms var(--ease)" }}
        />
      </svg>
      {children && (
        <div className="absolute inset-0 flex flex-col items-center justify-center">{children}</div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Floating
// ---------------------------------------------------------------------------

/**
 * A floating glass bar, anchored to the bottom of its positioned parent.
 *
 * Used by the focus dock. Deliberately `sticky` rather than `fixed`: the dock
 * belongs to the content column, so it shouldn't sit over the sidebar.
 */
export function FloatingDock({
  className,
  children,
  visible = true,
}: {
  className?: string;
  children: ReactNode;
  visible?: boolean;
}) {
  return (
    <div
      className={cx(
        "pointer-events-none sticky bottom-5 z-30 flex justify-center px-6",
        "transition-all duration-[220ms] ease-[var(--ease)]",
        visible ? "translate-y-0 opacity-100" : "pointer-events-none translate-y-3 opacity-0",
      )}
      aria-hidden={!visible}
    >
      <div
        className={cx(
          "glass pointer-events-auto flex items-center gap-4 rounded-[var(--r-xl)] px-4 py-3",
          className,
        )}
      >
        {children}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------

/**
 * A compact pill control — filters, segmented choices, tags.
 *
 * Selection is a tinted fill plus a slightly stronger border, not a saturated
 * blue block. The point is a selected *object*, not a web-app button.
 */
export function Chip({
  active = false,
  onClick,
  children,
  colour,
  className,
  title,
}: {
  active?: boolean;
  onClick?: () => void;
  children: ReactNode;
  /** Tints the active state to a subject's colour. */
  colour?: string;
  className?: string;
  title?: string;
}) {
  const tint = colour ?? "var(--accent)";

  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-pressed={onClick ? active : undefined}
      className={cx(
        "pressable inline-flex min-w-0 items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px]",
        active
          ? "text-[var(--ink)]"
          : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)] hover:text-[var(--ink)]",
        className,
      )}
      style={
        active
          ? {
              borderColor: `color-mix(in srgb, ${tint} 38%, transparent)`,
              background: `color-mix(in srgb, ${tint} 14%, transparent)`,
            }
          : undefined
      }
    >
      {children}
    </button>
  );
}

/**
 * A keyboard hint. Small, monospaced, quiet.
 *
 * Shortcuts should be discoverable in place rather than hidden in a help
 * screen, which only works if showing one is cheap.
 */
export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={cx(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-[7px] px-1.5",
        "border border-[var(--line)] bg-[var(--surface-hi)] font-mono text-[10.5px] leading-none",
        "text-[var(--ink-faint)]",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/**
 * A hairline separator. Fades at both ends so it doesn't box content in.
 */
export function Divider({ className, style }: { className?: string; style?: CSSProperties }) {
  return (
    <div
      className={cx("h-px w-full", className)}
      style={{
        background:
          "linear-gradient(90deg, transparent, var(--line-soft) 12%, var(--line-soft) 88%, transparent)",
        ...style,
      }}
    />
  );
}
