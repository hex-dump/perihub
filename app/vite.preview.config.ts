import { fileURLToPath } from "node:url";
import { defineConfig, mergeConfig } from "vite";
import base from "./vite.config.ts";

// The interface with the Tauri API replaced by preview/fake.ts, for the screenshots
// (npm run screenshots). Not part of the app.
const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));

export default mergeConfig(
  base,
  defineConfig({
    resolve: {
      alias: [
        { find: /^@tauri-apps\/api\/core$/, replacement: here("./preview/tauri/core.ts") },
        { find: /^@tauri-apps\/api\/event$/, replacement: here("./preview/tauri/event.ts") },
        { find: /^@tauri-apps\/api\/app$/, replacement: here("./preview/tauri/app.ts") },
        { find: /^@tauri-apps\/api\/window$/, replacement: here("./preview/tauri/window.ts") },
      ],
    },
    server: { port: 1430, strictPort: true },
    preview: { port: 1430, strictPort: true },
    build: {
      outDir: "preview-dist",
      emptyOutDir: true,
      rollupOptions: { input: { index: here("./preview/index.html"), social: here("./preview/social.html") } },
    },
  }),
);
