import { readFileSync } from "node:fs";
import type { Plugin } from "vite";
import { parseRelease } from "./src/lib/changelog.ts";

const ID = "virtual:perihub-release-notes";
const RESOLVED = "\0" + ID;

/** A virtual module with the release notes of the version in package.json, read from ../CHANGELOG.md at
 * build time: nothing is fetched when the app runs. It exports null when the version has no section. */
export function releaseNotes(): Plugin {
  const changelog = new URL("../CHANGELOG.md", import.meta.url);
  const pkg = new URL("./package.json", import.meta.url);
  return {
    name: "perihub-release-notes",
    resolveId: (id) => (id === ID ? RESOLVED : undefined),
    load(id) {
      if (id !== RESOLVED) return;
      this.addWatchFile(changelog.pathname);
      const version = (JSON.parse(readFileSync(pkg, "utf8")) as { version: string }).version;
      const notes = parseRelease(readFileSync(changelog, "utf8"), version);
      return `export default ${JSON.stringify(notes)};`;
    },
  };
}
