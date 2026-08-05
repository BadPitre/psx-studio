import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Tauri (Phase 4 suite) sert le build depuis dist/.
  build: { outDir: "dist", target: "es2022" },
  server: { port: 5173, strictPort: true },
});
