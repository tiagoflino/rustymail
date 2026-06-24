const EMAIL_RE = /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/;

export function extractEmail(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const match = raw.match(EMAIL_RE);
  return match ? match[0] : null;
}

export function normalizeEmail(raw: string | null | undefined): string {
  const email = extractEmail(raw);
  return email ? email.toLowerCase() : "";
}

export interface SenderCandidate {
  email: string;
  name: string;
}

/// Build a de-duplicated list of normalized sender candidates from the most
/// recent threads, capped at `limit`. Shared by the new-sender check on sync.
export function collectSenderCandidates(
  threads: { sender?: string | null }[],
  limit = 5,
): SenderCandidate[] {
  const seen = new Set<string>();
  const candidates: SenderCandidate[] = [];
  for (const t of threads.slice(0, limit)) {
    const email = normalizeEmail(t.sender);
    if (!email || seen.has(email)) continue;
    seen.add(email);
    candidates.push({ email, name: t.sender ?? email });
  }
  return candidates;
}

/// Pick the first candidate that the backend flagged as new — "show one at a time".
export function firstNewSender(
  candidates: SenderCandidate[],
  results: { sender_email: string; is_new: boolean }[],
): SenderCandidate | null {
  const newSet = new Set(results.filter((r) => r.is_new).map((r) => r.sender_email));
  return candidates.find((c) => newSet.has(c.email)) ?? null;
}
