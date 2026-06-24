import { describe, it, expect } from "vitest";
import {
  extractEmail,
  normalizeEmail,
  collectSenderCandidates,
  firstNewSender,
} from "./email";

describe("extractEmail", () => {
  it("extracts a bare address", () => {
    expect(extractEmail("bob@acme.com")).toBe("bob@acme.com");
  });

  it("extracts from a Name <addr> header", () => {
    expect(extractEmail("Alice Smith <alice@test.com>")).toBe("alice@test.com");
  });

  it("returns null for missing/invalid input", () => {
    expect(extractEmail(null)).toBeNull();
    expect(extractEmail(undefined)).toBeNull();
    expect(extractEmail("no address here")).toBeNull();
  });
});

describe("normalizeEmail", () => {
  it("lowercases the address", () => {
    expect(normalizeEmail("Bob@Acme.COM")).toBe("bob@acme.com");
  });

  it("extracts and lowercases from a header", () => {
    expect(normalizeEmail("Alice <Alice@Test.com>")).toBe("alice@test.com");
  });

  it("returns empty string when no address is present", () => {
    expect(normalizeEmail("")).toBe("");
    expect(normalizeEmail(null)).toBe("");
    expect(normalizeEmail("garbage")).toBe("");
  });

  it("matches the backend normalization contract (lowercase whole address)", () => {
    // Mirrors src-tauri/src/sender_routing.rs::normalize_email
    expect(normalizeEmail("USER+tag@Example.org")).toBe("user+tag@example.org");
  });
});

describe("collectSenderCandidates", () => {
  it("normalizes, dedups, and caps at the limit", () => {
    const threads = [
      { sender: "Alice <Alice@test.com>" },
      { sender: "alice@test.com" }, // dup after normalization
      { sender: "Bob <bob@test.com>" },
      { sender: "no-address" }, // skipped
      { sender: "carol@test.com" },
      { sender: "dave@test.com" }, // beyond limit of 5 visible rows
    ];
    const result = collectSenderCandidates(threads, 5);
    expect(result.map((c) => c.email)).toEqual([
      "alice@test.com",
      "bob@test.com",
      "carol@test.com",
    ]);
  });

  it("returns empty when no senders parse", () => {
    expect(collectSenderCandidates([{ sender: "junk" }, { sender: null }])).toEqual([]);
  });
});

describe("firstNewSender", () => {
  const candidates = [
    { email: "a@x.com", name: "A" },
    { email: "b@x.com", name: "B" },
    { email: "c@x.com", name: "C" },
  ];

  it("returns the first candidate flagged new (one at a time)", () => {
    const results = [
      { sender_email: "a@x.com", is_new: false },
      { sender_email: "b@x.com", is_new: true },
      { sender_email: "c@x.com", is_new: true },
    ];
    expect(firstNewSender(candidates, results)?.email).toBe("b@x.com");
  });

  it("returns null when none are new", () => {
    const results = candidates.map((c) => ({ sender_email: c.email, is_new: false }));
    expect(firstNewSender(candidates, results)).toBeNull();
  });
});
