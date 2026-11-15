import { mount } from "svelte";
import App from "../src/App.svelte";
import "../src/styles.css";

// The US layout whatever the language of the computer taking the screenshots.
localStorage.setItem("perihub.layout", "ansi-us");

mount(App, { target: document.getElementById("app")! });
