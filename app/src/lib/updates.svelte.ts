import { api, errorText, type UpdateInfo, type UpdateProgress } from "./api";

/** Update state, shared by the sidebar and the settings page. */
export const updates = $state({
  available: null as UpdateInfo | null,
  checking: false,
  installing: false,
  /** A check finished in this session (to say "up to date"). */
  checked: false,
  error: null as string | null,
  /** The download of the installer, which starts as soon as an update is found. */
  progress: null as UpdateProgress | null,
  /** The version an update just installed, until the message is closed. */
  installed: null as string | null,
});

/** What the update button says while it works. */
export function installingText(progress: UpdateProgress | null): string {
  if (!progress) return "Starting…";
  if (!progress.total) return "Downloading…";
  if (progress.received < progress.total) {
    return `Downloading ${Math.floor((progress.received * 100) / progress.total)}%…`;
  }
  return "Installing…";
}

/** What happens once the installer starts: there is no installer window. */
export const INSTALL_NOTE = "PeriHub closes, installs the update without any window and opens again by itself in a few seconds.";

export async function checkForUpdate() {
  if (updates.checking || updates.installing) return;
  updates.checking = true;
  updates.error = null;
  try {
    updates.available = await api.checkUpdate();
    updates.checked = true;
  } catch (e) {
    updates.error = errorText(e);
  } finally {
    updates.checking = false;
  }
}

/** Runs the installer, downloading it first if needed: PeriHub closes and the new version starts. */
export async function installUpdate() {
  updates.installing = true;
  updates.error = null;
  try {
    await api.installUpdate();
  } catch (e) {
    updates.error = errorText(e);
    updates.installing = false;
  }
}
