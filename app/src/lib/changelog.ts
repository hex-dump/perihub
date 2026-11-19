// The release notes of one version, read from CHANGELOG.md at build time (see vite.changelog.ts). The
// rules are those of tools/release.py: a `## X.Y.Z — date` heading, a summary paragraph, then `### `
// sections of `- ` items, an item going on in the indented lines under it. Only the bold lead of each
// item is kept: the long details stay in the full release notes.

export interface NotesSection {
  /** The section as CHANGELOG.md names it: "Added", "Fixed"... */
  name: string;
  /** The bold lead of each item, as plain text. */
  titles: string[];
}

export interface ReleaseNotes {
  version: string;
  date: string;
  /** The paragraph before the first section; empty when there is none. */
  summary: string;
  sections: NotesSection[];
}

const HEADING = /^## (\d+\.\d+\.\d+) — (\d{4}-\d{2}-\d{2})$/;

/** Markdown out of a title: links keep their text, code and emphasis lose their marks. */
function plain(text: string): string {
  return text
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** The bold lead of an item (`**Title**: details`); an item without one gives its first words. */
export function itemTitle(item: string): string {
  const bold = /^\*\*(.+?)\*\*/.exec(item);
  if (bold) return plain(bold[1]);
  const text = plain(item);
  return text.length > 90 ? text.slice(0, 89).trimEnd() + "…" : text;
}

/** The notes of `version` in `changelog`, or null when it has no section. */
export function parseRelease(changelog: string, version: string): ReleaseNotes | null {
  let notes: ReleaseNotes | null = null;
  let inside = false;
  const summary: string[] = [];
  const items: string[][] = [];
  const names: string[] = [];
  for (const raw of changelog.split(/\r?\n/)) {
    const line = raw.trimEnd();
    if (line.startsWith("## ")) {
      if (inside) break;
      const m = HEADING.exec(line);
      if (m && m[1] === version) {
        inside = true;
        notes = { version, date: m[2], summary: "", sections: [] };
      }
    } else if (!inside) {
      continue;
    } else if (line.startsWith("### ")) {
      names.push(line.slice(4).trim());
      items.push([]);
    } else if (names.length === 0) {
      if (line.trim()) summary.push(line.trim());
    } else {
      const list = items[items.length - 1];
      if (line.startsWith("- ")) list.push(line.slice(2).trim());
      else if (line.trim() && list.length) list[list.length - 1] += " " + line.trim();
    }
  }
  if (!notes) return null;
  notes.summary = plain(summary.join(" "));
  notes.sections = names.map((name, i) => ({ name, titles: items[i].map(itemTitle).filter(Boolean) }));
  return notes;
}
