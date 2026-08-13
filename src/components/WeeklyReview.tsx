// The week in review.
//
// Two halves, deliberately separated. The numbers come from Retain's own
// database and are always shown — no key, no network, no waiting. The written
// paragraph is the only part that needs a provider, and it is generated from
// those same numbers rather than from raw data, so the model can't disagree
// with the table sitting directly above it.

import { useEffect, useState } from "react";

import { api } from "../lib/api";
import type { WeeklyFacts } from "../lib/types";
import { AiAction, AiGate, useAi } from "./Ai";
import { Card, SectionTitle } from "./ui";

function hm(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h ? `${h}h ${m}m` : `${m}m`;
}

export function WeeklyReview({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { status } = useAi();
  const [facts, setFacts] = useState<WeeklyFacts | null>(null);
  const [prose, setProse] = useState<string | null>(null);

  useEffect(() => {
    void api.weeklyFacts().then(setFacts).catch(() => setFacts(null));
  }, []);

  if (!facts) return null;

  const busiest = facts.minutesBySubject[0];
  const topError = facts.errorsByCategory[0];
  const quiet = facts.sessions < 2 && facts.errorsByCategory.length === 0;

  return (
    <section className="animate-in">
      <SectionTitle>This week</SectionTitle>
      <Card className="mt-2.5 p-5">
        <div className="flex flex-wrap gap-x-8 gap-y-3">
          <Figure label="Focused" value={hm(facts.totalMinutes)} />
          <Figure label="Sessions" value={String(facts.sessions)} />
          <Figure label="Cards reviewed" value={String(facts.cardsReviewed)} />
        </div>

        {facts.minutesBySubject.length > 0 && (
          <div className="mt-5 space-y-1.5">
            {facts.minutesBySubject.map(([name, mins]) => (
              <div key={name} className="flex items-center gap-3">
                <span className="w-[128px] shrink-0 truncate text-[12.5px] text-[var(--ink-dim)]">
                  {name}
                </span>
                <div className="h-[6px] flex-1 overflow-hidden rounded-full bg-[var(--surface-hi)]">
                  <div
                    className="h-full rounded-full bg-[var(--accent)]"
                    style={{
                      width: `${busiest ? Math.max(3, (mins / busiest[1]) * 100) : 0}%`,
                    }}
                  />
                </div>
                <span className="tabular w-[52px] shrink-0 text-right text-[12.5px] text-[var(--ink-dim)]">
                  {hm(mins)}
                </span>
              </div>
            ))}
          </div>
        )}

        {facts.untouched.length > 0 && (
          <p className="mt-4 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
            No time logged this week for{" "}
            <span className="text-[var(--ink)]">{facts.untouched.join(", ")}</span>.
          </p>
        )}

        {topError && (
          <p className="mt-1.5 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
            Most common mistake:{" "}
            <span className="text-[var(--ink)]">{topError[0]}</span> ({topError[1]}×).
          </p>
        )}

        <div className="mt-5 border-t border-[var(--line-soft)] pt-4">
          {quiet ? (
            <p className="text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
              Not much logged yet this week — the written review turns on once there's something
              to actually say.
            </p>
          ) : prose ? (
            <div className="space-y-2.5">
              {prose.split("\n").filter(Boolean).map((para, i) => (
                <p key={i} className="text-[13px] leading-[1.65] text-[var(--ink)]">
                  {para}
                </p>
              ))}
              <p className="pt-1 text-[11.5px] text-[var(--ink-faint)]">
                Written by {status?.model}. The numbers above are Retain's own.
              </p>
            </div>
          ) : (
            <AiGate
              status={status}
              what="write this week up in a paragraph — which subject you've been avoiding, and the mistake that keeps coming back"
              onOpenSettings={onOpenSettings}
            >
              <AiAction
                label="Write the review"
                run={() => api.aiWeeklyReview()}
                onDone={(r) => setProse(r.prose)}
              />
            </AiGate>
          )}
        </div>
      </Card>
    </section>
  );
}

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="tabular text-[21px] font-medium tracking-[-0.02em]">{value}</div>
      <div className="mt-0.5 text-[11.5px] text-[var(--ink-faint)]">{label}</div>
    </div>
  );
}
