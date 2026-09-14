import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const root = dirname(fileURLToPath(import.meta.url));

function entries(mode: string): Record<string, string> {
  const input: Record<string, string> = { main: resolve(root, "index.html") };
  if (mode !== "production") input.gallery = resolve(root, "gallery.html");
  return input;
}

// Two entries: the pet window the Tauri shell loads, and a browser-only
// gallery of all ten states (§9.3 — "do these states read at 80px"). The
// gallery is a visual check; the pipeline is proved by `cargo test`, not here.
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    rollupOptions: {
      // The gallery is a debug surface, so it is absent from a release build
      // rather than merely unlinked (§8.3). It is reachable on the dev server.
      input: entries(mode),
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
}));
