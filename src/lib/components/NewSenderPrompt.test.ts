import { render, screen, fireEvent } from "@testing-library/svelte";
import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import NewSenderPrompt from "./NewSenderPrompt.svelte";

describe("NewSenderPrompt.svelte", () => {
  it("shows the sender name when provided, falling back to email", () => {
    render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      senderName: "Alice Smith",
      onroute: vi.fn(),
      onclose: vi.fn(),
    });
    expect(screen.getByText("Alice Smith")).toBeInTheDocument();
  });

  it("falls back to the email when no name is given", () => {
    render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      onroute: vi.fn(),
      onclose: vi.fn(),
    });
    expect(screen.getByText("alice@test.com")).toBeInTheDocument();
  });

  it("renders all four routing options", () => {
    render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      onroute: vi.fn(),
      onclose: vi.fn(),
    });
    expect(screen.getByText("Inbox")).toBeInTheDocument();
    expect(screen.getByText("Feed")).toBeInTheDocument();
    expect(screen.getByText("Auto-archive")).toBeInTheDocument();
    expect(screen.getByText("Block")).toBeInTheDocument();
  });

  it("renders icons as inline svg, not via @html", () => {
    const { container } = render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      onroute: vi.fn(),
      onclose: vi.fn(),
    });
    expect(container.querySelectorAll("svg").length).toBe(4);
  });

  it("calls onroute with the chosen routing id", async () => {
    const onroute = vi.fn();
    render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      onroute,
      onclose: vi.fn(),
    });
    await fireEvent.click(screen.getByText("Block"));
    expect(onroute).toHaveBeenCalledWith("blocked");
  });

  it("calls onclose from the Decide later button", async () => {
    const onclose = vi.fn();
    render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      onroute: vi.fn(),
      onclose,
    });
    await fireEvent.click(screen.getByText(/Decide later/));
    expect(onclose).toHaveBeenCalled();
  });

  it("calls onclose on Escape", async () => {
    const onclose = vi.fn();
    const { container } = render(NewSenderPrompt, {
      senderEmail: "alice@test.com",
      onroute: vi.fn(),
      onclose,
    });
    const backdrop = container.querySelector(".prompt-backdrop")!;
    await fireEvent.keyDown(backdrop, { key: "Escape" });
    expect(onclose).toHaveBeenCalled();
  });
});
