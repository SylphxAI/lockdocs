import { fileURLToPath } from "node:url";
import { defineConfig } from "@solidjs/start/config";
import tailwind from "@tailwindcss/vite";

export default defineConfig({
  server: { preset: "node-server" },
  middleware: "./src/middleware.ts",
  vite: { resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } }, plugins: [tailwind()] },
});
