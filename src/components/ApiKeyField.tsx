import { useState } from "react";
import { AlertTriangle, Check, Loader2, WifiOff, X } from "lucide-react";

import { api } from "../lib/api";
import type { KeyCheck, Provider } from "../lib/types";
import { Button, cx } from "./ui";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "result"; check: KeyCheck };

/**
 * Paste a key, have it checked with the provider, keep it only if it works.
 *
 * The three outcomes are surfaced differently on purpose:
 *
 *   valid       → saved, input cleared immediately
 *   invalid     → not saved, input KEPT so a typo can be fixed without retyping
 *   unreachable → not saved, but offered as an explicit "save without checking"
 *
 * That last branch is the one that matters for a tool you might set up on a
 * train: a good key must not be refused because the network was down. What it
 * must never do is save silently and pretend it was verified.
 */
export function ApiKeyField({
  provider,
  label,
  present,
  onChange,
}: {
  provider: Provider;
  label: string;
  present: boolean;
  onChange: () => Promise<void>;
}) {
  const [key, setKey] = useState("");
  const [state, setState] = useState<State>({ kind: "idle" });
  const [editing, setEditing] = useState(false);

  const verify = async () => {
    setState({ kind: "checking" });
    try {
      const check = await api.secretVerifyAndStore(provider, key);
      setState({ kind: "result", check });
      if (check.status === "valid") {
        // Drop the plaintext from component state the instant it's in the
        // Keychain. On the other two branches it stays so the user can correct
        // it — nothing is written to disk on this side either way.
        setKey("");
        setEditing(false);
        await onChange();
      }
    } catch (e) {
      setState({ kind: "result", check: { status: "unreachable", message: String(e) } });
    }
  };

  const saveAnyway = async () => {
    try {
      await api.secretStoreUnverified(provider, key);
      setKey("");
      setEditing(false);
      setState({ kind: "idle" });
      await onChange();
    } catch (e) {
      setState({ kind: "result", check: { status: "invalid", message: String(e) } });
    }
  };

  const testStored = async () => {
    setState({ kind: "checking" });
    try {
      setState({ kind: "result", check: await api.secretTestStored(provider) });
    } catch (e) {
      setState({ kind: "result", check: { status: "unreachable", message: String(e) } });
    }
  };

  const remove = async () => {
    await api.secretDelete(provider);
    setState({ kind: "idle" });
    await onChange();
  };

  return (
    <div className="px-5 py-3.5">
      <div className="flex items-center gap-3">
        <span className="flex-1 text-[14px]">{label}</span>

        {present ? (
          <>
            <span className="flex items-center gap-1.5 text-[12.5px] text-[var(--color-positive)]">
              <Check size={14} />
              In Keychain
            </span>
            <Button size="sm" variant="ghost" onClick={testStored} disabled={state.kind === "checking"}>
              {state.kind === "checking" ? "Checking…" : "Test"}
            </Button>
            <Button size="sm" variant="ghost" onClick={remove}>
              Remove
            </Button>
          </>
        ) : editing ? null : (
          <Button size="sm" variant="ghost" onClick={() => setEditing(true)}>
            Add key
          </Button>
        )}
      </div>

      {editing && !present && (
        <div className="animate-in mt-3 flex gap-2">
          <input
            autoFocus
            type="password"
            value={key}
            placeholder="Paste your key"
            onChange={(e) => {
              setKey(e.target.value);
              if (state.kind === "result") setState({ kind: "idle" });
            }}
            onKeyDown={(e) => e.key === "Enter" && key.trim() && void verify()}
            className="h-8 flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13px] text-[var(--ink)]"
          />
          <Button size="sm" onClick={verify} disabled={!key.trim() || state.kind === "checking"}>
            {state.kind === "checking" ? (
              <>
                <Loader2 size={13} className="animate-spin" />
                Checking
              </>
            ) : (
              "Verify & save"
            )}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              setKey("");
              setEditing(false);
              setState({ kind: "idle" });
            }}
          >
            Cancel
          </Button>
        </div>
      )}

      {state.kind === "checking" && !editing && (
        <div className="mt-2.5 flex items-center gap-1.5 text-[12.5px] text-[var(--ink-faint)]">
          <Loader2 size={13} className="animate-spin" />
          Asking {label}…
        </div>
      )}

      {state.kind === "result" && <Outcome check={state.check} onSaveAnyway={saveAnyway} />}
    </div>
  );
}

function Outcome({
  check,
  onSaveAnyway,
}: {
  check: KeyCheck;
  onSaveAnyway: () => Promise<void>;
}) {
  if (check.status === "valid") {
    return (
      <div className="animate-in mt-2.5 flex items-start gap-1.5 text-[12.5px] leading-relaxed text-[var(--color-positive)]">
        <Check size={14} className="mt-0.5 shrink-0" />
        <span>{check.detail ?? "Verified and saved to your Keychain."}</span>
      </div>
    );
  }

  if (check.status === "invalid") {
    return (
      <div className="animate-in mt-2.5 flex items-start gap-1.5 text-[12.5px] leading-relaxed text-[var(--danger)]">
        <X size={14} className="mt-0.5 shrink-0" />
        <span>{check.message}</span>
      </div>
    );
  }

  return (
    <div
      className={cx(
        "animate-in mt-2.5 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--warn)_35%,transparent)] bg-[color-mix(in_srgb,var(--warn)_10%,transparent)] p-3",
      )}
    >
      <div className="flex items-start gap-1.5 text-[12.5px] leading-relaxed text-[var(--ink)]">
        <WifiOff size={14} className="mt-0.5 shrink-0 text-[var(--warn)]" />
        <span>{check.message}</span>
      </div>
      <div className="mt-2.5 flex items-center gap-2">
        <Button size="sm" onClick={onSaveAnyway}>
          Save without checking
        </Button>
        <span className="flex items-center gap-1 text-[11.5px] text-[var(--ink-faint)]">
          <AlertTriangle size={11} />
          Retain couldn't confirm this one works
        </span>
      </div>
    </div>
  );
}
