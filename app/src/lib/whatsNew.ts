// "What's new": a dialog with the release notes of the version, once after each update. The version last
// seen is kept in the embedded browser's storage, as the other interface preferences are.
import notes from "virtual:perihub-release-notes";
import type { ReleaseNotes } from "./changelog";

export const LAST_SEEN_KEY = "perihub.lastSeenVersion";

/** The release notes of this build, or null when its version has no section in CHANGELOG.md. */
export const currentNotes: ReleaseNotes | null = notes;

/** The notes to show for `version` now, or null. Nothing is shown on a first start (the version is only
 * remembered), when this version was already seen, when it has no notes, or when storage is unavailable. */
export function notesToShow(version: string, available: ReleaseNotes | null = currentNotes): ReleaseNotes | null {
  let last: string | null;
  try {
    last = localStorage.getItem(LAST_SEEN_KEY);
    if (last === version) return null;
    localStorage.setItem(LAST_SEEN_KEY, version);
  } catch {
    return null;
  }
  if (last === null) return null;
  return available?.version === version ? available : null;
}
