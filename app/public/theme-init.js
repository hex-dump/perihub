// Runs before the first paint (index.html), so the window does not open dark and then turn light. It is a file
// of its own because the app's Content-Security-Policy allows no inline script. Same rules as src/lib/theme.ts.
(function () {
  try {
    var root = document.documentElement;
    var theme = localStorage.getItem("perihub.theme");
    var accent = localStorage.getItem("perihub.accent");
    var light = theme === "light" || (theme !== "dark" && !!window.matchMedia && matchMedia("(prefers-color-scheme: light)").matches);
    root.dataset.theme = light ? "light" : "dark";
    if (accent && /^[a-z]+$/.test(accent)) root.dataset.accent = accent;
  } catch (e) {
    /* storage unavailable: the dark theme */
  }
})();
