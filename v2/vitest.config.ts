import { defineConfig } from "vitest/config";
import packageJson from "./package.json";

export default defineConfig({
  define: { __APP_VERSION__: JSON.stringify(packageJson.version) },
  test: { environment: "jsdom", include: ["tests/frontend/**/*.test.ts"], coverage: { provider: "v8", reporter: ["text", "html"], include: ["src/**/*.ts"] } }
});
