import { describe, expect, it } from "vitest";
import { hexPretty, hexToText } from "./hex";

describe("hexToText", () => {
  it("decodes ASCII", () => {
    expect(hexToText("68656c6c6f")).toBe("hello");
  });

  it("decodes multi-byte UTF-8", () => {
    // "你好" 的 UTF-8 编码
    expect(hexToText("e4bda0e5a5bd")).toBe("\u4f60\u597d");
  });

  it("replaces invalid bytes with U+FFFD instead of throwing", () => {
    const out = hexToText("fffe41");
    expect(out).toContain("A");
    expect(out).toContain("");
  });

  it("empty string returns empty string", () => {
    expect(hexToText("")).toBe("");
  });
});

describe("hexPretty", () => {
  it("groups pairs and trims trailing space", () => {
    expect(hexPretty("7b22636d64")).toBe("7b 22 63 6d 64");
  });

  it("single byte has no space appended", () => {
    expect(hexPretty("ff")).toBe("ff");
  });

  it("empty string returns empty string", () => {
    expect(hexPretty("")).toBe("");
  });
});
