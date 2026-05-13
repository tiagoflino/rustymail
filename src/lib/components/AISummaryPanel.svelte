<script lang="ts">
  import { addToast } from "$lib/stores/toast";
  import { sanitizeHtml } from "$lib/utils/htmlSanitizer";

  let {
    isOpen = $bindable(false),
    summary = $bindable(null),
    isLoading = $bindable(false),
    statusMessage = $bindable(null),
    onClose = $bindable(() => {}),
    onCopy = $bindable(() => {}),
  } = $props();

  // Parse summary into sections using derived state
  interface ParsedSections {
    overview: string;
    keyDetails: string[];
    actionItems: string[];
  }

  function parseSummary(raw: string): ParsedSections {
    let overview = "";
    let keyDetails: string[] = [];
    let actionItems: string[] = [];

    const lines = raw.split("\n");
    let currentSection: string | null = null;
    let overviewLines: string[] = [];

    for (const line of lines) {
      const trimmed = line.trim();

      // Check for section headers
      if (trimmed.startsWith("**Overview**")) {
        currentSection = "overview";
        const content = trimmed.replace("**Overview**", "").trim();
        if (content) {
          overviewLines.push(content);
        }
        continue;
      }

      if (trimmed.startsWith("**Key Details**")) {
        // Save overview if exists
        if (overviewLines.length > 0) {
          overview = overviewLines.join(" ");
        }
        currentSection = "keyDetails";
        continue;
      }

      if (trimmed.startsWith("**Action Items**")) {
        // Save key details if exists
        if (keyDetails.length > 0) {
          keyDetails = keyDetails.filter(d => d.trim().length > 0);
        }
        currentSection = "actionItems";
        continue;
      }

      // Parse content based on current section
      if (currentSection === "overview" && trimmed) {
        if (trimmed.startsWith("**") && trimmed.endsWith("**")) {
          // New section header without ** prefix detected
          continue;
        }
        overviewLines.push(trimmed);
      } else if (currentSection === "keyDetails" && trimmed) {
        const content = trimmed.replace(/^[-•*]\s*/, "");
        if (content) {
          keyDetails.push(content);
        }
      } else if (currentSection === "actionItems" && trimmed) {
        const content = trimmed.replace(/^[-•*☐]\s*/, "");
        if (content) {
          actionItems.push(content);
        }
      }
    }

    // Save last section
    if (currentSection === "overview" && overviewLines.length > 0) {
      overview = overviewLines.join(" ");
    } else if (currentSection === "keyDetails" && keyDetails.length > 0) {
      keyDetails = keyDetails.filter(d => d.trim().length > 0);
    }

    return { overview, keyDetails, actionItems };
  }

  // Use $derived.by for computed parsed sections - avoids reactive loop
  let parsedSections = $derived.by(() => {
    if (!summary) return { overview: "", keyDetails: [] as string[], actionItems: [] as string[] };
    return parseSummary(summary);
  });

  // Alias for template compatibility
  let overview = $derived(parsedSections.overview);
  let keyDetails = $derived(parsedSections.keyDetails);
  let actionItems = $derived(parsedSections.actionItems);

  function formatSummary(raw: string): string {
    let text = raw
      .replace(/\*\*([^*]+)\*\*\s*\n\s*(?=\()/g, '**$1** ')
      .replace(/:\n\s*(?=[A-Za-z'"])/g, ': ');

    function fmt(s: string): string {
      return s
        .replace(/&/g, '&').replace(/</g, '<').replace(/>/g, '>')
        .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
        .replace(/__(.+?)__/g, '<strong>$1</strong>')
        .replace(/(?<!\w)\*([^*]+?)\*(?!\w)/g, '<em>$1</em>')
        .replace(/`([^`]+?)`/g, '<code>$1</code>');
    }

    type Entry = { type: 'header' | 'topic' | 'sub' | 'text'; content: string; raw: string };
    const entries: Entry[] = [];

    for (const rawLine of text.split('\n')) {
      const trimmed = rawLine.trim();
      if (!trimmed || trimmed === '-' || trimmed === '•' || trimmed === '*') continue;

      const isBullet = /^(?:[-*•]|\d+[.)]) /.test(trimmed);
      if (!isBullet) {
        entries.push({ type: /\*\*/.test(trimmed) ? 'header' : 'text', content: trimmed, raw: rawLine });
      } else {
        const content = trimmed.replace(/^(?:[-*•]|\d+[.)]) /, '');
        if (!content.trim()) continue;
        const hasBold = /\*\*/.test(content);
        const indent = (rawLine.match(/^(\s*)/)?.[1].length || 0);
        if (hasBold && indent < 2) {
          entries.push({ type: 'topic', content, raw: rawLine });
        } else {
          entries.push({ type: 'sub', content, raw: rawLine });
        }
      }
    }

    const out: string[] = [];
    let nestDepth = 0;

    for (let i = 0; i < entries.length; i++) {
      const e = entries[i];

      if (e.type === 'header' || e.type === 'text') {
        while (nestDepth > 0) { out.push('</div>'); nestDepth--; }
        out.push(`<div class="ai-line">${fmt(e.content)}</div>`);
        continue;
      }

      if (e.type === 'topic') {
        while (nestDepth > 0) { out.push('</div>'); nestDepth--; }
        out.push(`<div class="ai-topic">• ${fmt(e.content)}</div>`);
        if (i + 1 < entries.length && entries[i + 1].type === 'sub') {
          out.push('<div class="ai-nest">');
          nestDepth = 1;
        }
        continue;
      }

      if (e.type === 'sub') {
        const hasBold = /\*\*/.test(e.content);
        const nextIsSub = i + 1 < entries.length && entries[i + 1].type === 'sub';
        const nextHasBold = nextIsSub && /\*\*/.test(entries[i + 1].content);

        if (hasBold) {
          out.push(`<div class="ai-topic">• ${fmt(e.content)}</div>`);
          if (nextIsSub && !nextHasBold) {
            out.push('<div class="ai-nest">');
            nestDepth++;
          }
        } else {
          out.push(`<div class="ai-subitem">• ${fmt(e.content)}</div>`);
          if (nextIsSub && /\*\*/.test(entries[i + 1].content)) {
            if (nestDepth > 1) { out.push('</div>'); nestDepth--; }
          }
        }
        continue;
      }
    }

    while (nestDepth > 0) { out.push('</div>'); nestDepth--; }
    return out.join('');
  }

  function handleCopy(event: MouseEvent): void {
    event.stopPropagation();
    if (summary) {
      navigator.clipboard.writeText(summary).then(() => {
        addToast("Summary copied to clipboard", "success", 2000);
      }).catch(() => {
        addToast("Failed to copy summary", "error", 2000);
      });
    }
    onCopy();
  }

  function handleClose(event: MouseEvent): void {
    event.stopPropagation();
    onClose();
  }
</script>

<div class="ai-summary-panel" class:open={isOpen} role="complementary" aria-label="AI Summary panel">
  <!-- Header -->
  <div class="panel-header">
    <span class="panel-title">AI Summary</span>
    <div class="panel-actions">
      {#if summary && !isLoading}
        <button
          class="action-btn copy-btn"
          onclick={handleCopy}
          title="Copy summary"
          aria-label="Copy summary"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2"/>
            <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
          </svg>
        </button>
      {/if}
      <button
        class="action-btn close-btn"
        onclick={handleClose}
        title="Close AI Summary"
        aria-label="Close AI Summary"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18"/>
          <line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- Content -->
  <div class="panel-content">
    {#if isLoading}
      <!-- Loading skeleton -->
      <div class="skeleton-shimmer">
        <div class="skeleton-line w80"></div>
        <div class="skeleton-line w60"></div>
        <div class="skeleton-line w70"></div>
        <div class="skeleton-line w90"></div>
        <div class="skeleton-line w50"></div>
      </div>
      {#if statusMessage}
        <p class="loading-status">{statusMessage}</p>
      {/if}
    {:else if summary}
      <!-- Summary content -->
      <div class="summary-content">
        {#if overview}
          <section class="summary-section">
            <h3>Overview</h3>
            <p>{overview}</p>
          </section>
        {/if}

        {#if keyDetails.length > 0}
          <section class="summary-section">
            <h3>Key Details</h3>
            <ul>
              {#each keyDetails as detail}
                <li>{detail}</li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if actionItems.length > 0}
          <section class="summary-section action-section">
            <h3>Action Items</h3>
            <ul class="action-items">
              {#each actionItems as item}
                <li><span class="checkbox">☐</span>{item}</li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if !overview && keyDetails.length === 0 && actionItems.length === 0}
          <!-- Fallback: render sanitized formatted text -->
          <div class="summary-fallback">{@html sanitizeHtml(formatSummary(summary))}</div>
        {/if}
      </div>
    {:else}
      <!-- Empty state -->
      <div class="empty-state">
        <svg class="empty-icon" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 2a4 4 0 014 4c0 1.1-.4 2.1-1 2.8L12 12l-3-3.2A4 4 0 0112 2z"/>
          <path d="M8 14s1.5 2 4 2 4-2 4-2"/>
          <path d="M9 18h6"/>
          <path d="M10 22h4"/>
        </svg>
        <p>No summary available</p>
        <span class="empty-hint">Click "Summarize" in the toolbar to generate an AI summary</span>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Panel container - fixed overlay, aligned with message-scroll-area */
  .ai-summary-panel {
    position: fixed;
    top: 72px;
    right: 0;
    width: 320px;
    min-width: 320px;
    height: calc(100vh - 72px);
    background: var(--bg-view);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-left: 1px solid var(--border-color);
    transform: translateX(100%);
    transition: transform 200ms cubic-bezier(0.25, 0.1, 0.25, 1);
    display: flex;
    flex-direction: column;
    z-index: 1000;
    overflow: hidden;
  }

  .ai-summary-panel.open {
    transform: translateX(0);
  }

  /* Header */
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    height: 36px;
    flex-shrink: 0;
    border-bottom: 0.5px solid var(--border-color);
    background: var(--bg-sidebar);
  }

  .panel-title {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    letter-spacing: normal;
  }

  .panel-actions {
    display: flex;
    gap: 2px;
  }

  .action-btn {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.1s;
  }

  .action-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  /* Content area - matches message detail padding */
  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-section);
    scrollbar-width: thin;
  }

  /* Summary sections */
  .summary-section {
    margin-bottom: 16px;
  }

  .summary-section:last-child {
    margin-bottom: 0;
  }

  .summary-section h3 {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    text-transform: none;
    letter-spacing: normal;
    margin-bottom: 8px;
  }

  .summary-section p {
    font-size: var(--font-size-base);
    line-height: 1.5;
    color: var(--text-primary);
    margin: 0;
  }

  .summary-section ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .summary-section li {
    font-size: var(--font-size-base);
    line-height: 1.5;
    color: var(--text-primary);
    padding: 4px 0 4px 20px;
    margin-bottom: 6px;
    position: relative;
  }

  .summary-section li::before {
    content: "•";
    position: absolute;
    left: 4px;
    color: var(--accent-blue);
    font-weight: 700;
  }

  /* Action items with checkboxes */
  .action-section li::before {
    content: none;
  }

  .action-items {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .action-items li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding-left: 0;
    color: var(--text-primary);
  }

  .action-items li .checkbox {
    color: var(--accent-blue);
    font-size: 14px;
    line-height: 1.5;
    flex-shrink: 0;
    margin-top: 1px;
  }

  /* Skeleton shimmer */
  .skeleton-shimmer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .skeleton-line {
    height: 12px;
    border-radius: 4px;
    background: var(--border-color);
    animation: shimmer 1.5s infinite;
  }

  .skeleton-line.w50 { width: 50%; }
  .skeleton-line.w60 { width: 60%; }
  .skeleton-line.w70 { width: 70%; }
  .skeleton-line.w80 { width: 80%; }
  .skeleton-line.w90 { width: 90%; }

  @keyframes shimmer {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.8; }
  }

  .loading-status {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    text-align: center;
    padding: var(--spacing-base);
    font-style: italic;
  }

  /* Empty state */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    text-align: center;
    gap: var(--spacing-base);
  }

  .empty-icon {
    color: var(--text-secondary);
    opacity: 0.3;
    margin-bottom: var(--spacing-base);
  }

  .empty-state p {
    font-size: var(--font-size-base);
    font-weight: 500;
    color: var(--text-primary);
  }

  .empty-hint {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    line-height: 1.4;
    max-width: 220px;
  }

  /* Fallback for unstructured summary */
  .summary-fallback {
    font-size: var(--font-size-base);
    line-height: 1.6;
    color: var(--text-primary);
  }

  .summary-fallback :global(strong) {
    font-weight: 600;
    color: var(--text-primary);
  }

  .summary-fallback :global(em) {
    font-style: italic;
    color: var(--text-secondary);
  }

  .summary-fallback :global(code) {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.9em;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--sidebar-hover);
  }

  .summary-fallback :global(ul),
  .summary-fallback :global(ol) {
    padding-left: 20px;
    margin: var(--spacing-base) 0;
  }

  .summary-fallback :global(li) {
    margin-bottom: 4px;
  }
</style>
