import { describe, it, expect } from "vitest";
import {
  computeMute24h,
  computeMute3d,
  computeMute1w,
  computeMuteForever,
  formatMuteExpiry,
  muteOptions,
} from "./mute";

describe("mute utilities", () => {
  describe("computeMute24h", () => {
    it("returns a timestamp ~24 hours in the future", () => {
      const now = Math.floor(Date.now() / 1000);
      const result = computeMute24h();
      expect(result).toBeGreaterThan(now + 23 * 3600);
      expect(result).toBeLessThan(now + 25 * 3600);
    });
  });

  describe("computeMute3d", () => {
    it("returns a timestamp ~3 days in the future", () => {
      const now = Math.floor(Date.now() / 1000);
      const result = computeMute3d();
      expect(result).toBeGreaterThan(now + 71 * 3600);
      expect(result).toBeLessThan(now + 73 * 3600);
    });
  });

  describe("computeMute1w", () => {
    it("returns a timestamp ~7 days in the future", () => {
      const now = Math.floor(Date.now() / 1000);
      const result = computeMute1w();
      expect(result).toBeGreaterThan(now + 167 * 3600);
      expect(result).toBeLessThan(now + 169 * 3600);
    });
  });

  describe("computeMuteForever", () => {
    it("returns null", () => {
      expect(computeMuteForever()).toBeNull();
    });
  });

  describe("formatMuteExpiry", () => {
    it("returns 'Forever' for null", () => {
      expect(formatMuteExpiry(null)).toBe("Forever");
    });

    it("returns a formatted date string for a timestamp", () => {
      // 1759387200 = 2025-10-02T06:40:00Z (unix seconds)
      const ts = 1759387200;
      const formatted = formatMuteExpiry(ts);
      expect(formatted).toBeTruthy();
      expect(formatted).not.toBe("Forever");
      // The formatted output must round-trip to the same calendar day as the input
      const expected = new Date(ts * 1000);
      expect(formatted).toContain(String(expected.getDate()));
    });
  });

  describe("muteOptions", () => {
    it("has 4 options", () => {
      expect(muteOptions).toHaveLength(4);
    });

    it("each option has id, label, description, and compute", () => {
      for (const opt of muteOptions) {
        expect(opt.id).toBeTruthy();
        expect(opt.label).toBeTruthy();
        expect(opt.description).toBeTruthy();
        expect(typeof opt.compute).toBe("function");
      }
    });

    it("ids are unique", () => {
      const ids = muteOptions.map((o) => o.id);
      expect(new Set(ids).size).toBe(ids.length);
    });
  });
});
