// Runs before the first paint (index.html), so the window does not open dark and then turn light. It is a file
// of its own because the app's Content-Security-Policy allows no inline script. Same rules as src/lib/theme.ts.
(function () {
  try {
    var root = document.documentElement;
    var theme = localStorage.getItem("perihub.theme");
    var accent = localStorage.getItem("perihub.accent");
    // Linux: the app's answer first (see __PERIHUB_SYSTEM_THEME__ in src/lib/theme.ts), WebKitGTK's matchMedia can be wrong.
    var system = window.__PERIHUB_SYSTEM_THEME__;
    var light =
      theme === "light" ||
      (theme !== "dark" &&
        (system === "light" || (system !== "dark" && !!window.matchMedia && matchMedia("(prefers-color-scheme: light)").matches)));
    root.dataset.theme = light ? "light" : "dark";
    if (accent && /^[a-z]+$/.test(accent)) root.dataset.accent = accent;
  } catch (e) {
    /* storage unavailable: the dark theme */
  }
})();
