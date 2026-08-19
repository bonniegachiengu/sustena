import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { vanillaExtractPlugin } from "@vanilla-extract/vite-plugin";

// Tauri drives this; the fixed port is what `tauri.conf.json` waits on.
export default defineConfig({
  plugins: [solid(), vanillaExtractPlugin()],
  clearScreen: false,
  // ★ 1421 (Tauri's own default), bound to IPv4. Port 5273 turned out to sit
  // inside a Windows reserved range (`netsh interface ipv4 show
  // excludedportrange` -> 5241-5340), and the default IPv6 bind fails with
  // EACCES on ::1 before it ever reaches IPv4.
  server: { host: "127.0.0.1", port: 1421, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
  build: { target: "esnext", sourcemap: true },
});
