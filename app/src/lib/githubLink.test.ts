import { describe, expect, it } from "vitest";
import { fitsGithubLink } from "./githubLink";

describe("fitsGithubLink", () => {
  const base = "https://github.com/hex-dump/perihub/issues/new?template=device_request.yml&details=";
  const body = (n: number) => base + encodeURIComponent("ab 12 cd 34\n".repeat(n));

  it("takes a short link", () => expect(fitsGithubLink(body(10))).toBe(true));
  // The lengths measured on GitHub while signed out: 5000 opened, 6000 did not.
  it("refuses a link whose login wrapping is too long", () => {
    const six = body(2000).slice(0, 6000);
    expect(six.length).toBe(6000);
    expect(fitsGithubLink(six)).toBe(false);
    expect(fitsGithubLink(body(2000).slice(0, 5000))).toBe(true);
  });
  it("refuses a link over 8 KB as it is", () => expect(fitsGithubLink("x".repeat(8001))).toBe(false));
});
