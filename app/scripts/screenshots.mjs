// Screenshots of the interface for the README and the website (docs/screenshots) and the social
// preview (docs/social-preview.png), from the preview build (vite.preview.config.ts, preview/fake.ts).
// Needs Chrome or Chromium: set CHROME to its path if it is not found.
//   npm run screenshots
import { execFile, spawnSync } from "node:child_process";
import { promisify } from "node:util";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { build, preview } from "vite";


const app = resolve(import.meta.dirname, "..");
const docs = resolve(app, "../docs");
const config = resolve(app, "vite.preview.config.ts");

// [file, page as the app's address says it, width, height]
const shots = [
  ["screenshots/keyboard-lighting.png", "index.html#keyboard:lighting", 1160, 780],
  ["screenshots/keyboard-keys.png", "index.html#keyboard:keys", 1160, 780],
  ["screenshots/mouse-dpi.png", "index.html#mouse:performance", 1160, 780],
  ["screenshots/mouse-buttons.png", "index.html#mouse:buttons", 1160, 780],
  ["screenshots/mouse-lighting.png", "index.html#mouse:lighting", 1160, 780],
  ["social-preview.png", "social.html", 1280, 640],
];

function chrome() {
  const names = process.env.CHROME ? [process.env.CHROME] : ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser"];
  for (const name of names) {
    if (spawnSync(name, ["--version"], { stdio: "ignore" }).status === 0) return name;
  }
  throw new Error("Chrome or Chromium not found: set CHROME");
}

const browser = chrome();
await build({ configFile: config, root: app, logLevel: "warn" });
const server = await preview({ configFile: config, root: app, logLevel: "warn" });
const base = server.resolvedUrls.local[0] + "preview/";
// A profile of its own, so a Chrome the user has open is left alone.
const profile = mkdtempSync(join(tmpdir(), "perihub-shots-"));
try {
  for (const [file, page, width, height] of shots) {
    const out = join(docs, file);
    // Asynchronous: the preview server runs in this process and must keep answering.
    await promisify(execFile)(
      browser,
      [
        "--headless=new",
        "--disable-gpu",
        "--hide-scrollbars",
        "--no-first-run",
        "--force-device-scale-factor=1",
        `--user-data-dir=${profile}`,
        `--window-size=${width},${height}`,
        "--virtual-time-budget=5000",
        `--screenshot=${out}`,
        base + page,
      ],
      { timeout: 60_000 },
    );
    console.log(file);
  }
} finally {
  rmSync(profile, { recursive: true, force: true });
  await new Promise((done) => server.httpServer.close(done));
}
