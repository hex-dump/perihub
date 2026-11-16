import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  // THIRD-PARTY-NOTICES.md, at the root of the repository, is read by a test.
  server: { fs: { allow: [".."] } },
  // styles.css is read as text by theme.test.ts, so vitest must not blank it as it does the CSS it does not need.
  test: { environment: "jsdom", include: ["src/**/*.test.ts"], css: { include: [/src\/styles\.css/] } },
});
