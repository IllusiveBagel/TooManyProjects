import { describe, expect, it } from "vitest";
import { unreachableFromInternet } from "./AiAccessPanel";

describe("unreachableFromInternet", () => {
  const check = (u: string) => unreachableFromInternet(new URL(u));
  it("accepts public https addresses", () => {
    expect(check("https://projects.example.com")).toBe(false);
    expect(check("https://tmp.tail1234.ts.net")).toBe(false);
  });
  it("flags plain http and home-network addresses", () => {
    expect(check("http://projects.example.com")).toBe(true);
    expect(check("http://192.168.1.20:8080")).toBe(true);
    expect(check("https://10.0.0.5")).toBe(true);
    expect(check("https://172.20.1.1")).toBe(true);
    expect(check("https://localhost:8080")).toBe(true);
    expect(check("https://nas.local")).toBe(true);
    expect(check("https://server")).toBe(true);
  });
});
