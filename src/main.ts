import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

const target = document.getElementById("app");
if (!target) {
  throw new Error("index.html is missing the #app mount element.");
}

const app = mount(App, { target });

export default app;
