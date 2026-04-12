import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SnoozePopover from "./SnoozePopover.svelte";

// Mock Web Animations API for Svelte transitions
beforeEach(() => {
  Element.prototype.animate = vi.fn().mockReturnValue({
    finished: Promise.resolve(),
    cancel: vi.fn(),
    onfinish: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
});

describe("SnoozePopover", () => {
  /** @type {any} */
  let onsnooze;
  /** @type {any} */
  let onclose;

  beforeEach(() => {
    onsnooze = vi.fn();
    onclose = vi.fn();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders 3 snooze options with menu role", () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    expect(screen.getByRole("menu")).toBeTruthy();
    const items = screen.getAllByRole("menuitem");
    expect(items.length).toBe(3);
    expect(screen.getByText("Later Today")).toBeTruthy();
    expect(screen.getByText("Tomorrow Morning")).toBeTruthy();
    expect(screen.getByText("Next Week")).toBeTruthy();
  });

  it("renders header text", () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    expect(screen.getByText("Snooze until...")).toBeTruthy();
  });

  it("fires onsnooze with future timestamp on click", async () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (/** @type {HTMLButtonElement} */ (screen.getByText("Later Today").closest("button")));
    await fireEvent.click(btn);
    expect(onsnooze).toHaveBeenCalledTimes(1);
    const ts = onsnooze.mock.calls[0][0];
    expect(typeof ts).toBe("number");
    const now = Math.floor(Date.now() / 1000);
    expect(ts).toBeGreaterThan(now);
  });

  it("does not fire onsnooze on Escape", async () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    const menu = screen.getByRole("menu");
    await fireEvent.keyDown(menu, { key: "Escape" });
    expect(onclose).toHaveBeenCalledTimes(1);
    expect(onsnooze).not.toHaveBeenCalled();
  });

  it("fires onclose on backdrop click", async () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    const backdrop = /** @type {Element} */ (document.querySelector(".snooze-backdrop"));
    await fireEvent.click(backdrop);
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("Later Today computes +3h when before 6 PM", async () => {
    const mockDate = new Date(2026, 3, 8, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Later Today").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expected = Math.floor((mockDate.getTime() + 3 * 60 * 60 * 1000) / 1000);
    expect(ts).toBe(expected);
  });

  it("Later Today rolls to tomorrow 9 AM when after 6 PM", async () => {
    const mockDate = new Date(2026, 3, 8, 19, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Later Today").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 9, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));
  });

  it("Later Today rolls to tomorrow 9 AM at exactly 6 PM", async () => {
    const mockDate = new Date(2026, 3, 8, 18, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Later Today").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 9, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));
  });

  it("Tomorrow Morning computes next day 9 AM", async () => {
    const mockDate = new Date(2026, 3, 8, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Tomorrow Morning").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 9, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));
  });

  it("Next Week computes next Monday 9 AM from Wednesday", async () => {
    const mockDate = new Date(2026, 3, 8, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Next Week").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 13, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));
  });

  it("Next Week computes next Monday 9 AM from Monday", async () => {
    const mockDate = new Date(2026, 3, 6, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Next Week").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 13, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));
  });

  it("Next Week computes next Monday 9 AM from Sunday", async () => {
    const mockDate = new Date(2026, 3, 5, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const btn = /** @type {HTMLButtonElement} */ (screen.getByText("Next Week").closest("button"));
    await fireEvent.click(btn);

    const ts = onsnooze.mock.calls[0][0];
    const expectedDate = new Date(2026, 3, 6, 9, 0, 0);
    expect(ts).toBe(Math.floor(expectedDate.getTime() / 1000));
  });

  it("supports arrow key navigation", async () => {
    render(SnoozePopover, { props: { onsnooze, onclose } });
    const menu = screen.getByRole("menu");
    const items = screen.getAllByRole("menuitem");

    await fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[1]);

    await fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[2]);

    await fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[0]);

    await fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(document.activeElement).toBe(items[2]);
  });

  it("renders preview date text for each option", () => {
    const mockDate = new Date(2026, 3, 8, 14, 0, 0);
    vi.useFakeTimers();
    vi.setSystemTime(mockDate);

    render(SnoozePopover, { props: { onsnooze, onclose } });
    const previews = document.querySelectorAll(".snooze-preview");
    expect(previews.length).toBe(3);
    previews.forEach(p => {
      expect(p.textContent?.trim().length).toBeGreaterThan(0);
    });
  });
});
