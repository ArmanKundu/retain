// A line of a note, with its inline formatting drawn.
//
// This is the read half of live preview: a blurred block renders through here,
// and swaps back to its textarea the moment you click into it. You see the
// markers on the line you're editing and the formatting everywhere else.
//
// No `dangerouslySetInnerHTML` anywhere — the spans come from the parser as
// data and become React elements, so a note holding `<img onerror=...>` is just
// text with angle brackets in it.

import { openUrl } from "@tauri-apps/plugin-opener";

import { parseInline, type Span } from "../lib/inlineMarkdown";

export function InlineText({ source }: { source: string }) {
  return <>{parseInline(source).map((span, i) => render(span, i))}</>;
}

function render(span: Span, key: number) {
  switch (span.kind) {
    case "bold":
      return (
        <strong key={key} className="font-semibold text-[var(--ink)]">
          {span.text}
        </strong>
      );
    case "italic":
      return (
        <em key={key} className="italic">
          {span.text}
        </em>
      );
    case "boldItalic":
      return (
        <strong key={key} className="font-semibold italic text-[var(--ink)]">
          {span.text}
        </strong>
      );
    case "code":
      return (
        <code
          key={key}
          className="rounded-[var(--r-xs)] bg-[var(--surface-hi)] px-1 py-0.5 font-mono text-[0.88em] text-[var(--ink-dim)]"
        >
          {span.text}
        </code>
      );
    case "strike":
      return (
        <s key={key} className="text-[var(--ink-faint)]">
          {span.text}
        </s>
      );
    case "highlight":
      return (
        // A wash rather than a solid fill: highlighter over text you can still
        // read, not a block of colour with text hiding in it.
        <mark
          key={key}
          className="rounded-[var(--r-xs)] bg-[var(--warn)]/22 px-0.5 text-[var(--ink)]"
        >
          {span.text}
        </mark>
      );
    case "link":
      return (
        <a
          key={key}
          href={span.href}
          onClick={(e) => {
            // The webview would navigate the whole app away from itself.
            // Links open in the real browser instead.
            e.preventDefault();
            void openUrl(span.href).catch(() => {});
          }}
          className="text-[var(--accent)] underline decoration-[var(--accent)]/40 underline-offset-2 hover:decoration-[var(--accent)]"
        >
          {span.text}
        </a>
      );
    default:
      return <span key={key}>{span.text}</span>;
  }
}
