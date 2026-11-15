// The screenshots' stand-in for @tauri-apps/api/event: nothing ever happens.
export type UnlistenFn = () => void;

export async function listen(): Promise<UnlistenFn> {
  return () => {};
}
