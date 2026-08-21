/* @refresh reload */
import { render } from "solid-js/web";

// ★★★ The fonts ship INSIDE the app, and this is a correctness fix as much as a
// speed one. index.html used to <link> them from fonts.googleapis.com, so an
// installed native app reached across the network to render its own text --
// slow on a phone, and on a device with no connection it silently fell back to
// whatever the system happened to have. These imports let Vite bundle the woff2
// files into the same asset directory the engine already embeds, so the app
// looks identical offline and on. Only the weights actually used: an unrendered
// font file is dead weight in a binary that already carries an engine.
// ★ The `latin-` entrypoints, not the bare ones. The bare ones declare every
//   unicode subset, and although a browser only DOWNLOADS the subset it needs,
//   Vite copies them all into dist and the engine embeds dist wholesale -- so
//   the installed app would carry Cyrillic and Vietnamese it will never draw.
import "@fontsource/dm-mono/latin-400.css";
import "@fontsource/dm-mono/latin-500.css";
import "@fontsource/inter-tight/latin-400.css";
import "@fontsource/inter-tight/latin-500.css";
import "@fontsource/inter-tight/latin-600.css";

// The design system, imported once so its global rules (the reset, the body
// ground, the scrollbars) are in the bundle before anything renders.
import "./ui/ui.css";
import App from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("#root is missing from index.html");
render(() => <App />, root);
