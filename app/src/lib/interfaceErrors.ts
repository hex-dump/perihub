// Errors of the window's own code. A page that throws while it is drawn can leave the window without
// changing any more (issue #40: Settings and Test my device needed a reload), and nothing behind to tell
// why. Each error goes into PeriHub's log file, and an error nobody caught is also said on screen, in
// plain HTML: it is shown even when the pages stopped updating.
import { api } from "./api";

/** The error, with the first lines of where it happened, on one line. */
export function describeError(error: unknown): string {
  if (error instanceof Error) {
    const where = (error.stack ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith(`${error.name}:`))
      .slice(0, 4);
    return [`${error.name}: ${error.message}`, ...where].join(" | ");
  }
  return typeof error === "string" ? error : JSON.stringify(error) ?? String(error);
}

/** Into the log file; nothing more if the app cannot be reached (the preview, the tests). */
export function logInterfaceError(context: string, error: unknown) {
  try {
    void api.logInterfaceError(`${context}: ${describeError(error)}`)?.catch?.(() => {});
  } catch {
    /* not in the app */
  }
}

const BANNER_ID = "interface-error";

function showBanner(error: unknown) {
  let banner = document.getElementById(BANNER_ID);
  if (!banner) {
    banner = document.createElement("div");
    banner.id = BANNER_ID;
    banner.className = "notice error";
    banner.setAttribute("role", "alert");
    document.body.prepend(banner);
  }
  const message = error instanceof Error ? error.message : String(error);
  banner.textContent =
    `Something went wrong in PeriHub's window: ${message}. If a page stops changing, press Ctrl+R to reload it. ` +
    "The error is also in PeriHub's log file (Settings → Open log folder).";
}

/** Logs the errors nobody caught, and says the thrown ones on screen. Returns what stops it. */
export function watchInterfaceErrors(): () => void {
  const onError = (event: ErrorEvent) => {
    // Without an error object it is a browser notice (such as a ResizeObserver loop), not PeriHub's code.
    if (!(event.error instanceof Error)) return;
    logInterfaceError("uncaught", event.error);
    showBanner(event.error);
  };
  // Commands the backend refused are plain texts, handled where they matter: only the window's own errors.
  const onRejection = (event: PromiseRejectionEvent) => {
    if (event.reason instanceof Error) logInterfaceError("unhandled promise", event.reason);
  };
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}
