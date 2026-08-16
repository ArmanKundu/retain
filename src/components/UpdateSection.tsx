// Update check.
//
// Three states, kept distinct on purpose. "Up to date" and "couldn't check"
// look similar and mean opposite things — one is an answer, the other is the
// absence of one — so the UI never collapses them into a single reassuring
// tick.
//
// Update installs in place. The manual routine it replaces was seven steps
// ending in two that always got skipped, which is why a Downloads folder fills
// with old disk images. The install itself is in Rust; this only asks.

import { useEffect, useState } from "react";
import {
  ArrowUpRight,
  Check,
  CloudOff,
  Download,
  RefreshCw,
} from "lucide-react";

import { api } from "../lib/api";
import type { UpdateReport } from "../lib/types";
import { Button, Card, SectionTitle } from "./ui";

export function UpdateSection({ version }: { version?: string }) {
  const [report, setReport] = useState<UpdateReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);

  /**
   * Download, replace, restart.
   *
   * On success the app exits, so nothing after the await runs — an error
   * reaching the catch means the install failed and this copy is untouched.
   */
  const install = async (downloadUrl: string) => {
    setInstalling(true);
    setInstallError(null);
    try {
      await api.installUpdate(downloadUrl);
    } catch (e) {
      setInstallError(String(e));
      setInstalling(false);
    }
  };

  useEffect(() => {
    // Cached only — no network on mount, so this renders instantly offline.
    void api
      .updateStatus()
      .then(setReport)
      .catch(() => setReport(null));
  }, []);

  const check = async () => {
    setBusy(true);
    try {
      setReport(await api.checkForUpdates());
    } catch {
      // The command records failures in the report itself; a rejected promise
      // here would be a bug rather than an offline machine.
    } finally {
      setBusy(false);
    }
  };

  const s = report?.status;

  return (
    <section>
      <SectionTitle>Updates</SectionTitle>
      <Card className="mt-2.5 p-5">
        <div className="flex items-start gap-3">
          <div className="mt-[2px] shrink-0">
            {s?.status === "available" ? (
              <ArrowUpRight size={15} className="text-[var(--accent)]" />
            ) : s?.status === "upToDate" ? (
              <Check size={15} className="text-[var(--color-positive)]" />
            ) : (
              <CloudOff size={15} className="text-[var(--ink-faint)]" />
            )}
          </div>

          <div className="min-w-0 flex-1">
            {s?.status === "available" ? (
              <>
                <div className="text-[14px]">
                  Version {s.latest} is available
                </div>
                <div className="mt-0.5 text-[12.5px] text-[var(--ink-dim)]">
                  You're on {s.current}.
                </div>
                {s.notes && (
                  <p className="selectable mt-2.5 max-h-32 overflow-y-auto whitespace-pre-wrap rounded-[var(--r-sm)] border border-[var(--line-soft)] p-2.5 text-[12px] leading-relaxed text-[var(--ink-dim)]">
                    {s.notes}
                  </p>
                )}
              </>
            ) : s?.status === "upToDate" ? (
              <>
                <div className="text-[14px]">Retain is up to date</div>
                <div className="mt-0.5 text-[12.5px] text-[var(--ink-dim)]">
                  Version {s.current}.
                </div>
              </>
            ) : (
              <>
                <div className="text-[14px]">Couldn't check for updates</div>
                <div className="mt-0.5 text-[12.5px] text-[var(--ink-dim)]">
                  {s?.reason ?? "Not checked yet."} You're on{" "}
                  {s?.current ?? version}. This isn't a problem — Retain doesn't
                  need GitHub to run.
                </div>
              </>
            )}

            <div className="mt-3 flex flex-wrap items-center gap-2">
              <Button size="sm" disabled={busy} onClick={() => void check()}>
                <RefreshCw
                  size={13}
                  className={busy ? "animate-spin" : undefined}
                />
                {busy ? "Checking…" : "Check now"}
              </Button>

              {s?.status === "available" && s.downloadUrl && (
                <Button
                  size="sm"
                  variant="primary"
                  disabled={installing}
                  onClick={() => void install(s.downloadUrl!)}
                >
                  <Download size={13} />
                  {installing ? "Installing…" : `Update to ${s.latest}`}
                </Button>
              )}

              {s?.status === "available" && (
                <Button
                  size="sm"
                  onClick={() => void api.openReleasePage(s.url)}
                >
                  Release notes
                </Button>
              )}

              {report?.checkedAt && (
                <span className="text-[11.5px] text-[var(--ink-faint)]">
                  Last checked {new Date(report.checkedAt).toLocaleDateString()}
                </span>
              )}
            </div>
          </div>
        </div>

        {installError && (
          <p className="mt-3 text-[12.5px] leading-relaxed text-[var(--danger)]">
            {installError}
          </p>
        )}

        <p className="mt-4 border-t border-[var(--line-soft)] pt-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          Retain checks GitHub for a newer release about once a day. Nothing
          about you or this Mac is sent. Pressing Update downloads that
          release's disk image, replaces this copy of the app with it, throws
          the image away and restarts — the same steps you were doing by hand.
          It only ever installs from Retain's own GitHub releases, and only when
          Retain is running from an Applications folder.
        </p>
      </Card>
    </section>
  );
}
