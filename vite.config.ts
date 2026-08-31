import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // O Tauri serve o front na 5173 e espera que a porta seja fixa.
  server: { port: 5173, strictPort: true },
  // Sem isso o erro de compilação do Rust some atrás do overlay do Vite.
  clearScreen: false,
  build: { target: "chrome110", outDir: "dist", emptyOutDir: true },
});
