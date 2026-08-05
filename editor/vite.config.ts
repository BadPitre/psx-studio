import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Tauri sert le build depuis dist/.
  build: { outDir: "dist", target: "es2022" },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Ne jamais surveiller les artefacts cargo : sous Windows, watcher
      // les .exe en cours d'écriture par le build Rust fait crasher Vite
      // (EBUSY), ce qui tue `cargo tauri dev`.
      ignored: ["**/src-tauri/**"],
    },
  },
});
