// "Record the official software" on the Send a report page: the steps the user goes through while
// PeriHub records what the official software sends the device (docs/captures.md).

/** What official software can change, as the device request form lists it; each one ticked is a step. */
export interface Capability {
  id: string;
  label: string;
  /** What to do in the official software during the step. */
  step: string;
}

export const CAPABILITIES: Capability[] = [
  { id: "dpi", label: "DPI", step: "DPI: change one DPI level to another value, then to a third one" },
  { id: "polling", label: "Polling rate", step: "Polling rate: pick each rate the software offers, one after the other" },
  { id: "lighting", label: "Lighting", step: "Lighting: change the effect, then the colour, then the brightness" },
  { id: "buttons", label: "Button remapping", step: "Buttons: give one button another action, then put it back" },
  { id: "macros", label: "Macros", step: "Macros: record a short macro and assign it to a button" },
  { id: "profiles", label: "Onboard profiles", step: "Profiles: switch to another profile, then back" },
  { id: "battery", label: "Battery or sleep", step: "Battery: open the page that shows the battery, and change the sleep time if there is one" },
];

/** The last step of every recording. */
export const PUT_BACK = "Put back every setting you changed";

/** The longest note or step a user can write: the recording has to fit in a report. */
export const MAX_STEP = 200;

/** The steps for what was ticked, in the form's order, then one per line of "something else", then putting back. */
export function recordingSteps(picked: string[], other: string): string[] {
  const steps = CAPABILITIES.filter((c) => picked.includes(c.id)).map((c) => c.step);
  for (const line of other.split("\n")) {
    const text = line.trim().slice(0, MAX_STEP);
    if (text) steps.push(text);
  }
  return steps.length ? [...steps, PUT_BACK] : [];
}

/** A step's name in the recording: its first words, and what the user noted they changed. */
export function stepName(step: string, note: string): string {
  const short = step.split(":")[0].slice(0, MAX_STEP);
  const said = note.trim().slice(0, MAX_STEP);
  return said ? `${short} (${said})` : short;
}
