import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { environment: "jsdom", include: ["tests/frontend/**/*.test.ts"], coverage: { provider: "v8", reporter: ["text", "html"], include: ["src/**/*.ts"] } }
});
