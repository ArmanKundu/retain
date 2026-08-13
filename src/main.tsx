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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isCapture ? <Capture /> : <App />}</React.StrictMode>,
);
