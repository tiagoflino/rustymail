import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SnoozePopover from "./SnoozePopover.svelte";

describe("SnoozePopover", () => {
  let onsnooze;
  let onclose;

  beforeEach(() => {
    onsnooze = vi.fn();
    onclose = vi.fn();
  });

  it("renders 3 snooze options", () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    expect(screen.getByText("Later Today")).toBeTruthy();
    expect(screen.getByText("Tomorrow Morning")).toBeTruthy();
    expect(screen.getByText("Next Week")).toBeTruthy();
  });

  it("renders header text", () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    expect(screen.getByText("Snooze until...")).toBeTruthy();
  });

  it("fires onsnooze with timestamp when clicking Later Today", async () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = screen.getByText("Later Today").closest("button");
    await fireEvent.click(btn);
    expect(onsnooze).toHaveBeenCalledTimes(1);
    const ts = onsnooze.mock.calls[0][0];
    expect(typeof ts).toBe("number");
    // Should be in the future (within reasonable range)
    const now = Math.floor(Date.now() / 1000);
    expect(ts).toBeGreaterThan(now);
  });

  it("Later Today computes +3h when before 6 PM", async () => {
    // Mock Date to 2 PM
    const mockDate = new Date(2026, 3, 8, 14, 0, 0); // April 8, 2026 2 PM
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = screen.getByText("Later Today").closest("button");
    await fireEvent.click(btn);
    
    const ts = onsnooze.mock.calls[0][0];
    const expected = Math.floor((mockDate.getTime() + 3 * 60 * 60 * 1000) / 1000);
    expect(ts).toBe(expected);

    vi.useRealTimers();
  });

  it("Later Today rolls to tomorrow 9 AM when after 6 PM", async () => {
    // Mock Date to 7 PM
    const mockDate = new Date(2026, 3, 8, 19, 0, 0); // April 8, 2026 7 PM
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = screen.getByText("Later Today").closest("button");
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 9, 9, 0, 0); // April 9, 2026 9 AM
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));

    vi.useRealTimers();
  });

  it("Tomorrow Morning computes next day 9 AM", async () => {
    const mockDate = new Date(2026, 3, 8, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = screen.getByText("Tomorrow Morning").closest("button");
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 9, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));

    vi.useRealTimers();
  });

  it("Next Week computes next Monday 9 AM", async () => {
    // April 8, 2026 is a Wednesday
    const mockDate = new Date(2026, 3, 8, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = screen.getByText("Next Week").closest("button");
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 13, 9, 0, 0); // Monday April 13
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));

    vi.useRealTimers();
  });

  it("calls onclose when Escape is pressed", async () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    const popover = document.querySelector(".snooze-popover");
    await fireEvent.keyDown(popover, { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});
