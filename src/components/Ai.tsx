// Shared pieces for the optional AI features.
//
// The design rule these enforce: an AI feature that can't run is never an error
// state. With no API key the app is complete and correct — the AI parts simply
// aren't offered, and the space they'd occupy explains how to turn them on. Any
// screen using these should still be fully usable if you delete every key.

import { useEffect, useState, type ReactNode } from "react";
import { Sparkles } from "lucide-react";

import type { AiStatus } from "../lib/types";
import { useApp } from "../store";
import { Button, cx } from "./ui";

/**
 * Reads whether any provider key exists. `null` while still loading.
 *
 * Backed by one shared fetch in the store rather than a fetch per component.
 * That matters more than it looks: answering "is there a key?" means reading
 * the Keychain once per provider, and this hook is called from inside list
 * rows. A per-instance fetch would turn a ten-item inbox into forty Keychain
 * lookups on every render pass.
 */
export function useAi(): {
  status: AiStatus | null;
  enabled: boolean;
  reload: () => void;
} {
  const status = useApp((s) => s.ai);
  const reload = useApp((s) => s.refreshAi);

  useEffect(() => {
    if (status === null) void reload();
  }, [status, reload]);

  return { status, enabled: !!status?.provider, reload };
}

/**
 * Wraps an AI feature. Renders `children` when a key exists; otherwise renders
 * a quiet explanation instead of an error.
 */
export function AiGate({
  status,
  what,
  onOpenSettings,
  children,
}: {
  status: AiStatus | null;
  /** What this particular feature would do, in one clause. */
  what: string;
  onOpenSettings?: () => void;
  children: ReactNode;
}) {
  if (status === null) return null;
  if (status.provider) return <>{children}</>;

  return (
    <div className="rounded-[var(--r-md)] border border-dashed border-[var(--line)] px-4 py-3.5">
      <div className="flex items-center gap-2 text-[13px] font-medium text-[var(--ink)]">
        <Sparkles
          size={14}
          strokeWidth={1.9}
          className="text-[var(--ink-faint)]"
        />
        Optional
      </div>
      <p className="mt-1.5 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
        Add an API key in Settings and Retain can {what}. Everything else on
        this screen works without one.
      </p>
      {onOpenSettings && (
        <button
          onClick={onOpenSettings}
          className="mt-2.5 text-[12.5px] text-[var(--accent)] hover:underline"
        >
          Open Settings
        </button>
      )}
    </div>
  );
}

/**
 * A button that runs one AI call, with its own pending and error state.
 *
 * Errors render inline and in full — including "that model name is wrong", which
 * is the most likely failure once a provider retires a model.
 */
export function AiAction<T>({
  label,
  run,
  onDone,
  disabled,
  className,
}: {
  label: string;
  run: () => Promise<T>;
  onDone: (result: T) => void;
  disabled?: boolean;
  className?: string;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <div className={cx("flex flex-col gap-1.5", className)}>
      <Button
        variant="secondary"
        disabled={busy || disabled}
        onClick={async () => {
          setBusy(true);
          setError(null);
          try {
            onDone(await run());
          } catch (e) {
            setError(String(e));
          } finally {
            setBusy(false);
          }
        }}
      >
        <Sparkles size={13.5} strokeWidth={1.9} />
        {busy ? "Thinking…" : label}
      </Button>
      {error && (
        <p className="text-[12px] leading-relaxed text-[var(--danger)]">
          {error}
        </p>
      )}
    </div>
  );
}
