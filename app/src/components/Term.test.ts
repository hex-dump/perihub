import { cleanup, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import { afterEach, describe, expect, it } from "vitest";
import Term from "./Term.svelte";
import { GLOSSARY } from "../lib/glossary";

afterEach(cleanup);

describe("Term", () => {
  it("explains the word on hover", () => {
    const children = createRawSnippet(() => ({ render: () => "<span>DPI</span>" }));
    const { container } = render(Term, { term: "dpi", children });
    const abbr = container.querySelector("abbr.term")!;
    expect(abbr.textContent).toBe("DPI");
    expect(abbr.getAttribute("title")).toBe(GLOSSARY.dpi);
  });

  it("keeps every explanation short enough for a tooltip", () => {
    for (const [term, text] of Object.entries(GLOSSARY)) expect(text.length, term).toBeLessThanOrEqual(140);
  });
});
