// What actually goes on the page.
//
// The previous approach marked a div in the app `print-target` and hid the rest
// with `visibility: hidden`. Two things wrong with it, and both produce the
// same symptom — a print that is mostly blank paper:
//
//   * **`visibility: hidden` still occupies space.** Every hidden element keeps
//     its full height, so a four-screen-tall app printed four pages of nothing
//     before reaching anything.
//   * **Ancestors still clip.** The target sat inside a flex column with
//     `overflow-y-auto`. Overriding `overflow` on the target itself does
//     nothing about the scroll container two levels above it, so only the
//     visible screenful ever reached the printer.
//
// So the printable version is built separately and rendered into its own node
// on `<body>`, and print hides every other child of body with `display: none`.
// Nothing is inside a scroll container, nothing reserves space, and what you
// see on paper is written for paper rather than being a screenshot of the app
// with its furniture hidden.

import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export function PrintPortal({ children }: { children: ReactNode }) {
  const [host] = useState(() => {
    const el = document.createElement("div");
    el.className = "print-only";
    return el;
  });

  useEffect(() => {
    document.body.appendChild(host);
    return () => {
      host.remove();
    };
  }, [host]);

  return createPortal(children, host);
}

/**
 * The masthead on a printed page.
 *
 * A stack of untitled printouts is why people stop printing things.
 */
export function PrintHeader({
  title,
  meta,
}: {
  title: string;
  meta?: (string | null)[];
}) {
  return (
    <header className="print-header">
      <div className="print-title">{title}</div>
      <div className="print-meta">
        {(meta ?? []).filter(Boolean).join(" · ")}
      </div>
    </header>
  );
}
