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
