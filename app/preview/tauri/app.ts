// The screenshots' stand-in for @tauri-apps/api/app.
import pkg from "../../package.json";

export async function getVersion(): Promise<string> {
  return pkg.version;
}
