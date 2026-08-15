import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

import App from "./App";
import { Capture } from "./screens/Capture";
import "./index.css";

/**
 * Two windows share this bundle. The capture bar is a separate, pre-warmed
 * window that must not mount the whole app — booting the sidebar, store and
 * routing behind a hotkey would cost exactly the latency capture can't afford.
 */
const isCapture = getCurrentWindow().label === "capture";

// The capture window is created `transparent: true`, but `html` and `body` both
// paint `--canvas` — so the "transparent" window rendered as an opaque black
// rectangle with the pill floating inside it. Marking the root here lets the
// stylesheet drop those two backgrounds for this window only; the main window
// still needs them.
//
// Set before render so the first paint is already correct — doing it in an
// effect shows one frame of the black box.
if (isCapture) document.documentElement.dataset.window = "capture";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isCapture ? <Capture /> : <App />}</React.StrictMode>,
);
