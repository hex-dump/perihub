import { describe, expect, it } from "vitest";
import { uniqueBy } from "./unique";

describe("uniqueBy", () => {
  it("keeps the first of each key, in order", () => {
    expect(uniqueBy([{ k: "a", n: 1 }, { k: "b", n: 2 }, { k: "a", n: 3 }], (x) => x.k)).toEqual([
      { k: "a", n: 1 },
      { k: "b", n: 2 },
    ]);
  });
});
