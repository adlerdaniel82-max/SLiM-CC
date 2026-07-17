import { describe, expect, it } from "vitest";
import { escapeHtml, formatBytes, pathName } from "../../src/core/dom";

describe("presentation helpers", () => {
  it("escapes untrusted mod metadata", () => expect(escapeHtml(`<img src=x onerror='x'>`)).toBe("&lt;img src=x onerror=&#39;x&#39;&gt;"));
  it("formats sizes compactly", () => expect(formatBytes(1024 * 1024)).toBe("1.0 MB"));
  it("derives names from Linux and Windows paths", () => { expect(pathName("/mods/Test.7z")).toBe("Test.7z"); expect(pathName("C:\\mods\\Test.7z")).toBe("Test.7z"); });
});
