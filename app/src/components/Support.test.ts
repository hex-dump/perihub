import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ openSupportLink: vi.fn() }));
vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));

import Support from "./Support.svelte";

beforeEach(() => {
  vi.clearAllMocks();
  api.openSupportLink.mockResolvedValue(undefined);
});
afterEach(cleanup);

describe("Support", () => {
  it("shows nothing but the entry until it is clicked", () => {
    render(Support);
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByRole("button", { name: "Support me" }).getAttribute("aria-expanded")).toBe("false");
  });

  it("offers both links and opens them through the app", async () => {
    render(Support);
    await fireEvent.click(screen.getByRole("button", { name: "Support me" }));
    const coffee = screen.getByRole("link", { name: /Buy me a coffee/ });
    const star = screen.getByRole("link", { name: /Star on GitHub/ });
    expect(coffee.getAttribute("href")).toBe("https://buymeacoffee.com/hexdump1");
    expect(star.getAttribute("href")).toBe("https://github.com/hex-dump/perihub");
    await fireEvent.click(coffee);
    expect(api.openSupportLink).toHaveBeenCalledWith("https://buymeacoffee.com/hexdump1");
    await fireEvent.click(star);
    expect(api.openSupportLink).toHaveBeenCalledWith("https://github.com/hex-dump/perihub");
  });

  it("closes with Escape", async () => {
    render(Support);
    await fireEvent.click(screen.getByRole("button", { name: "Support me" }));
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("link")).toBeNull();
  });
});
