import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";
import { convAtlasData } from "./dataServer.ts";

export default defineConfig({
  plugins: [react(), convAtlasData()],
  server: { port: 5181, strictPort: false },
  test: { environment: "node" },
});
