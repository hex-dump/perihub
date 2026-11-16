import { mount } from "svelte";
import App from "../src/App.svelte";
import "../src/styles.css";
import { initTheme, setAccent, setTheme } from "../src/lib/prefs.svelte";

// The US layout whatever the language of the computer taking the screenshots.
localStorage.setItem("perihub.layout", "ansi-us");

// The dark theme with the orange accent whatever the colors of the computer taking the screenshots.
setTheme("dark");
setAccent("orange");
initTheme();

mount(App, { target: document.getElementById("app")! });
