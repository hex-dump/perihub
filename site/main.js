// Shows the download for the visitor's system only. Without JavaScript, or on a system it does not
// recognize, both buttons stay visible.
(function () {
  const ua = navigator.userAgent;
  // iPadOS says it is a Mac: a touch screen tells them apart.
  const mobile = /Android|iPhone|iPad|iPod|Mobi/i.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
  const os = mobile ? "mobile" : /Windows/.test(ua) ? "windows" : /Mac OS X|Macintosh/.test(ua) ? "mac" : /Linux|X11|CrOS/.test(ua) ? "linux" : "";

  const buttons = document.querySelector(".os-buttons");
  if (!buttons) return; // pages without downloads, like the privacy policy
  const show = (selector) => (document.querySelector(selector).hidden = false);

  if (os === "windows") {
    buttons.querySelector('[data-os="linux"]').hidden = true;
  } else if (os === "linux") {
    buttons.hidden = true;
    show(".linux-choices");
    // In the Download section too, the visitor's system comes first.
    const linux = document.getElementById("linux");
    linux.parentElement.prepend(linux);
  } else if (os === "mobile") {
    buttons.hidden = true;
    show(".mobile-note");
  } else if (os === "mac") {
    buttons.hidden = true;
    show(".mac-note");
  }

  // Clipboard access is often missing in the in-app browsers of social apps: the link stays visible
  // and selectable as text either way. The result goes in a status line next to the button, so
  // screen readers announce it too.
  document.querySelectorAll("[data-copy]").forEach((button) => {
    const status = button.parentElement.querySelector(".copy-status");
    let timer;
    button.addEventListener("click", async () => {
      clearTimeout(timer);
      let message;
      try {
        await navigator.clipboard.writeText(button.dataset.copy);
        message = "Copied";
      } catch {
        const range = document.createRange();
        range.selectNodeContents(button.parentElement.querySelector(".url"));
        const selection = getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        message = "Link selected: copy it from the menu";
      }
      status.textContent = "";
      requestAnimationFrame(() => (status.textContent = message));
      timer = setTimeout(() => (status.textContent = ""), 4000);
    });
  });
})();

// On narrow screens the header links are in a popover: close it when a link is picked. Browsers
// without popovers (older iPhones, and the in-app browsers that use their engine) get the same menu
// opened and closed by this script.
(function () {
  const menu = document.getElementById("site-menu");
  const button = document.querySelector(".menu-button");
  if (!menu || !button) return;
  if ("popover" in HTMLElement.prototype) {
    menu.addEventListener("click", (event) => {
      if (event.target.closest("a") && menu.matches(":popover-open")) menu.hidePopover();
    });
    return;
  }
  document.documentElement.classList.add("no-popover");
  button.setAttribute("aria-expanded", "false");
  const toggle = (open) => {
    menu.classList.toggle("open", open);
    button.setAttribute("aria-expanded", String(open));
  };
  button.addEventListener("click", () => toggle(!menu.classList.contains("open")));
  menu.addEventListener("click", (event) => event.target.closest("a") && toggle(false));
  document.addEventListener("keydown", (event) => event.key === "Escape" && toggle(false));
})();

// Devices: find a model by name. Brand rows start closed; while searching, the rows with a match open
// by themselves and close again when the search is cleared, unless the visitor opened them. Without
// JavaScript the search stays hidden and every brand row opens on click.
(function () {
  const finder = document.querySelector(".finder");
  if (!finder) return;
  finder.hidden = false;
  const input = document.getElementById("device-search");
  const status = finder.querySelector(".finder-status");
  const rows = [...document.querySelectorAll(".brand-row")];
  const groups = document.querySelectorAll(".level-group");
  const empty = document.querySelector(".finder-empty");
  // "G 703", "g703" and "G-703" find the same mouse.
  const norm = (text) => text.toLowerCase().replace(/[^a-z0-9]+/g, "");
  const devices = [...document.querySelectorAll(".dev")].map((el) => ({
    el,
    models: el.dataset.models ? el.dataset.models.split("|") : [],
    text: norm(el.closest(".brand-row").querySelector(".brand-name").textContent + el.textContent + el.dataset.keys),
    match: el.querySelector(".dev-match"),
  }));
  let announce;

  function update() {
    const query = norm(input.value);
    let shown = 0;
    for (const d of devices) {
      const models = query ? d.models.filter((m) => norm(m).includes(query)) : [];
      const visible = !query || models.length > 0 || d.text.includes(query);
      d.el.hidden = !visible;
      // Name the models of a family that match, so "G703" shows up inside "26 more G mice".
      const named = visible && models.length > 0 && !(d.models.length === 1 && models.length === 1);
      d.match.hidden = !named;
      if (named) {
        const list = models.slice(0, 6).join(", ");
        d.match.textContent = `Matches: ${list}${models.length > 6 ? ` and ${models.length - 6} more` : ""}`;
      }
      shown += visible;
    }
    let brands = 0;
    for (const row of rows) {
      const has = !!row.querySelector(".dev:not([hidden])");
      row.hidden = !has;
      brands += has;
      if (query && has && !row.open) {
        row.open = true;
        row.dataset.auto = "";
      } else if ((!query || !has) && "auto" in row.dataset) {
        row.open = false;
        delete row.dataset.auto;
      }
    }
    groups.forEach((group) => (group.hidden = !group.querySelector(".brand-row:not([hidden])")));
    empty.hidden = shown > 0;
    // Said once the typing pauses, not at every key.
    clearTimeout(announce);
    announce = setTimeout(() => {
      status.textContent = !query ? "" : shown === 0 ? "No matches" : `${shown} ${shown === 1 ? "entry" : "entries"} in ${brands} ${brands === 1 ? "brand" : "brands"}, opened below`;
    }, 400);
  }
  input.addEventListener("input", update);
  // A row the visitor opens or closes is theirs: the search leaves it as it is.
  rows.forEach((row) => row.querySelector("summary").addEventListener("click", () => delete row.dataset.auto));

  // A link to a brand row, or to something inside one (like #logitech), opens it.
  function openTarget() {
    const target = location.hash.length > 1 && document.getElementById(decodeURIComponent(location.hash.slice(1)));
    for (let el = target; el; el = el.parentElement.closest("details")) {
      if (el.matches("details")) el.open = true;
      if (!el.parentElement) break;
    }
  }
  addEventListener("hashchange", openTarget);
  openTarget();
})();
