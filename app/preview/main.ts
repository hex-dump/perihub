import { mount } from "svelte";
import App from "../src/App.svelte";
import "../src/styles.css";
import { initTheme, setAccent, setTheme } from "../src/lib/prefs.svelte";

// The dark theme with the orange accent whatever the colors of the computer taking the screenshots,
// unless the address asks for others (?theme=light&accent=blue), to look at them.
const query = new URLSearchParams(location.search);
setTheme((query.get("theme") ?? "dark") as Parameters<typeof setTheme>[0]);
setAccent((query.get("accent") ?? "orange") as Parameters<typeof setAccent>[0]);
initTheme();

mount(App, { target: document.getElementById("app")! });
