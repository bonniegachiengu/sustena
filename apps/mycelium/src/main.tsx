/* @refresh reload */
import { render } from "solid-js/web";
// The design system, imported once so its global rules (the reset, the body
// ground, the scrollbars) are in the bundle before anything renders.
import "./ui/ui.css";
import App from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("#root is missing from index.html");
render(() => <App />, root);
