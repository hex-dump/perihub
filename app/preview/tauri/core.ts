// The screenshots' stand-in for @tauri-apps/api/core (vite.preview.config.ts).
import { answers } from "../fake";

export async function invoke<T>(command: string): Promise<T> {
  if (command in answers) return structuredClone(answers[command]) as T;
  return new Promise<T>(() => {});
}
