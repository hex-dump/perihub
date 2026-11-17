import { describe, expect, it } from "vitest";
import styles from "../styles.css?raw";
import { ACCENTS } from "./theme";

// Contrast of the colors in styles.css, which is read as it is, so a change there is checked here.
// Text: 4.5:1 (WCAG AA). The edge of a control and the accent drawn as a shape: 3:1. The dark theme is the look
// PeriHub always had, so its borders, which are quiet on purpose, are not held to 3:1.

const source = styles.replace(/\/\*[\s\S]*?\*\//g, "");

function block(selector: string): Record<string, string> {
  const found = new RegExp("(?:^|\\})\\s*" + selector.replace(/[.*+?^${}()|[\]\\"=:]/g, "\\$&") + "\\s*\\{([^}]*)\\}", "m").exec(source);
  if (!found) throw new Error("no block for " + selector);
  return Object.fromEntries([...found[1].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

type Rgb = [number, number, number];
const hex = (v: string): Rgb => {
  const m = /^#([0-9a-f]{6})$/i.exec(v);
  if (!m) throw new Error("not a color: " + v);
  return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)) as Rgb;
};
const triplet = (v: string): Rgb => v.split(" ").map(Number) as Rgb;

function luminance([r, g, b]: Rgb): number {
  const [lr, lg, lb] = [r, g, b].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
}
function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}
/** `color` at `alpha` over `under`. */
const over = (color: Rgb, alpha: number, under: Rgb): Rgb => color.map((c, i) => c * alpha + under[i] * (1 - alpha)) as Rgb;

const dark = block(":root");
const light = { ...dark, ...block(':root[data-theme="light"]') };

/** The accents as they are on each theme: [name, theme values]. Orange is the theme's own. */
function accentsOf(theme: "dark" | "light") {
  const base = theme === "dark" ? dark : light;
  return ACCENTS.map(({ id }) => {
    const own =
      id === "orange"
        ? {}
        : { ...block(`:root[data-accent="${id}"]`), ...(theme === "light" ? block(`:root[data-theme="light"][data-accent="${id}"]`) : {}) };
    const v = { ...base, ...own };
    return { id, accent: triplet(v["--accent-rgb"]), accent2: triplet(v["--accent-2-rgb"]), onAccent: hex(v["--on-accent"]) };
  });
}

const themes = { dark, light } as const;

/** The status colors (see styles.css), which never follow the accent. */
const STATUS = ["ok", "warn", "bad", "readonly", "experimental"] as const;

/** Distance in OKLab: how different two colors look (0.02 is barely visible, 0.1 is clearly another color). */
function oklab([r, g, b]: Rgb): [number, number, number] {
  const [lr, lg, lb] = [r, g, b].map((c) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}
const distance = (a: Rgb, b: Rgb) => Math.hypot(...oklab(a).map((x, i) => x - oklab(b)[i]));

describe.each(Object.entries(themes))("%s palette", (name, v) => {
  const surfaces = ["--bg", "--panel", "--panel-2", "--panel-3", "--field"] as const;
  const at = (k: string) => hex(v[k]);

  it.each(surfaces)("text and muted text read on %s", (s) => {
    expect(contrast(at("--text"), at(s))).toBeGreaterThanOrEqual(7);
    expect(contrast(at("--muted"), at(s))).toBeGreaterThanOrEqual(name === "dark" ? 6.5 : 4.5);
  });

  it.each(["--bg", "--panel", "--panel-2", "--field"])("faint text reads on %s", (s) => {
    expect(contrast(at("--faint"), at(s))).toBeGreaterThanOrEqual(4.5);
  });

  it("the status colors read as text and on their own soft background", () => {
    for (const [key, alpha] of [["ok", 0.12], ["warn", 0.1], ["bad", 0.1]] as const) {
      const color = triplet(v[`--${key}-rgb`]);
      for (const s of ["--bg", "--panel", "--panel-2"]) {
        expect(contrast(color, at(s)), `${key} on ${s}`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(color, over(color, alpha, at(s))), `${key} on its soft on ${s}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("every status icon reads on every surface, the selected sidebar item included", () => {
    // Status.svelte: the icon in the status color, the word in --muted, on a card or in the sidebar (--panel-3
    // when the item is selected). 4.5:1 although an icon needs 3:1, so the color can also be used as text.
    for (const key of STATUS) {
      for (const s of ["--bg", "--panel", "--panel-2", "--panel-3"]) {
        expect(contrast(triplet(v[`--${key}-rgb`]), at(s)), `${key} on ${s}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("the alpha of the soft backgrounds is the one tested here", () => {
    expect(dark["--ok-soft"]).toBe("rgb(var(--ok-rgb) / 0.12)");
    expect(dark["--warn-soft"]).toBe("rgb(var(--warn-rgb) / 0.1)");
    expect(dark["--bad-soft"]).toBe("rgb(var(--bad-rgb) / 0.1)");
    expect(dark["--accent-soft"]).toBe("rgb(var(--accent-rgb) / 0.12)");
  });

  it("every accent reads as text, as a button with its text, and as a link on hover", () => {
    for (const a of accentsOf(name as "dark" | "light")) {
      for (const s of ["--bg", "--panel", "--panel-2", "--panel-3"]) {
        expect(contrast(a.accent, at(s)), `${a.id} on ${s}`).toBeGreaterThanOrEqual(4.5);
      }
      for (const s of ["--bg", "--panel", "--panel-2"]) {
        // chip.accent: the accent on its own soft background
        expect(contrast(a.accent, over(a.accent, 0.12, at(s))), `${a.id} soft on ${s}`).toBeGreaterThanOrEqual(4.5);
      }
      for (const s of ["--bg", "--panel", "--panel-2"]) {
        expect(contrast(a.accent2, at(s)), `${a.id} hover on ${s}`).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrast(a.onAccent, a.accent), `${a.id} button`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(a.onAccent, a.accent2), `${a.id} button hover`).toBeGreaterThanOrEqual(4.5);
    }
  });
});

describe("light theme", () => {
  it("the borders of controls and the idle dot are 3:1 on the surfaces they sit on", () => {
    for (const s of ["--bg", "--panel", "--panel-2", "--field"]) {
      expect(contrast(hex(light["--line-strong"]), hex(light[s])), s).toBeGreaterThanOrEqual(3);
    }
  });

  it("a text field is told apart from its card", () => {
    expect(light["--field"]).toBe(light["--panel"]);
  });
});

describe("accents", () => {
  it("the swatches are the dark theme's accents", () => {
    const rgb = ([r, g, b]: Rgb) => "#" + [r, g, b].map((c) => c.toString(16).padStart(2, "0")).join("");
    for (const a of accentsOf("dark")) {
      expect(rgb(a.accent), a.id).toBe(ACCENTS.find((x) => x.id === a.id)!.swatch);
    }
  });

  it("orange on the dark theme is the color PeriHub always had", () => {
    expect(dark["--accent-rgb"]).toBe("255 106 43");
    expect(dark["--accent-2-rgb"]).toBe("255 138 82");
    expect(dark["--on-accent"]).toBe("#1a0d05");
  });
});

describe("status colors and the accent", () => {
  const ownLight = block(':root[data-theme="light"]');

  it("each theme sets its status colors itself, as plain colors", () => {
    for (const [name, v] of [["dark", dark], ["light", ownLight]] as const) {
      for (const key of STATUS) expect(v[`--${key}-rgb`], `${name} ${key}`).toMatch(/^\d+ \d+ \d+$/);
    }
  });

  it("the accent blocks set the accent and nothing else", () => {
    const blocks = [...source.matchAll(/:root((?:\[data-[a-z]+="[a-z]+"\])*)\s*\{([^}]*)\}/g)].filter(([, sel]) =>
      sel.includes("data-accent"),
    );
    expect(blocks.length).toBe((ACCENTS.length - 1) * 2);
    for (const [, sel, body] of blocks) {
      const keys = [...body.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]).sort();
      expect(keys, sel).toEqual(["--accent-2-rgb", "--accent-rgb", "--on-accent"]);
    }
  });

  it("no status color is written with the accent", () => {
    expect(source).not.toMatch(/--(ok|warn|bad|readonly|experimental)[\w-]*\s*:[^;]*accent/);
  });

  it.each(Object.entries(themes))("the status colors are told apart from each other on the %s theme", (name, v) => {
    const color = (key: string) => triplet(v[`--${key}-rgb`]);
    for (const [a, b] of [["ok", "warn"], ["ok", "bad"], ["warn", "bad"], ["readonly", "ok"], ["readonly", "warn"], ["readonly", "bad"]]) {
      expect(distance(color(a), color(b)), `${name}: ${a} and ${b}`).toBeGreaterThanOrEqual(0.12);
    }
    // Experimental is a caution: the warning's color, with an icon of its own (a flask).
    expect(v["--experimental-rgb"]).toBe(v["--warn-rgb"]);
  });

  it("the accents close to a status color are these, and the state is told by its icon and word there", () => {
    // Six accents around the color wheel cannot all stay away from green, amber and red. Where one is close,
    // the state is still told apart: Status.svelte always draws an icon and a word (Status.test.ts, App.test.ts),
    // and the accent is drawn as a fill or an edge. A new accent or a changed color shows up here first.
    const close: string[] = [];
    for (const [name, v] of Object.entries(themes)) {
      for (const a of accentsOf(name as "dark" | "light")) {
        for (const key of STATUS) {
          if (key !== "readonly" && distance(a.accent, triplet(v[`--${key}-rgb`])) < 0.1) close.push(`${name} ${a.id} ~ ${key}`);
        }
      }
    }
    expect(close).toEqual([
      "dark orange ~ bad",
      "dark green ~ ok",
      "dark pink ~ bad",
      "dark teal ~ ok",
      "light orange ~ bad",
      "light green ~ ok",
      "light pink ~ bad",
      "light teal ~ ok",
    ]);
  });
});

describe("components", () => {
  const sources = import.meta.glob("../**/*.svelte", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

  it("show states with Status, not with the accent or a color alone", () => {
    for (const [path, text] of Object.entries(sources)) {
      // The accent on a chip is for a call to action (Update), never for a support level or a state.
      expect(text, path).not.toMatch(/class:accent=/);
      expect(text, path).not.toMatch(/class="state[ "]/);
      expect(text, path).not.toMatch(/class="chip (ok|test|running)"/);
    }
  });
});
