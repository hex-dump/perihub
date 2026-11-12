// Shows the download for the visitor's system only. Without JavaScript, or on a system it does not
// recognize, both buttons stay visible.
(function () {
  const ua = navigator.userAgent;
  // iPadOS says it is a Mac: a touch screen tells them apart.
  const mobile = /Android|iPhone|iPad|iPod|Mobi/i.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
  const os = mobile ? "mobile" : /Windows/.test(ua) ? "windows" : /Mac OS X|Macintosh/.test(ua) ? "mac" : /Linux|X11|CrOS/.test(ua) ? "linux" : "";

  const buttons = document.querySelector(".os-buttons");
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

  // Clipboard access is often missing in the in-app browsers of social apps:
  // the link stays visible and selectable as text either way.
  document.querySelectorAll("[data-copy]").forEach((button) => {
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(button.dataset.copy);
        button.textContent = "Copied";
      } catch {
        const range = document.createRange();
        range.selectNodeContents(button.parentElement.querySelector(".url"));
        const selection = getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        button.textContent = "Select and copy";
      }
    });
  });
})();

// Devices: find a model by name, or show one level. Without JavaScript the finder stays hidden and
// every device is listed.
(function () {
  const finder = document.querySelector(".finder");
  if (!finder) return;
  finder.hidden = false;
  const input = document.getElementById("device-search");
  const buttons = finder.querySelectorAll("[data-filter]");
  const blocks = document.querySelectorAll(".brand-block");
  const empty = document.querySelector(".finder-empty");
  // "G 703", "g703" and "G-703" find the same mouse.
  const norm = (text) => text.toLowerCase().replace(/[^a-z0-9]+/g, "");
  const devices = [...document.querySelectorAll(".dev")].map((el) => ({
    el,
    levels: el.dataset.level.split(" "),
    models: el.dataset.models ? el.dataset.models.split("|") : [],
    text: norm(el.closest(".brand-block").querySelector(".brand-name").textContent + el.textContent + el.dataset.keys),
    match: el.querySelector(".dev-match"),
  }));
  let level = "all";

  function update() {
    const query = norm(input.value);
    let shown = 0;
    for (const d of devices) {
      const models = query ? d.models.filter((m) => norm(m).includes(query)) : [];
      const visible = (level === "all" || d.levels.includes(level)) && (!query || models.length > 0 || d.text.includes(query));
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
    blocks.forEach((block) => (block.hidden = !block.querySelector(".dev:not([hidden])")));
    empty.hidden = shown > 0;
  }

  input.addEventListener("input", update);
  buttons.forEach((button) =>
    button.addEventListener("click", () => {
      level = button.dataset.filter;
      buttons.forEach((b) => b.setAttribute("aria-pressed", String(b === button)));
      update();
    }),
  );
})();
