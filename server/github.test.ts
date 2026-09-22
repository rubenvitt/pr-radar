import { describe, expect, it } from "vitest";
import { parseClientToken } from "./github.js";

describe("parseClientToken", () => {
  it("nimmt übliche GitHub-Token an", () => {
    expect(parseClientToken("ghp_0123456789abcdef")).toBe("ghp_0123456789abcdef");
    expect(parseClientToken("  github_pat_11ABCDE_xyz  ")).toBe("github_pat_11ABCDE_xyz");
  });

  it("lehnt Leeres, Kurzes und Zeilenumbrüche/Steuerzeichen ab", () => {
    expect(parseClientToken(undefined)).toBeNull();
    expect(parseClientToken("   ")).toBeNull();
    expect(parseClientToken("kurz")).toBeNull();
    expect(parseClientToken("ghp_abc\ndef_header_injection")).toBeNull();
    expect(parseClientToken("ghp_abc def_mit_leerzeichen")).toBeNull();
    expect(parseClientToken("x".repeat(513))).toBeNull();
  });
});
