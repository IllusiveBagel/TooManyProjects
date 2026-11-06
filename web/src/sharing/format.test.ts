import { describe, expect, it } from "vitest";
import { initials, sharedLabel } from "./format";

describe("initials", () => {
  it("takes first and last words", () => {
    expect(initials("Sam Lee")).toBe("SL");
    expect(initials("Mary Ann Jones")).toBe("MJ");
  });
  it("uses two letters of a single name", () => {
    expect(initials("logan")).toBe("LO");
    expect(initials("  ")).toBe("?");
  });
});

describe("sharedLabel", () => {
  it("says whose it is and what you can do", () => {
    expect(sharedLabel({ role: "owner", owner_name: "Logan" })).toBeNull();
    expect(sharedLabel({ role: "editor", owner_name: "Logan" })).toBe("Logan’s · can edit");
    expect(sharedLabel({ role: "viewer", owner_name: "Logan" })).toBe("Logan’s · view only");
  });
});
