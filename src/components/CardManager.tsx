// The cards in a deck, and what you can do to them.
//
// Until now a card, once imported, was permanent. You could answer it and
// nothing else — no delete, no edit, no way to take one out of rotation. That
// is the wrong shape for the thing it holds: half of what makes spaced
// repetition work is *fixing the cards*, and a card you can't fix is one you
// keep failing forever.
//
// Four actions, and the reason there are four rather than one:
//
//   Edit      the usual answer. A card you keep getting wrong is usually
//             ambiguous rather than hard. Rewriting it keeps the schedule,
//             because you still know roughly what you knew.
//   Suspend   for a card you can't fix now. Deleting loses the wording and
//             the history; leaving it in means meeting it every day.
//   Reset     for one that is genuinely lost. Starts the schedule over,
//             keeps what you actually did.
//   Delete    for one that shouldn't exist. Takes its review log too.

import { useCallback, useEffect, useState } from "react";
import { EyeOff, Pencil, RotateCcw, Trash2, TriangleAlert } from "lucide-react";

import { api } from "../lib/api";
import type { CardRow } from "../lib/types";
import { Button, cx } from "./ui";

/** Leech threshold — Anki's, and it holds up. Mirrors `mastery.rs`. */
const LEECH_LAPSES = 8;

export function CardManager({
  subjectId,
  topicId,
  onChanged,
}: {
  subjectId: number;
  topicId: number | null;
  /** Fired after anything that changes the deck's counts. */
  onChanged: () => void;
}) {
  const [cards, setCards] = useState<CardRow[]>([]);
  const [editing, setEditing] = useState<number | null>(null);
  const [confirming, setConfirming] = useState<number | null>(null);

  const load = useCallback(async () => {
    setCards(await api.listCards(subjectId, topicId).catch(() => []));
  }, [subjectId, topicId]);

  useEffect(() => {
    void load();
  }, [load]);

  if (cards.length === 0) {
    return (
      <p className="px-1 py-6 text-center text-[13px] text-[var(--ink-faint)]">
        No cards in this deck yet.
      </p>
    );
  }

  const leeches = cards.filter((c) => c.lapses >= LEECH_LAPSES).length;

  return (
    <div>
      {leeches > 0 && (
        <div className="mb-3 flex items-start gap-2 rounded-[var(--r-sm)] border border-[var(--warn)]/30 bg-[var(--warn)]/8 px-3 py-2.5">
          <TriangleAlert
            size={13}
            className="mt-0.5 shrink-0 text-[var(--warn)]"
          />
          <p className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
            {leeches} {leeches === 1 ? "card is" : "cards are"} at the top of
            this list because you keep forgetting{" "}
            {leeches === 1 ? "it" : "them"}. That usually means the wording is
            ambiguous rather than the content hard — edit before you repeat.
          </p>
        </div>
      )}

      <ul className="space-y-1">
        {cards.map((card) =>
          editing === card.id ? (
            <CardEditor
              key={card.id}
              card={card}
              onCancel={() => setEditing(null)}
              onSaved={async () => {
                setEditing(null);
                await load();
              }}
            />
          ) : (
            <li
              key={card.id}
              className={cx(
                "group flex items-start gap-3 rounded-[var(--r-md)] border px-3.5 py-2.5",
                card.suspended
                  ? "border-transparent opacity-55"
                  : "border-[var(--line-soft)] bg-[var(--surface)]",
              )}
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13.5px] text-[var(--ink)]">
                  {card.front}
                </div>
                <div className="mt-0.5 truncate text-[12.5px] text-[var(--ink-dim)]">
                  {card.back}
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-[var(--ink-faint)]">
                  {card.suspended && <span>Suspended</span>}
                  {card.topicName && <span>{card.topicName}</span>}
                  {/* Stability, not "percent correct". It's the number that
                      says whether the card will survive to next fortnight. */}
                  {card.stability !== null && (
                    <span>{Math.round(card.stability)}d memory</span>
                  )}
                  {card.reps > 0 && <span>{card.reps} reviews</span>}
                  {card.lapses > 0 && (
                    <span
                      className={
                        card.lapses >= LEECH_LAPSES
                          ? "text-[var(--warn)]"
                          : undefined
                      }
                    >
                      {card.lapses} forgotten
                    </span>
                  )}
                </div>
              </div>

              <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-[var(--t-fast)] group-hover:opacity-100 focus-within:opacity-100">
                <Action label="Edit" onClick={() => setEditing(card.id)}>
                  <Pencil size={13} />
                </Action>
                <Action
                  label={
                    card.suspended
                      ? "Put back in rotation"
                      : "Take out of rotation"
                  }
                  onClick={async () => {
                    await api.suspendCard(card.id, !card.suspended);
                    await load();
                    onChanged();
                  }}
                >
                  <EyeOff size={13} />
                </Action>
                <Action
                  label="Start this card over"
                  onClick={async () => {
                    await api.resetCard(card.id);
                    await load();
                    onChanged();
                  }}
                >
                  <RotateCcw size={13} />
                </Action>
                {/* Two presses. Deleting takes the review history with it, and
                    a mis-click on a hover-revealed icon is easy. */}
                <Action
                  label={
                    confirming === card.id ? "Press again to delete" : "Delete"
                  }
                  danger={confirming === card.id}
                  onClick={async () => {
                    if (confirming !== card.id) {
                      setConfirming(card.id);
                      return;
                    }
                    setConfirming(null);
                    await api.deleteCard(card.id);
                    await load();
                    onChanged();
                  }}
                  onBlur={() => setConfirming(null)}
                >
                  <Trash2 size={13} />
                </Action>
              </div>
            </li>
          ),
        )}
      </ul>
    </div>
  );
}

function Action({
  label,
  danger,
  onClick,
  onBlur,
  children,
}: {
  label: string;
  danger?: boolean;
  onClick: () => void;
  onBlur?: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      onBlur={onBlur}
      title={label}
      aria-label={label}
      className={cx(
        "pressable rounded-[var(--r-sm)] p-1.5",
        danger
          ? "bg-[var(--danger)]/15 text-[var(--danger)]"
          : "text-[var(--ink-faint)] hover:bg-[var(--surface-hi)] hover:text-[var(--ink)]",
      )}
    >
      {children}
    </button>
  );
}

function CardEditor({
  card,
  onCancel,
  onSaved,
}: {
  card: CardRow;
  onCancel: () => void;
  onSaved: () => Promise<void>;
}) {
  const [front, setFront] = useState(card.front);
  const [back, setBack] = useState(card.back);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    try {
      await api.editCard(card.id, front, back);
      await onSaved();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <li className="rounded-[var(--r-md)] border border-[var(--accent)]/35 bg-[var(--surface-hi)] p-3">
      <textarea
        autoFocus
        value={front}
        onChange={(e) => setFront(e.target.value)}
        rows={2}
        className="w-full resize-none rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface)] p-2.5 text-[13.5px] text-[var(--ink)] outline-none focus:border-[var(--accent)]"
      />
      <textarea
        value={back}
        onChange={(e) => setBack(e.target.value)}
        rows={2}
        className="mt-2 w-full resize-none rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface)] p-2.5 text-[13.5px] text-[var(--ink-dim)] outline-none focus:border-[var(--accent)]"
      />

      {error && (
        <p className="mt-2 text-[12px] text-[var(--danger)]">{error}</p>
      )}

      <div className="mt-2.5 flex items-center gap-2">
        <Button size="sm" variant="primary" onClick={() => void save()}>
          Save
        </Button>
        <Button size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <span className="ml-auto text-[11.5px] text-[var(--ink-faint)]">
          The schedule is kept — rewording a card doesn't undo what you know.
        </span>
      </div>
    </li>
  );
}
