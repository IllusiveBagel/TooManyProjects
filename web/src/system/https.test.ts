import { describe, expect, it } from "vitest";
import { covers, httpsOrigin, type HttpsInfo } from "./queries";
import { trustCommand } from "./HttpsPanel";

const info: HttpsInfo = {
  mode: "generated",
  port: 8443,
  hosts: [
    { host: "localhost", added: false },
    { host: "192.168.1.50", added: true },
  ],
  expires_at: null,
  fingerprint: null,
};

describe("https address", () => {
  it("keeps an https page as it is", () => {
    expect(httpsOrigin({ protocol: "https:", hostname: "p.example.com", origin: "https://p.example.com" }, info)).toBe("https://p.example.com");
  });
  it("moves an http page to the HTTPS port", () => {
    expect(httpsOrigin({ protocol: "http:", hostname: "192.168.1.50", origin: "http://192.168.1.50:8080" }, info)).toBe(
      "https://192.168.1.50:8443",
    );
  });
  it("has nothing when HTTPS is off", () => {
    expect(httpsOrigin({ protocol: "http:", hostname: "x", origin: "http://x" }, { ...info, mode: "off", port: null })).toBeNull();
  });
  it("knows which hosts the certificate covers", () => {
    expect(covers(info, "192.168.1.50")).toBe(true);
    expect(covers(info, "nas.local")).toBe(false);
    expect(covers({ ...info, mode: "own" }, "nas.local")).toBe(true);
  });
  it("gives a trust line per computer", () => {
    expect(trustCommand("windows")).toContain("setx NODE_EXTRA_CA_CERTS");
    expect(trustCommand("mac")).toContain("~/.zshrc");
  });
});
