import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  // Vite options tailored for Tauri development.
  //
  // 1. Don't clear the screen, so Rust compile errors stay visible.
  clearScreen: false,
  server: {
    // 2. Tauri expects a fixed port and should fail loudly if it's taken.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // 3. src-tauri is watched by cargo, not Vite.
      ignored: ["**/src-tauri/**"],
    },
  },
}));
