// Hides what could point to one person or one device before a test report is shared: user names
// in paths, serial numbers, unit ids, hardware addresses and other unique ids. PeriHub's reports
// already leave most of these out (the core masks the unit id of a Logitech device, no report holds a
// serial number or a path); this is a second net for what a device or the system may put in a text
// by surprise, and the wizard tells the user what it hid.

/** What was hidden: a short description and how many times. */
export interface Hidden {
  what: string;
  count: number;
}

export interface Redacted {
  text: string;
  hidden: Hidden[];
}

const USER = "[user]";
const ID = "[id hidden]";
/** Shorter names would hide too much: "pro" or "g5" in a model name. */
const MIN_NAME = 3;

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** The user names in home folders, such as "/home/ann/.config/perihub" or "C:\\Users\\Ann\\AppData". */
export function userNames(paths: string[]): string[] {
  const names = new Set<string>();
  for (const path of paths) {
    for (const m of path.matchAll(/(?:\/home\/|\/Users\/)([^\\/\s]+)|[A-Za-z]:[\\/]Users[\\/]([^\\/\r\n]+)/g)) {
      const name = m[1] ?? m[2];
      if (name.length >= MIN_NAME) names.add(name);
    }
  }
  return [...names];
}

type Rule = { what: string; pattern: RegExp; replace: string | ((match: string, ...groups: string[]) => string) };

/** Values after a label that say nothing: they stay. */
const EMPTY_VALUE = /^(?:x+|not|none|n\/a|unknown|-+|\[.*)$/i;

const RULES: Rule[] = [
  {
    what: "user name in a path",
    pattern: /(\/home\/|\/Users\/|\/run\/media\/|\/media\/)(?!\[user\])([^\\/\s"'<>|]+)/g,
    replace: (_m, folder) => folder + USER,
  },
  {
    // A Windows name may hold a space ("Ann Smith"): up to the next separator.
    what: "user name in a path",
    pattern: /([A-Za-z]:[\\/]Users[\\/])(?!\[user\])([^\\/"'<>|\r\n]+)/g,
    replace: (_m, folder) => folder + USER,
  },
  { what: "email address", pattern: /[\w.+-]+@[\w-]+(?:\.[\w-]+)+/g, replace: "[email hidden]" },
  { what: "hardware address", pattern: /\b[0-9a-f]{2}([:-])[0-9a-f]{2}(?:\1[0-9a-f]{2}){4}\b/gi, replace: "[address hidden]" },
  {
    what: "unique id",
    pattern: /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi,
    replace: ID,
  },
  {
    // "Serial number: AB12", "unit id = 12 34 56 78". "xx" is the core's own mask: it stays.
    what: "serial number or unit id",
    pattern:
      /\b(serial(?: number)?|s\/n|unit[ _]?id|uid|unique id|device id|mac(?: address)?)(\s*[:=]\s*)([0-9A-Za-z_.-]+(?: [0-9a-f]{2}\b)*)/gi,
    replace: (m, label, sep, value) => (EMPTY_VALUE.test(value.split(" ")[0]) ? m : `${label}${sep}${ID}`),
  },
  {
    what: "serial number or unit id",
    pattern: /\b(serial(?: number)?|s\/n)( )(?=[0-9A-Za-z_.-]{6,})(?=[^\s]*\d)([0-9A-Za-z_.-]+)/gi,
    replace: (_m, label, sep) => `${label}${sep}${ID}`,
  },
  // A long run of hex digits is no raw byte dump (those have a space between bytes): an id.
  { what: "long id", pattern: /\b(?=[0-9a-f]*\d)[0-9a-f]{12,}\b/gi, replace: ID },
];

/** `text` without what could identify the user or the device. `names`: user names to hide wherever
 * they stand (see {@link userNames}). */
export function redact(text: string, names: string[] = []): Redacted {
  const counts = new Map<string, number>();
  const count = (what: string, n = 1) => counts.set(what, (counts.get(what) ?? 0) + n);
  let out = text;
  for (const { what, pattern, replace } of RULES) {
    out = out.replace(pattern, (...args) => {
      const match = args[0] as string;
      const result = typeof replace === "string" ? replace : replace(match, ...(args.slice(1) as string[]));
      if (result !== match) count(what);
      return result;
    });
  }
  for (const name of names.filter((n) => n.length >= MIN_NAME)) {
    out = out.replace(new RegExp(`(?<![\\w-])${escape(name)}(?![\\w-])`, "gi"), () => {
      count("user name");
      return USER;
    });
  }
  return { text: out, hidden: [...counts].map(([what, count]) => ({ what, count })) };
}

/** What several redactions hid, added up. */
export function mergeHidden(lists: Hidden[][]): Hidden[] {
  const total = new Map<string, number>();
  for (const { what, count } of lists.flat()) total.set(what, (total.get(what) ?? 0) + count);
  return [...total].map(([what, count]) => ({ what, count }));
}
