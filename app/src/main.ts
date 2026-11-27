import { mount } from "svelte";
import App from "./App.svelte";
import "./styles.css";
import { initTheme } from "./lib/prefs.svelte";
import { watchInterfaceErrors } from "./lib/interfaceErrors";

watchInterfaceErrors();
initTheme();
mount(App, { target: document.getElementById("app")! });
