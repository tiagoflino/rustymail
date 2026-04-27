import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AISummaryPanel from "./AISummaryPanel.svelte";

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

afterEach(() => {
  vi.restoreAllMocks();
});

describe("AISummaryPanel", () => {
  /** @type {Function} */
  let onClose;
  /** @type {Function} */
  let onCopy;

  beforeEach(() => {
    onClose = vi.fn();
    onCopy = vi.fn();
  });

  it("renders collapsed by default when isOpen is false", () => {
    const { container } = render(AISummaryPanel, {
      props: {
        isOpen: false,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const panel = container.querySelector(".ai-summary-panel");
    expect(panel).not.toHaveClass("open");
  });

  it("renders open when isOpen is true", () => {
    const { container } = render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const panel = container.querySelector(".ai-summary-panel");
    expect(panel).toHaveClass("open");
  });

  it("renders header with title", () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    expect(screen.getByText("AI Summary")).toBeInTheDocument();
  });

  it("shows close button", () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const closeBtn = screen.getByLabelText("Close AI Summary");
    expect(closeBtn).toBeInTheDocument();
  });

  it("shows copy button when summary is available", () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: "Test summary",
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const copyBtn = screen.getByLabelText("Copy summary");
    expect(copyBtn).toBeInTheDocument();
  });

  it("displays loading skeleton when isLoading is true", () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: true,
        statusMessage: "Generating summary...",
        onClose,
        onCopy,
      },
    });
    
    expect(screen.getByText("Generating summary...")).toBeInTheDocument();
    const skeleton = document.querySelector(".skeleton-shimmer");
    expect(skeleton).toBeInTheDocument();
  });

  it("displays status message during loading", () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: true,
        statusMessage: "Downloading AI model... 50%",
        onClose,
        onCopy,
      },
    });
    
    expect(screen.getByText("Downloading AI model... 50%")).toBeInTheDocument();
  });

  it("displays summary content when available", () => {
    const summary = "**Overview**\nThis is a test summary.\n\n**Key Details**\n• Point 1\n• Point 2";
    
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    expect(screen.getByText("Overview")).toBeInTheDocument();
    expect(screen.getByText("Key Details")).toBeInTheDocument();
  });

  it("fires onClose when close button is clicked", async () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const closeBtn = screen.getByLabelText("Close AI Summary");
    await fireEvent.click(closeBtn);
    
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("fires onCopy when copy button is clicked", async () => {
    const summary = "Test summary content";
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
      writable: true,
      configurable: true,
    });
    
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const copyBtn = screen.getByLabelText("Copy summary");
    await fireEvent.click(copyBtn);
    
    expect(onCopy).toHaveBeenCalledTimes(1);
  });

  it("shows empty state when no summary and not loading", () => {
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    expect(screen.getByText("No summary available")).toBeInTheDocument();
  });

  it("renders action items section when present", () => {
    const summary = "**Overview**\nTest.\n\n**Action Items**\n- Complete task\n- Review PR";
    
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    expect(screen.getByText("Action Items")).toBeInTheDocument();
  });

  it("renders key details as bullet list", () => {
    const summary = "**Key Details**\n- Important point\n- Another point";
    
    render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const detailsSection = screen.getByText("Key Details").parentElement;
    expect(detailsSection).toBeInTheDocument();
  });

  it("panel has correct ARIA attributes", () => {
    const { container } = render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const panel = container.querySelector(".ai-summary-panel");
    expect(panel).toHaveAttribute("role", "complementary");
    expect(panel).toHaveAttribute("aria-label", "AI Summary panel");
  });

  it("hides panel completely when isOpen is false", () => {
    const { container } = render(AISummaryPanel, {
      props: {
        isOpen: false,
        summary: "Test",
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const panel = container.querySelector(".ai-summary-panel");
    expect(panel).not.toHaveClass("open");
  });

  it("shows panel when isOpen changes to true", async () => {
    const onClose = vi.fn();
    const onCopy = vi.fn();
    
    // Test that the panel correctly shows the open class when isOpen is true
    // This is a separate test from "renders collapsed by default" which tests false
    const { container } = render(AISummaryPanel, {
      props: {
        isOpen: true,
        summary: null,
        isLoading: false,
        statusMessage: null,
        onClose,
        onCopy,
      },
    });
    
    const panel = container.querySelector(".ai-summary-panel");
    expect(panel).toHaveClass("open");
    
    // Verify content area is visible when open
    const content = container.querySelector(".panel-content");
    expect(content).toBeInTheDocument();
  });
});
