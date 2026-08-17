// Calendar subscription settings.
//
// One address, one toggle, one button. The screen's job beyond that is to be
// honest about state: when it last worked, what went wrong if it didn't, and
// how many events are actually stored. A sync feature that fails silently is
// worse than none, because you plan around a timetable that stopped updating
// three weeks ago.

import { useEffect, useState } from "react";
import { AlertTriangle, Check, RefreshCw } from "lucide-react";

import { api } from "../lib/api";
import type { CalendarStatus } from "../lib/types";
import { Button, Card, SectionTitle, Toggle } from "./ui";

function whenSynced(iso: string | null): string {
  if (!iso) return "never";

  const then = new Date(iso);
  const mins = Math.round((Date.now() - then.getTime()) / 60000);

  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  if (mins < 60 * 24) return `${Math.round(mins / 60)}h ago`;
  return then.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

export function CalendarSection() {
  const [status, setStatus] = useState<CalendarStatus | null>(null);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .calendarStatus()
      .then((s) => {
        setStatus(s);
        setUrl(s.url);
      })
      .catch(() => setStatus(null));
  }, []);

  if (!status) return null;

  const save = async (enabled: boolean, address: string) => {
    setSaveError(null);
    try {
      setStatus(await api.setCalendarSettings(enabled, address));
    } catch (e) {
      setSaveError(String(e));
    }
  };

  const sync = async () => {
    setBusy(true);
    setSaveError(null);
    try {
      // Resolves even on a network failure; the error arrives inside the status.
      setStatus(await api.syncCalendar());
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const dirty = url.trim() !== status.url;

  return (
    <section>
      <SectionTitle>School calendar</SectionTitle>
      <Card className="mt-2.5 p-5">
        <Toggle
          label="Subscribe to a calendar"
          description="Compass publishes an ICS address under its calendar settings. Retain reads that address and nothing else — it never signs in to Compass."
          checked={status.enabled}
          onChange={(v) => void save(v, url)}
        />

        <div className="mt-3">
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            spellCheck={false}
            placeholder="https://…/calendar.ics"
            className="h-9 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 font-mono text-[12px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
          />

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              disabled={!dirty}
              onClick={() => void save(status.enabled, url)}
            >
              {dirty ? "Save address" : "Saved"}
            </Button>

            <Button
              size="sm"
              variant="primary"
              disabled={busy || !status.url || !status.enabled}
              onClick={() => void sync()}
            >
              <RefreshCw
                size={13}
                className={busy ? "animate-spin" : undefined}
              />
              {busy ? "Syncing…" : "Sync now"}
            </Button>

            {/* Say why the button is off rather than leaving it inert. */}
            {!status.enabled && status.url !== "" && (
              <span className="text-[12px] text-[var(--ink-faint)]">
                Turn the calendar on to sync.
              </span>
            )}
            {status.url === "" && (
              <span className="text-[12px] text-[var(--ink-faint)]">
                Paste an address above first.
              </span>
            )}

            {status.eventCount > 0 && (
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={async () => setStatus(await api.clearCalendar())}
              >
                Clear
              </Button>
            )}
          </div>
        </div>

        {/* State, stated plainly. */}
        <div className="mt-4 space-y-2 border-t border-[var(--line-soft)] pt-4">
          <div className="flex items-center gap-2 text-[12.5px] text-[var(--ink-dim)]">
            {status.lastError ? (
              <AlertTriangle
                size={13}
                className="shrink-0 text-[var(--warn)]"
              />
            ) : status.lastSyncAt ? (
              <Check
                size={13}
                className="shrink-0 text-[var(--color-positive)]"
              />
            ) : null}
            <span>
              Last sync: {whenSynced(status.lastSyncAt)} · {status.eventCount}{" "}
              {status.eventCount === 1 ? "event" : "events"} stored
            </span>
          </div>

          {status.lastError && (
            <p className="text-[12.5px] leading-relaxed text-[var(--warn)]">
              {status.lastError}
              <span className="mt-1 block text-[var(--ink-faint)]">
                Events from the last successful sync are still here — nothing
                was lost.
              </span>
            </p>
          )}

          {saveError && (
            <p className="text-[12.5px] leading-relaxed text-[var(--danger)]">
              {saveError}
            </p>
          )}

          <p className="text-[12px] leading-relaxed text-[var(--ink-faint)]">
            Syncing replaces the stored events with whatever the feed currently
            says, so a cancelled class disappears rather than lingering.
            Recurring events are expanded about a year ahead in their own
            timezone, which is what keeps a 9am class at 9am across daylight
            saving.
          </p>
        </div>
      </Card>
    </section>
  );
}
