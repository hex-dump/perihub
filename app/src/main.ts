import { mount } from "svelte";
import App from "./App.svelte";
import "./styles.css";
import { initTheme } from "./lib/prefs.svelte";

initTheme();
mount(App, { target: document.getElementById("app")! });
