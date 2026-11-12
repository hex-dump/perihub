import { describe, expect, it, vi } from "vitest";

vi.mock("./api", async (original) => ({ ...(await original<typeof import("./api")>()), api: {} }));

import { installingText } from "./updates.svelte";

describe("Update button", () => {
  it("shows the download, then the install", () => {
    expect(installingText(null)).toBe("Starting…");
    expect(installingText({ received: 1_000_000, total: 4_000_000 })).toBe("Downloading 25%…");
    expect(installingText({ received: 4_000_000, total: 4_000_000 })).toBe("Installing…");
    expect(installingText({ received: 300_000, total: null })).toBe("Downloading…");
  });
});
