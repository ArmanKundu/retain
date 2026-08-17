import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

import App from "./App";
import { Capture } from "./screens/Capture";
import { Sticky } from "./screens/Sticky";
import "./index.css";

/**
 * Two windows share this bundle. The capture bar is a separate, pre-warmed
 * window that must not mount the whole app — booting the sidebar, store and
 * routing behind a hotkey would cost exactly the latency capture can't afford.
 */
const label = getCurrentWindow().label;
const isCapture = label === "capture";

// Sticky windows are labelled `sticky-<note id>`, so the window itself carries
// which note it is showing — nothing has to be passed in or looked up.
const stickyId = label.startsWith("sticky-")
  ? Number(label.slice("sticky-".length))
  : null;

// The capture window is created `transparent: true`, but `html` and `body` both
// paint `--canvas` — so the "transparent" window rendered as an opaque black
// rectangle with the pill floating inside it. Marking the root here lets the
// stylesheet drop those two backgrounds for this window only; the main window
// still needs them.
//
// Set before render so the first paint is already correct — doing it in an
// effect shows one frame of the black box.
if (isCapture) document.documentElement.dataset.window = "capture";
// Stickies are transparent windows too, so the same background rule applies.
if (stickyId !== null) document.documentElement.dataset.window = "capture";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isCapture ? (
      <Capture />
    ) : stickyId !== null ? (
      <Sticky noteId={stickyId} />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
