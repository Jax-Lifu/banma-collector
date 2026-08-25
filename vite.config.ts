import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  optimizeDeps: {
    include: [
      "react",
      "react/jsx-dev-runtime",
      "react-dom/client",
      "@tanstack/react-query",
      "react-router-dom",
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
      "@tauri-apps/plugin-dialog",
      "lucide-react",
      "zod",
      "zustand",
      "clsx",
      "tailwind-merge",
      "class-variance-authority",
    ],
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
});
