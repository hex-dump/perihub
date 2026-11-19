/** How long a link to a GitHub form may be. GitHub sends someone who is not signed in to its login page
 * with the whole link in `return_to`, encoded once more, and answers 414 (or 500) above about 8 KB. Measured
 * on 2026-11-19 while signed out: a 5000-character device request link opened, a 6000-character one did not
 * (its login link was 8413 characters), and a 7000-character one failed even before the login page. */
export const MAX_LINK = 8000;

const LOGIN = "https://github.com/login?return_to=";

/** Whether GitHub opens `url` for everyone: as it is, and wrapped in the login page's `return_to`. */
export function fitsGithubLink(url: string): boolean {
  return url.length <= MAX_LINK && LOGIN.length + encodeURIComponent(url).length <= MAX_LINK;
}
