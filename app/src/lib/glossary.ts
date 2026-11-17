// Short explanations of the words a newcomer may not know, shown on hover by Term.svelte.
export const GLOSSARY = {
  dpi: "DPI (dots per inch): how far the pointer moves when the mouse moves one inch. Higher is faster.",
  polling: "Polling rate: how many times per second the mouse tells the computer where it is. 1000 Hz is usual for gaming.",
  sniper: "Sniper level: the DPI used while you hold the DPI Shift button, usually a slower one for precise aiming.",
  gshift: "G-Shift: a second set of button functions, used while you hold the button set to G-Shift.",
  onboard:
    "Onboard memory: the mouse or keyboard keeps its settings itself, so they work on any computer, even without PeriHub.",
} as const;

export type GlossaryTerm = keyof typeof GLOSSARY;
