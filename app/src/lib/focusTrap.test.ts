import { describe, expect, it } from "vitest";
import { trapFocus } from "./focusTrap";

function dialog() {
  document.body.innerHTML = `<button id="opener">open</button>
    <div id="d"><input id="a" /><button id="cancel" data-autofocus>Cancel</button><button id="ok">OK</button></div>`;
  const opener = document.getElementById("opener")!;
  opener.focus();
  const node = document.getElementById("d")!;
  return { opener, node, action: trapFocus(node) };
}

const tab = (shiftKey = false) => {
  const e = new KeyboardEvent("keydown", { key: "Tab", shiftKey, cancelable: true, bubbles: true });
  document.dispatchEvent(e);
  return e;
};

describe("trapFocus", () => {
  it("starts on the safe button, not on the one that confirms", () => {
    const { action } = dialog();
    expect(document.activeElement?.id).toBe("cancel");
    action.destroy();
  });

  it("keeps Tab inside and gives the focus back", () => {
    const { opener, action } = dialog();
    document.getElementById("ok")!.focus();
    expect(tab().defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe("a");
    expect(tab(true).defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe("ok");
    action.destroy();
    expect(document.activeElement).toBe(opener);
  });
});
