import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { releaseNotes } from "./vite.changelog.ts";

// Fixed port: Tauri loads the interface from here during development.
export default defineConfig({
  plugins: [svelte(), releaseNotes()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist" },
});
