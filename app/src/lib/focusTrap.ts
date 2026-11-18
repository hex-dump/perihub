/**
 * Svelte action for a modal dialog: puts the focus inside it (on the element marked
 * `data-autofocus`, else the first one that can take it), keeps Tab and Shift+Tab inside, and gives
 * the focus back to what opened the dialog when it closes. Escape stays with each dialog.
 * Mark the safe button (Cancel, Undo, Close) with `data-autofocus`, so that Enter never confirms
 * a write by accident.
 */
const FOCUSABLE =
  "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

export function focusables(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.hidden);
}

export function trapFocus(node: HTMLElement) {
  const opener = document.activeElement;
  (node.querySelector<HTMLElement>("[data-autofocus]") ?? focusables(node)[0])?.focus();

  function keydown(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const items = focusables(node);
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (!items.length || !node.contains(active)) {
      e.preventDefault();
      first?.focus();
    } else if (e.shiftKey && active === first) {
      e.preventDefault();
      last?.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first?.focus();
    }
  }
  document.addEventListener("keydown", keydown);

  return {
    destroy() {
      document.removeEventListener("keydown", keydown);
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    },
  };
}
