// Shared UI primitives.
//
// Hand-rolled rather than pulled from a component library. Every kit ships with
// a look, and that look is exactly the "Bootstrap dashboard" the brief rules out.
// These are small enough to keep honest.

import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
} from "react";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

// ---------------------------------------------------------------------------

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
};

export function Button({
  variant = "secondary",
  size = "md",
  className,
  ...props
}: ButtonProps) {
  const sizes = {
    sm: "h-[30px] px-3 text-[12.5px] rounded-[var(--r-sm)]",
    md: "h-9 px-4 text-[13px] rounded-[var(--r-md)]",
    lg: "h-11 px-6 text-[14.5px] rounded-[var(--r-md)]",
  };

  // Primary carries a contact shadow so it reads as a raised object; the others
  // stay flat until hovered. Only one thing on a screen should look pressable
  // before you touch it.
  const variants = {
    primary:
      "bg-[var(--accent)] text-white shadow-[var(--e-sm)] hover:brightness-[1.07] hover:shadow-[var(--e-md)]",
    secondary:
      "bg-[var(--surface-hi)] text-[var(--ink)] border border-[var(--line)] hover:border-[var(--ink-faint)] hover:shadow-[var(--e-sm)]",
    ghost:
      "text-[var(--ink-dim)] hover:text-[var(--ink)] hover:bg-[var(--surface-hi)]",
    danger:
      "bg-transparent text-[var(--danger)] border border-[var(--line)] hover:border-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_8%,transparent)]",
  };

  return (
    <button
      className={cx(
        "pressable inline-flex items-center justify-center gap-2 font-medium",
        // `pressable` carries the transition and the 0.98 press scale — the
        // small physical give AppKit buttons have.
        "focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--canvas)]",
        "disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap select-none",
        sizes[size],
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}

// ---------------------------------------------------------------------------

/**
 * A panel.
 *
 * `flat` is the default and has no shadow: it sits *in* the background rather
 * than on it. That's what stops nested cards from stacking shadow on shadow,
 * which was the main source of visual noise before this pass.
 */
/**
 * A surface. Publishes its own radius as `--r-parent` so anything nested
 * inside can size its corners concentrically — inner radius is the outer
 * radius minus the padding between them, and matching radii (which is what
 * picking two values off a scale gives you) leaves the inner corner
 * visibly too round.
 */
export function Card({
  children,
  className,
  elevation = "flat",
  ...rest
}: {
  children: ReactNode;
  className?: string;
  elevation?: "flat" | "raised";
} & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cx(
        "rounded-[var(--r-lg)] border border-[var(--line-soft)]",
        elevation === "raised"
          ? "bg-[var(--surface)] shadow-[var(--e-md)]"
          : "bg-[color-mix(in_srgb,var(--surface)_88%,transparent)]",
        className,
      )}
      {...rest}
      // After the spread, or a caller passing `style` would drop it — and this
      // is what `.concentric` children read.
      style={{ ["--r-parent" as string]: "var(--r-lg)", ...rest.style }}
    >
      {children}
    </div>
  );
}

/**
 * A small section label.
 *
 * Sentence case at 13px, not uppercase-tracked. Uppercase everywhere flattened
 * the hierarchy — when every label shouts, none of them rank.
 */
export function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="text-[13px] font-semibold tracking-[var(--track-body)] text-[var(--ink-dim)]">
      {children}
    </h2>
  );
}

// ---------------------------------------------------------------------------

export function TextField({
  label,
  hint,
  className,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label?: string; hint?: string }) {
  return (
    <label className="block">
      {label && (
        <span className="mb-1.5 block text-[13px] font-medium text-[var(--ink-dim)]">
          {label}
        </span>
      )}
      <input
        className={cx(
          "h-10 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3",
          "text-[14px] text-[var(--ink)] placeholder:text-[var(--ink-faint)]",
          "transition-colors duration-[120ms] focus:border-[var(--accent)]",
          className,
        )}
        {...props}
      />
      {hint && (
        <span className="mt-1.5 block text-[12px] text-[var(--ink-faint)]">
          {hint}
        </span>
      )}
    </label>
  );
}

// ---------------------------------------------------------------------------

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  size = "md",
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
  size?: "sm" | "md";
}) {
  return (
    <div
      role="tablist"
      className="inline-flex gap-0.5 rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface-hi)] p-0.5"
    >
      {options.map((o) => {
        const selected = o.value === value;
        return (
          <button
            key={o.value}
            role="tab"
            aria-selected={selected}
            onClick={() => onChange(o.value)}
            className={cx(
              "rounded-[var(--r-sm)] font-medium transition-all duration-[140ms]",
              size === "sm"
                ? "h-6 px-2.5 text-[12px]"
                : "h-8 px-3.5 text-[13px]",
              selected
                ? "bg-[var(--surface)] text-[var(--ink)] shadow-[var(--e-sm)]"
                : "text-[var(--ink-faint)] hover:text-[var(--ink-dim)]",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------

export function Toggle({
  checked,
  onChange,
  label,
  description,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: string;
}) {
  return (
    <div className="flex items-start justify-between gap-6 py-3">
      <div className="min-w-0">
        <div className="text-[14px] text-[var(--ink)]">{label}</div>
        {description && (
          <div className="mt-0.5 text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
            {description}
          </div>
        )}
      </div>
      <button
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cx(
          "relative mt-0.5 h-[26px] w-[44px] shrink-0 rounded-full transition-colors duration-200",
          checked ? "bg-[var(--accent)]" : "bg-[var(--line)]",
        )}
      >
        <span
          className={cx(
            "absolute top-[3px] h-5 w-5 rounded-full bg-white shadow transition-transform duration-200 ease-[var(--ease-out-soft)]",
            checked ? "translate-x-[21px]" : "translate-x-[3px]",
          )}
        />
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------

export function ColourDot({
  colour,
  size = 10,
}: {
  colour: string;
  size?: number;
}) {
  return (
    <span
      className="inline-block shrink-0 rounded-full"
      style={{ background: colour, width: size, height: size }}
    />
  );
}

export function Empty({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-col items-center justify-center px-6 py-14 text-center">
      <div className="text-[14px] font-medium text-[var(--ink-dim)]">
        {title}
      </div>
      <div className="mt-1.5 max-w-[380px] text-[13px] leading-relaxed text-[var(--ink-faint)]">
        {body}
      </div>
    </div>
  );
}
