// The study assistant.
//
// The thing that makes this different from a chat box: **strict grounding is
// the default**. It answers from material you've uploaded, and when your
// material doesn't cover something it says so rather than filling the gap. The
// toggle turns that off, and when it's off anything outside your notes is
// labelled.
//
// Every answer shows which passages of your own material it used, so a bad
// retrieval is visible rather than silently shaping what you revise from.

import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  BookLock,
  Check,
  Download,
  ExternalLink,
  Globe,
  Monitor,
  Paperclip,
  Plus,
  Printer,
  Send,
  Trash2,
  X,
} from "lucide-react";

import { AiGate, useAi } from "../components/Ai";
import { Chip } from "../components/primitives";
import { Button, Card, cx } from "../components/ui";
import { api } from "../lib/api";
import type {
  ChatMessage,
  Conversation,
  Grounding,
  NewAttachment,
  Proposal,
} from "../lib/types";
import { useApp } from "../store";

export function Assistant() {
  const setRoute = useApp((s) => s.setRoute);
  const { status } = useAi();

  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [question, setQuestion] = useState("");
  const [attachments, setAttachments] = useState<NewAttachment[]>([]);
  // What the assistant last offered to do. Deliberately not stored with the
  // conversation: a button offering to plan Thursday is meaningless in
  // November, and one still sitting there is a trap rather than a feature.
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [capturing, setCapturing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const endRef = useRef<HTMLDivElement>(null);

  const active = conversations.find((c) => c.id === activeId) ?? null;

  const loadConversations = useCallback(async () => {
    try {
      const list = await api.listConversations();
      setConversations(list);
      return list;
    } catch {
      setConversations([]);
      return [];
    }
  }, []);

  const loadMessages = useCallback(async (id: number) => {
    try {
      setMessages(await api.conversationMessages(id));
    } catch {
      setMessages([]);
    }
  }, []);

  useEffect(() => {
    void loadConversations();
  }, [loadConversations]);

  useEffect(() => {
    if (activeId != null) void loadMessages(activeId);
    else setMessages([]);
  }, [activeId, loadMessages]);

  // Follow the conversation as it grows, but only when something was added —
  // not on every render, which would fight you scrolling back.
  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages.length, busy]);

  const startConversation = async (grounding: Grounding = "strict") => {
    const id = await api.createConversation(null, grounding);
    await loadConversations();
    setActiveId(id);
    setMessages([]);
  };

  const send = async () => {
    const text = question.trim();
    if (!text || busy) return;

    let id = activeId;
    if (id == null) {
      id = await api.createConversation(null, "strict");
      setActiveId(id);
      await loadConversations();
    }

    setBusy(true);
    setError(null);
    const sent = attachments;
    setQuestion("");
    setAttachments([]);
    // Last turn's offers expire the moment you ask something else.
    setProposals([]);

    try {
      const turn = await api.askAssistant(id, text, sent);
      setProposals(turn.proposals);
    } catch (e) {
      setError(String(e));
      // Put the question back so a failed request doesn't lose what you typed.
      setQuestion(text);
      setAttachments(sent);
    } finally {
      setBusy(false);
      await loadMessages(id);
      await loadConversations();
    }
  };

  /**
   * Show the assistant what's on screen.
   *
   * One press, one image, and it goes nowhere until you send the message — it
   * appears as an attachment you can look at and remove first. There is no
   * watching mode; see `screen.rs` for why.
   */
  const showScreen = async () => {
    setCapturing(true);
    setError(null);
    try {
      const dataUrl = await api.captureScreen();
      setAttachments((a) => [
        ...a,
        { name: "Screenshot", content: "", imageDataUrl: dataUrl },
      ]);
    } catch (e) {
      setError(String(e));
    } finally {
      setCapturing(false);
    }
  };

  const attach = async () => {
    const picked = await open({
      multiple: true,
      title: "Attach to this question",
      filters: [
        {
          name: "Documents",
          extensions: [
            "pdf",
            "txt",
            "md",
            "markdown",
            "csv",
            "html",
            "rtf",
            "json",
          ],
        },
      ],
    });
    if (!picked) return;

    const paths = Array.isArray(picked) ? picked : [picked];
    for (const path of paths) {
      const outcome = await api.readFileText(path);
      if (outcome.status === "extracted") {
        setAttachments((a) => [
          ...a,
          { name: outcome.name, content: outcome.text, imageDataUrl: null },
        ]);
      } else {
        setError(
          outcome.status === "scanned"
            ? `${outcome.name} is a scanned PDF — its pages are images, so there's no text to read.`
            : `${outcome.name}: ${outcome.reason}`,
        );
      }
    }
  };

  return (
    <div className="flex h-full min-h-0">
      {/* Conversations */}
      <aside className="flex w-[236px] shrink-0 flex-col border-r border-[var(--line-soft)]">
        {/* Content scrolls under the title bar. macOS separates the two with a
          hard edge rather than letting text vanish mid-letter. */}
        <div className="titlebar-drag scroll-edge h-11" />

        <div className="px-3 pb-2">
          <Button
            size="sm"
            className="w-full"
            onClick={() => void startConversation()}
          >
            <Plus size={13} />
            New conversation
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
          {conversations.length === 0 ? (
            <p className="px-2 py-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
              Ask something below and it'll be kept here.
            </p>
          ) : (
            conversations.map((c) => (
              <button
                key={c.id}
                onClick={() => setActiveId(c.id)}
                className={cx(
                  "group mb-0.5 flex w-full items-start gap-2 rounded-[var(--r-md)] px-2.5 py-2 text-left",
                  "transition-colors duration-[var(--t-fast)]",
                  c.id === activeId
                    ? "bg-[var(--surface-hi)] text-[var(--ink)]"
                    : "text-[var(--ink-dim)] hover:bg-[var(--surface-hi)]/60",
                )}
              >
                {c.grounding === "strict" ? (
                  <BookLock
                    size={12}
                    className="mt-[3px] shrink-0 text-[var(--ink-faint)]"
                  />
                ) : (
                  <Globe
                    size={12}
                    className="mt-[3px] shrink-0 text-[var(--ink-faint)]"
                  />
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[12.5px]">
                    {c.title}
                  </span>
                  <span className="block text-[11px] text-[var(--ink-faint)]">
                    {c.messageCount}{" "}
                    {c.messageCount === 1 ? "message" : "messages"}
                  </span>
                </span>
              </button>
            ))
          )}
        </div>
      </aside>

      {/* Conversation */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="titlebar-drag h-11 shrink-0" />

        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[min(760px,100%)] px-6 pb-6 sm:px-9">
            {messages.length === 0 && (
              <header className="animate-rise mb-6">
                <h1 className="text-[28px] font-semibold tracking-[var(--track-display)]">
                  Assistant
                </h1>
                <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
                  Ask about your subjects, your material, or what to work on
                  next.
                </p>
              </header>
            )}

            <AiGate
              status={status}
              what="answer questions from the material you've uploaded"
              onOpenSettings={() => setRoute("settings")}
            >
              {messages.length === 0 ? (
                <StarterHints
                  grounding={active?.grounding ?? "strict"}
                  onPick={(q) => setQuestion(q)}
                />
              ) : (
                <div className="space-y-6 pt-2">
                  {messages.map((m) => (
                    <MessageBubble key={m.id} message={m} />
                  ))}
                  {busy && (
                    <div className="flex items-center gap-2 text-[13px] text-[var(--ink-faint)]">
                      <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-[var(--accent)]" />
                      Reading your material…
                    </div>
                  )}
                </div>
              )}
              <div ref={endRef} />
            </AiGate>
          </div>
        </div>

        {/* Composer */}
        {status?.provider && (
          <div className="shrink-0 px-6 pb-5 sm:px-9">
            <div className="mx-auto w-full max-w-[min(760px,100%)]">
              {error && (
                <p className="mb-2 text-[12.5px] leading-relaxed text-[var(--danger)]">
                  {error}
                </p>
              )}

              {proposals.length > 0 && (
                <ProposalList
                  proposals={proposals}
                  onDone={(i) =>
                    setProposals((p) => p.filter((_, j) => j !== i))
                  }
                />
              )}

              {attachments.length > 0 && (
                <div className="mb-2 flex flex-wrap gap-1.5">
                  {attachments.map((a, i) => (
                    <span
                      key={`${a.name}-${i}`}
                      className="flex items-center gap-1.5 rounded-full border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 py-1 text-[11.5px] text-[var(--ink-dim)]"
                    >
                      {a.imageDataUrl ? (
                        <img
                          src={a.imageDataUrl}
                          alt=""
                          className="h-4 w-6 rounded-[3px] object-cover"
                        />
                      ) : (
                        <Paperclip size={11} />
                      )}
                      {a.name}
                      <button
                        onClick={() =>
                          setAttachments((list) =>
                            list.filter((_, j) => j !== i),
                          )
                        }
                        aria-label={`Remove ${a.name}`}
                        className="pressable text-[var(--ink-faint)] hover:text-[var(--ink)]"
                      >
                        <X size={11} />
                      </button>
                    </span>
                  ))}
                </div>
              )}

              <div className="glass rounded-[var(--r-xl)] p-2.5">
                <textarea
                  value={question}
                  onChange={(e) => setQuestion(e.target.value)}
                  onKeyDown={(e) => {
                    // Enter sends; Shift+Enter is a newline. A question is
                    // usually one line, and reaching for a button each time is
                    // what makes an assistant feel slow.
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      void send();
                    }
                  }}
                  rows={2}
                  placeholder={
                    active?.grounding === "open"
                      ? "Ask anything — your material first, then general knowledge."
                      : "Ask about your uploaded material…"
                  }
                  className="w-full resize-none bg-transparent px-2 pt-1 text-[14px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none"
                />

                <div className="mt-1 flex items-center gap-2 px-1">
                  <button
                    onClick={() => void attach()}
                    title="Attach a file to this question"
                    aria-label="Attach a file"
                    className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:bg-[var(--surface-hi)] hover:text-[var(--ink)]"
                  >
                    <Paperclip size={15} />
                  </button>

                  <button
                    onClick={() => void showScreen()}
                    disabled={capturing}
                    title="Show the assistant what's on your screen"
                    aria-label="Attach a screenshot of your screen"
                    className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:bg-[var(--surface-hi)] hover:text-[var(--ink)] disabled:opacity-50"
                  >
                    <Monitor size={15} />
                  </button>

                  <GroundingToggle
                    grounding={active?.grounding ?? "strict"}
                    onChange={async (g) => {
                      if (activeId == null) {
                        await startConversation(g);
                        return;
                      }
                      await api.setConversationGrounding(activeId, g);
                      await loadConversations();
                    }}
                  />

                  <div className="ml-auto flex items-center gap-1.5">
                    {activeId != null && messages.length > 0 && (
                      <ConversationActions
                        id={activeId}
                        onDeleted={async () => {
                          setActiveId(null);
                          await loadConversations();
                        }}
                      />
                    )}
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={busy || !question.trim()}
                      onClick={() => void send()}
                    >
                      <Send size={13} />
                      {busy ? "Asking…" : "Ask"}
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * The grounding switch.
 *
 * Worded as a claim about behaviour rather than a mode name, because "strict"
 * on its own tells you nothing about what the assistant will do when your notes
 * fall short.
 */
function GroundingToggle({
  grounding,
  onChange,
}: {
  grounding: Grounding;
  onChange: (g: Grounding) => Promise<void>;
}) {
  const strict = grounding === "strict";

  return (
    <button
      onClick={() => void onChange(strict ? "open" : "strict")}
      title={
        strict
          ? "Answers come only from material you've uploaded. Click to also allow general knowledge."
          : "General knowledge is allowed and labelled. Click to restrict to your material only."
      }
      className={cx(
        "pressable flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11.5px]",
        strict
          ? "border-[var(--accent)]/35 bg-[var(--accent)]/12 text-[var(--accent)]"
          : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)]",
      )}
    >
      {strict ? <BookLock size={11} /> : <Globe size={11} />}
      {strict ? "My material only" : "Material + general knowledge"}
    </button>
  );
}

function ConversationActions({
  id,
  onDeleted,
}: {
  id: number;
  onDeleted: () => Promise<void>;
}) {
  const [saved, setSaved] = useState(false);

  return (
    <>
      <button
        onClick={async () => {
          // Reuses the library exporter's destination so everything Retain
          // writes lands in the same place.
          await api.conversationMarkdown(id);
          setSaved(true);
          window.setTimeout(() => setSaved(false), 2000);
          window.print();
        }}
        title="Print this conversation"
        aria-label="Print this conversation"
        className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:bg-[var(--surface-hi)] hover:text-[var(--ink)]"
      >
        {saved ? <Download size={15} /> : <Printer size={15} />}
      </button>

      <button
        onClick={() => void api.deleteConversation(id).then(onDeleted)}
        title="Delete this conversation"
        aria-label="Delete this conversation"
        className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] hover:text-[var(--danger)]"
      >
        <Trash2 size={15} />
      </button>
    </>
  );
}

function StarterHints({
  grounding,
  onPick,
}: {
  grounding: Grounding;
  onPick: (q: string) => void;
}) {
  const hints = [
    "What should I work on tonight?",
    "What have I been avoiding this week?",
    "Explain the dot point I keep getting wrong",
    "Turn my notes on this topic into flashcards I can paste in",
  ];

  return (
    <div className="animate-rise">
      <div className="flex flex-wrap gap-1.5">
        {hints.map((h) => (
          <Chip key={h} onClick={() => onPick(h)}>
            {h}
          </Chip>
        ))}
      </div>

      <Card className="mt-5 p-4">
        <p className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
          {grounding === "strict" ? (
            <>
              Right now the assistant answers{" "}
              <strong>only from material you've uploaded</strong>. If your notes
              don't cover something it'll say so rather than filling the gap —
              which is the point: an answer you can trace back to your own study
              design is worth having, and a confident one you can't is worse
              than none.
            </>
          ) : (
            <>
              The assistant will use your material first and then its own
              knowledge, saying explicitly which is which.
            </>
          )}
          <br />
          <br />
          It can also see what's due, what's coming up and this week's hours —
          so questions about your actual schedule work. It won't change anything
          for you; it points you at the screen instead.
        </p>
      </Card>
    </div>
  );
}

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === "user";

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] rounded-[var(--r-lg)] rounded-br-[var(--r-sm)] bg-[var(--accent)] px-4 py-2.5 text-[13.5px] leading-relaxed text-white">
          <p className="selectable whitespace-pre-wrap">{message.body}</p>
          {message.attachments.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1.5 border-t border-white/20 pt-2">
              {message.attachments.map((a) => (
                <span
                  key={a.id}
                  className="flex items-center gap-1 text-[11px] text-white/80"
                >
                  <Paperclip size={10} />
                  {a.name}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div>
      <p className="selectable whitespace-pre-wrap text-[13.5px] leading-[1.7] text-[var(--ink)]">
        {message.body}
      </p>

      {/* Citations. Hovering one shows the passage, so a bad retrieval is
          checkable rather than something you have to take on trust. */}
      {message.sources.length > 0 && (
        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          <span className="text-[11px] text-[var(--ink-faint)]">
            From your material:
          </span>
          {Array.from(new Set(message.sources.map((s) => s.resourceTitle))).map(
            (title) => (
              <span
                key={title}
                title={
                  message.sources
                    .find((s) => s.resourceTitle === title)
                    ?.content.slice(0, 500) ?? ""
                }
                className="rounded-full border border-[var(--line)] px-2 py-0.5 text-[11px] text-[var(--ink-dim)]"
              >
                {title}
              </span>
            ),
          )}
        </div>
      )}

      {message.model && (
        <div className="mt-2 font-mono text-[10.5px] text-[var(--ink-faint)]">
          {message.model}
        </div>
      )}
    </div>
  );
}

/**
 * What the assistant offered to do.
 *
 * Every one of these is a button and nothing happens without a press. The label
 * comes from Rust, built from the parsed action rather than from anything the
 * model wrote — a proposal that could describe itself would make the whole
 * confirmation step decoration. See `src-tauri/src/tools.rs`.
 */
function ProposalList({
  proposals,
  onDone,
}: {
  proposals: Proposal[];
  onDone: (index: number) => void;
}) {
  const [running, setRunning] = useState<number | null>(null);
  const [failed, setFailed] = useState<Record<number, string>>({});

  const run = async (p: Proposal, i: number) => {
    setRunning(i);
    try {
      await api.applyAssistantAction(p.action);
      onDone(i);
    } catch (e) {
      setFailed((f) => ({ ...f, [i]: String(e) }));
    } finally {
      setRunning(null);
    }
  };

  return (
    <div className="animate-rise mb-2.5 space-y-1.5">
      {proposals.map((p, i) => (
        <div
          key={`${p.summary}-${i}`}
          className="flex items-center gap-3 rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] px-3.5 py-2.5"
        >
          {p.external ? (
            <ExternalLink size={14} className="shrink-0 text-[var(--warn)]" />
          ) : (
            <Check size={14} className="shrink-0 text-[var(--ink-faint)]" />
          )}

          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] text-[var(--ink)]">
              {p.summary}
            </div>
            {p.external && (
              <div className="text-[11.5px] text-[var(--warn)]">
                Leaves Retain
              </div>
            )}
            {failed[i] && (
              <div className="mt-0.5 text-[11.5px] text-[var(--danger)]">
                {failed[i]}
              </div>
            )}
          </div>

          <Button
            size="sm"
            disabled={running === i}
            onClick={() => void run(p, i)}
          >
            {running === i ? "…" : "Do it"}
          </Button>
          <button
            onClick={() => onDone(i)}
            aria-label="Dismiss this suggestion"
            className="pressable shrink-0 rounded-full p-1 text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
