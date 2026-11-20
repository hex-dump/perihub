declare module "virtual:perihub-release-notes" {
  import type { ReleaseNotes } from "./lib/changelog";
  const notes: ReleaseNotes | null;
  export default notes;
}
