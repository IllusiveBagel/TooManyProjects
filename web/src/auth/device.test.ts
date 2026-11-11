import { describe, expect, it } from "vitest";
import { deviceName } from "./device";

describe("deviceName", () => {
  it("names common browsers", () => {
    expect(
      deviceName("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36"),
    ).toBe("Chrome on Windows");
    expect(
      deviceName("Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Mobile/15E148 Safari/604.1"),
    ).toBe("Safari on iPhone");
    expect(deviceName("Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0")).toBe("Firefox on Linux");
    expect(
      deviceName("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36 Edg/141.0"),
    ).toBe("Edge on Mac");
    expect(deviceName("curl/8.5.0")).toBe("curl");
    expect(deviceName(null)).toBe("Unknown browser");
  });
});
