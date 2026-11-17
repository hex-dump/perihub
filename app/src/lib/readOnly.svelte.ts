import { api, type WriteEntry } from "./api";

/** Read-only mode, as the app has it: the pages grey out what would change a device. */
export const readOnly = $state({ on: false });

/** Follows read-only mode, also when the tray menu changes it. Returns the function that stops. */
export function followReadOnly(): () => void {
  api.readOnly().then((on) => (readOnly.on = on), () => {});
  const unlisten = api.onReadOnly((on) => (readOnly.on = on));
  return () => {
    unlisten.then((fn) => fn());
  };
}

export async function setReadOnly(on: boolean): Promise<void> {
  readOnly.on = await api.setReadOnly(on);
}

/** How many writes a test report lists: the form's link must stay short. */
export const REPORT_WRITES = 12;

const pad = (n: number) => String(n).padStart(2, "0");

/** "14:03:27", in local time. */
export function writeTime(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** The writes of this session as lines for a test report: what was written, never the bytes
 * (a macro saved to a mouse would show what it types). */
export function writeLogLines(entries: WriteEntry[], readOnlyOn: boolean, max = REPORT_WRITES): string[] {
  const refused = entries.filter((e) => e.result.startsWith("refused")).length;
  const head = `Writes this session: ${entries.length - refused} sent, ${refused} refused${readOnlyOn ? " (read-only mode on)" : ""}`;
  if (entries.length === 0) return [head];
  const shown = entries.slice(-max);
  return [
    entries.length > shown.length ? `${head}, last ${shown.length}:` : `${head}:`,
    ...shown.map((e) => `  ${writeTime(e.time)} ${e.device}: ${e.kind}, ${e.bytes} bytes, ${e.result}`),
  ];
}

/** A test report with the writes of this session at its end. */
export function withWriteLog(report: string, entries: WriteEntry[], readOnlyOn: boolean): string {
  return [report, ...writeLogLines(entries, readOnlyOn)].join("\n");
}
