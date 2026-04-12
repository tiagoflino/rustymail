import { describe, it, expect, vi, afterEach } from "vitest";
import { computeLaterToday, computeTomorrowMorning, computeNextWeek, formatSnoozePreview } from "./snooze";

describe("snooze utilities", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  describe("computeLaterToday", () => {
    it("returns +3h before 6 PM", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 8, 14, 0, 0));
      const result = computeLaterToday();
      const expected = Math.floor(new Date(2026, 3, 8, 17, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });

    it("returns tomorrow 9 AM at exactly 6 PM", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 8, 18, 0, 0));
      const result = computeLaterToday();
      const expected = Math.floor(new Date(2026, 3, 9, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });

    it("returns tomorrow 9 AM after 6 PM", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 8, 22, 30, 0));
      const result = computeLaterToday();
      const expected = Math.floor(new Date(2026, 3, 9, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });
  });

  describe("computeTomorrowMorning", () => {
    it("returns next day 9 AM", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 8, 14, 0, 0));
      const result = computeTomorrowMorning();
      const expected = Math.floor(new Date(2026, 3, 9, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });

    it("works at 1 AM", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 8, 1, 0, 0));
      const result = computeTomorrowMorning();
      const expected = Math.floor(new Date(2026, 3, 9, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });
  });

  describe("computeNextWeek", () => {
    it("returns next Monday from Wednesday", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 8, 14, 0, 0));
      const result = computeNextWeek();
      const expected = Math.floor(new Date(2026, 3, 13, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });

    it("returns next Monday from Monday (7 days later)", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 6, 14, 0, 0));
      const result = computeNextWeek();
      const expected = Math.floor(new Date(2026, 3, 13, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });

    it("returns tomorrow Monday from Sunday", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 5, 14, 0, 0));
      const result = computeNextWeek();
      const expected = Math.floor(new Date(2026, 3, 6, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });

    it("returns next Monday from Saturday", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 3, 4, 14, 0, 0));
      const result = computeNextWeek();
      const expected = Math.floor(new Date(2026, 3, 6, 9, 0, 0).getTime() / 1000);
      expect(result).toBe(expected);
    });
  });

  describe("formatSnoozePreview", () => {
    it("formats timestamp into readable date string", () => {
      const ts = Math.floor(new Date(2026, 3, 9, 9, 0, 0).getTime() / 1000);
      const result = formatSnoozePreview(ts);
      expect(result).toContain("Apr");
      expect(result).toContain("9");
    });
  });
});
