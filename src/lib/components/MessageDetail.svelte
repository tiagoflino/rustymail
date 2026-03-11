<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    iconArchive,
    iconTrash,
    iconInbox,
    iconMail,
    iconReply,
    iconReplyAll,
    iconForward,
    iconDraft,
  } from "$lib/components/icons";
  import {
    selectedThreadId,
    currentMessages,
    isMessagesLoading,
    messagesError,
    type LocalMessage,
  } from "$lib/stores/messages";
  import { formatTime, decodeEntities } from "$lib/utils/formatters.js";

  let expandedMessages = $state(new Set<string>());
  let lastExpandedThreadId: string | null = null;
  let lastExpandedMsgIds: string | null = null;

  // Expand the last message when thread changes or messages first load for a thread
  $effect(() => {
    const tid = $selectedThreadId;
    const msgs = $currentMessages;

    if (!tid || msgs.length === 0) {
      // Thread deselected or no messages yet — reset
      if (!tid) {
        lastExpandedThreadId = null;
        lastExpandedMsgIds = null;
        expandedMessages = new Set();
      }
      return;
    }

    // Build a fingerprint of current message IDs to detect actual content changes
    const msgFingerprint = msgs.map(m => m.id).join(',');

    if (tid !== lastExpandedThreadId || lastExpandedMsgIds === null) {
      // New thread or first load — expand only the last message
      lastExpandedThreadId = tid;
      lastExpandedMsgIds = msgFingerprint;
      expandedMessages = new Set([msgs[msgs.length - 1].id]);
    } else if (msgFingerprint !== lastExpandedMsgIds) {
      // Same thread but messages changed (reply sent, draft save/delete, sync)
      const oldIds = new Set(lastExpandedMsgIds ? lastExpandedMsgIds.split(',') : []);
      lastExpandedMsgIds = msgFingerprint;
      const validIds = new Set(msgs.map(m => m.id));
      // Keep previously expanded messages that still exist
      const cleaned = new Set([...expandedMessages].filter(id => validIds.has(id)));
      // Auto-expand any NEW messages (not in the old set)
      for (const m of msgs) {
        if (!oldIds.has(m.id)) {
          cleaned.add(m.id);
        }
      }
      if (cleaned.size === 0) {
        cleaned.add(msgs[msgs.length - 1].id);
      }
      expandedMessages = cleaned;
    }
  });

  function toggleMessage(id: string) {
    const next = new Set(expandedMessages);
    if (next.has(id)) {
      if (next.size > 1) next.delete(id);
    } else {
      next.add(id);
    }
    expandedMessages = next;
  }

  function splitPlainTextQuote(text: string): { main: string; quoted: string | null } {
    // Detect "---------- Forwarded message ---------"
    const fwdIdx = text.indexOf('---------- Forwarded message');
    if (fwdIdx > 0) {
      return { main: text.slice(0, fwdIdx).trimEnd(), quoted: text.slice(fwdIdx) };
    }
    // Detect "On <date>, <name> wrote:" pattern
    const onWroteMatch = text.match(/\n(On .{10,80} wrote:\s*\n)/);
    if (onWroteMatch && onWroteMatch.index != null && onWroteMatch.index > 0) {
      return { main: text.slice(0, onWroteMatch.index).trimEnd(), quoted: text.slice(onWroteMatch.index) };
    }
    return { main: text, quoted: null };
  }

  function expandAll() {
    expandedMessages = new Set($currentMessages.map(m => m.id));
  }

  interface Props {
    isMacOS: boolean;
    isTrashView: boolean;
    onaction: (action: string) => void;
    onreply: (msg: LocalMessage) => void;
    onreplyall: (msg: LocalMessage) => void;
    onforward: (msg: LocalMessage) => void;
    oneditdraft: (msg: LocalMessage) => void;
    oniframeload: (iframe: HTMLIFrameElement) => void;
  }

  let {
    isMacOS,
    isTrashView,
    onaction,
    onreply,
    onreplyall,
    onforward,
    oneditdraft,
    oniframeload,
  }: Props = $props();
</script>

<main class="pane-view">
  <div class="titlebar-spacer" data-tauri-drag-region>
    {#if !isMacOS}
      <div class="window-controls">
        <button class="win-ctrl win-minimize" onclick={() => getCurrentWindow().minimize()} title="Minimize">
          <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
        </button>
        <button class="win-ctrl win-maximize" onclick={() => getCurrentWindow().toggleMaximize()} title="Maximize">
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1"/></svg>
        </button>
        <button class="win-ctrl win-close" onclick={() => getCurrentWindow().close()} title="Close">
          <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1.2"/></svg>
        </button>
      </div>
    {/if}
  </div>
  {#if $selectedThreadId}
    <div class="message-toolbar" data-tauri-drag-region>
      <button
        onclick={() => onaction("archive")}
        class="toolbar-btn"
        title="Archive (E)"
      >
        <span class="toolbar-icon">{@html iconArchive}</span><span
          >Archive</span
        >
      </button>
      {#if isTrashView}
        <button
          onclick={() => onaction("untrash")}
          class="toolbar-btn"
          title="Restore from Trash"
        >
          <span class="toolbar-icon">{@html iconInbox}</span><span>Restore</span>
        </button>
      {:else}
        <button
          onclick={() => onaction("trash")}
          class="toolbar-btn"
          title="Delete (#)"
        >
          <span class="toolbar-icon">{@html iconTrash}</span><span>Trash</span>
        </button>
      {/if}
      <button
        onclick={() => onaction("unread")}
        class="toolbar-btn"
        title="Mark Unread (Shift + I)"
      >
        <span class="toolbar-icon">{@html iconMail}</span><span>Unread</span
        >
      </button>
    </div>
    {#if $isMessagesLoading}
      <div class="message-scroll-area">
        {#each Array(2) as _}
          <div class="skeleton-message">
            <div class="skeleton-msg-header">
              <div class="skeleton-line w40"></div>
              <div class="skeleton-line w20"></div>
            </div>
            <div
              class="skeleton-line w60"
              style="height:18px;margin-bottom:12px"
            ></div>
            <div class="skeleton-line w100"></div>
            <div class="skeleton-line w90"></div>
            <div class="skeleton-line w70"></div>
          </div>
        {/each}
      </div>
    {:else if $messagesError}
      <div class="error-state">{$messagesError}</div>
    {:else if $currentMessages.length > 0}
      <div class="message-scroll-area">
        {#if $currentMessages.length > 2}
          <div class="expand-all-row">
            <button class="expand-all-btn" onclick={expandAll}>
              {expandedMessages.size === $currentMessages.length ? 'All expanded' : `Expand all (${$currentMessages.length} messages)`}
            </button>
          </div>
        {/if}
        {#each $currentMessages as msg, i}
          {#if expandedMessages.has(msg.id)}
            <div class="message-card animate-in">
              <div class="message-header">
                <button class="msg-collapse-btn" onclick={() => toggleMessage(msg.id)} title="Collapse">
                  <svg class="disclosure-icon expanded" width="10" height="10" viewBox="0 0 10 10"><path d="M2 3.5L5 6.5L8 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  <span class="msg-sender">{msg.sender || "Unknown Sender"}</span>
                </button>
                <div class="message-header-right">
                  <div class="msg-time">{formatTime(msg.internal_date)}</div>
                  <div class="message-actions">
                    {#if msg.is_draft}
                      <button class="msg-action-btn msg-action-draft" onclick={() => oneditdraft(msg)} data-tooltip="Edit Draft">
                        <span class="icon">{@html iconDraft}</span>
                        <span>Edit Draft</span>
                      </button>
                    {:else}
                      <button class="msg-action-btn" onclick={() => onreply(msg)} data-tooltip="Reply (R)">{@html iconReply}</button>
                      <button class="msg-action-btn" onclick={() => onreplyall(msg)} data-tooltip="Reply All">{@html iconReplyAll}</button>
                      <button class="msg-action-btn" onclick={() => onforward(msg)} data-tooltip="Forward">{@html iconForward}</button>
                    {/if}
                  </div>
                </div>
              </div>
              {#if i === 0 || (i === $currentMessages.length - 1 && expandedMessages.size === 1)}
                <h2 class="msg-subject">{msg.subject}</h2>
              {/if}
              <div class="message-body">
                {#if msg.body_html}
                  <iframe
                    title="Email Body"
                    sandbox="allow-scripts"
                    style="width:100%;height:0;border:none;overflow:hidden;background:#f5f5f5;border-radius:6px;opacity:0;transition:opacity .15s;"
                    srcdoc={`<html><head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src https: http: data: cid:;"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light only"><style>.quote-toggle{display:inline-block;cursor:pointer;padding:2px 8px;margin:4px 0;border-radius:4px;background:rgba(0,0,0,0.06);color:#666;font-size:12px;border:none;line-height:1;font-family:-apple-system,sans-serif}.quote-toggle:hover{background:rgba(0,0,0,0.1)}.quote-hidden{display:none}</style></head><body style="margin:0;padding:0;background:#f5f5f5;overflow:hidden;"><div style="max-width:680px;margin:0 auto;padding:12px;">${msg.body_html}</div><script>(function(){var b=document.body;function post(type,data){parent.postMessage(Object.assign({type:type},data),'*');}function resize(){post('rustymail-resize',{height:b.scrollHeight});}function collapseQuotes(){var quotes=b.querySelectorAll('.gmail_quote,blockquote');quotes.forEach(function(q){if(q.closest('.quote-hidden'))return;q.classList.add('quote-hidden');var btn=document.createElement('button');btn.className='quote-toggle';btn.textContent='\\u2026';btn.title='Show trimmed content';btn.addEventListener('click',function(){q.classList.toggle('quote-hidden');btn.textContent=q.classList.contains('quote-hidden')?'\\u2026':'Hide quoted text';resize();});q.parentNode.insertBefore(btn,q);});var walker=document.createTreeWalker(b,NodeFilter.SHOW_TEXT);while(walker.nextNode()){var n=walker.currentNode;if(n.textContent.indexOf('---------- Forwarded message')!==-1){var el=n.parentElement;if(!el||el.closest('.quote-hidden'))continue;var container=document.createElement('div');var rest=[];var sib=el.nextSibling;while(sib){rest.push(sib);sib=sib.nextSibling;}if(rest.length<1)continue;var wrap=document.createElement('div');wrap.className='quote-hidden';wrap.appendChild(el.cloneNode(true));rest.forEach(function(s){wrap.appendChild(s);});el.parentNode.insertBefore(wrap,el);el.remove();var btn2=document.createElement('button');btn2.className='quote-toggle';btn2.textContent='\\u2026';btn2.title='Show forwarded message';btn2.addEventListener('click',function(){wrap.classList.toggle('quote-hidden');btn2.textContent=wrap.classList.contains('quote-hidden')?'\\u2026':'Hide forwarded message';resize();});wrap.parentNode.insertBefore(btn2,wrap);break;}}}if(!${msg.is_draft})collapseQuotes();resize();new ResizeObserver(resize).observe(b);b.querySelectorAll('img').forEach(function(img){if(!img.complete)img.addEventListener('load',resize,{once:true});});document.addEventListener('click',function(e){var t=e.target;while(t&&t.tagName!=='A')t=t.parentElement;if(!t||!t.href)return;e.preventDefault();post('rustymail-link',{url:t.href});},true);})();<\/script></body></html>`}
                    onload={(e) => {
                      const iframe = e.currentTarget as HTMLIFrameElement;
                      oniframeload(iframe);
                    }}
                  ></iframe>
                {:else if msg.body_plain}
                  {@const parts = msg.is_draft ? { main: msg.body_plain, quoted: null } : splitPlainTextQuote(msg.body_plain)}
                  <pre class="plain-body">{parts.main}</pre>
                  {#if parts.quoted}
                    <button class="plain-quote-toggle" onclick={(e) => {
                      const pre = (e.currentTarget as HTMLElement).nextElementSibling;
                      if (pre) {
                        const hidden = pre.classList.toggle('plain-quote-hidden');
                        (e.currentTarget as HTMLElement).textContent = hidden ? '\u2026' : 'Hide quoted text';
                      }
                    }}>{'\u2026'}</button>
                    <pre class="plain-body plain-quoted plain-quote-hidden">{parts.quoted}</pre>
                  {/if}
                {:else}
                  <p class="no-body">Message body not available. Try refreshing.</p>
                {/if}
              </div>
            </div>
          {:else}
            <button class="collapsed-message" onclick={() => toggleMessage(msg.id)}>
              <svg class="disclosure-icon" width="10" height="10" viewBox="0 0 10 10"><path d="M3.5 2L6.5 5L3.5 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
              <span class="collapsed-sender">{msg.sender || "Unknown Sender"}</span>
              <span class="collapsed-snippet">{decodeEntities(msg.snippet || msg.subject || '')}</span>
              <span class="collapsed-time">{formatTime(msg.internal_date)}</span>
            </button>
          {/if}
        {/each}
      </div>
    {:else}
      <div class="empty-state">No messages loaded for this thread.</div>
    {/if}
  {:else}
    <div class="empty-state centered-empty">
      <div class="empty-icon">{@html iconInbox}</div>
      <p>Select a conversation to read</p>
      <span class="empty-hint">Press <kbd>/</kbd> to search</span>
    </div>
  {/if}
</main>

<style>
  .pane-view {
    display: flex;
    flex-direction: column;
    background: var(--bg-view);
    height: 100%;
  }
  .titlebar-spacer {
    height: 28px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }
  .window-controls {
    display: flex;
    align-items: center;
    height: 100%;
    -webkit-app-region: no-drag;
  }
  .win-ctrl {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.1s;
  }
  .win-ctrl:hover {
    background: var(--bg-hover, rgba(128, 128, 128, 0.2));
  }
  .win-close:hover {
    background: #e81123;
    color: #fff;
  }
  .message-toolbar {
    height: 44px;
    display: flex;
    align-items: center;
    padding: 0 16px;
    border-bottom: 1px solid var(--border-color);
    gap: 4px;
    flex-shrink: 0;
  }
  .toolbar-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    line-height: 15px;
    transition: background 0.1s;
    font-family: var(--font-family);
  }
  .toolbar-icon {
    display: flex;
    align-items: center;
    width: 16px;
    height: 16px;
  }
  .toolbar-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .msg-action-btn {
    background: transparent;
    border: none;
    border-radius: 50%;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
  }
  .msg-action-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .msg-action-btn :global(svg) {
    width: 14px;
    height: 14px;
  }
  .msg-action-btn::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: -30px;
    left: 50%;
    transform: translateX(-50%) translateY(5px);
    background: #333;
    color: #fff;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    white-space: nowrap;
    opacity: 0;
    visibility: hidden;
    transition: all 0.2s ease;
    z-index: 100;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }
  :global([data-theme="light"]) .msg-action-btn::after {
    background: #333;
    color: #fff;
  }
  :global([data-theme="dark"]) .msg-action-btn::after {
    background: #f0f0f0;
    color: #1c1c1e;
  }
  .msg-action-btn:hover::after {
    opacity: 1;
    visibility: visible;
    transform: translateX(-50%) translateY(0);
  }
  .message-scroll-area {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
  }
  .expand-all-row {
    display: flex;
    justify-content: center;
    margin-bottom: 8px;
  }
  .expand-all-btn {
    background: none;
    border: none;
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 14px;
    cursor: pointer;
    padding: 4px 12px;
    border-radius: 4px;
    font-family: var(--font-family);
    transition: background 0.1s;
  }
  .expand-all-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .collapsed-message {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 16px;
    margin-bottom: 4px;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: transparent;
    cursor: pointer;
    font-family: var(--font-family);
    color: var(--text-primary);
    text-align: left;
    transition: background 0.1s;
  }
  .collapsed-message:hover {
    background: var(--sidebar-hover);
  }
  .collapsed-sender {
    font-size: 13px;
    line-height: 16px;
    font-weight: 500;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .collapsed-snippet {
    font-size: 12px;
    line-height: 15px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .collapsed-time {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .disclosure-icon {
    color: var(--text-secondary);
    flex-shrink: 0;
    transition: transform 0.15s ease;
  }
  .msg-collapse-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: var(--font-family);
  }
  .msg-collapse-btn .msg-sender {
    font-weight: 600;
    color: var(--text-primary);
    font-size: 14px;
    line-height: 18px;
    letter-spacing: -0.08px;
  }
  .error-state {
    padding: 2rem;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: #ff3b30;
  }
  .message-card {
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 20px;
    margin-bottom: 16px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  }
  .animate-in {
    animation: fadeSlideIn 0.25s ease-out;
  }
  @keyframes fadeSlideIn {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .message-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
  }
  .message-header-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .message-actions {
    display: flex;
    gap: 2px;
  }
  .msg-action-draft {
    width: auto;
    padding: 0 12px;
    font-size: 13px;
    font-weight: 500;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .msg-time {
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 15px;
    flex-shrink: 0;
  }
  .msg-subject {
    font-size: 17px;
    line-height: 22px;
    font-weight: 600;
    margin: 0 0 14px 0;
    letter-spacing: -0.1px;
    color: var(--text-primary);
  }
  .message-body {
    font-size: 14px;
    line-height: 1.6;
    color: var(--text-primary);
    overflow-x: hidden;
  }
  .plain-body {
    white-space: pre-wrap;
    font-family: inherit;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    margin: 0;
    background: var(--bg-view);
    color: var(--text-primary);
    padding: 12px;
    border-radius: 6px;
  }
  .plain-quote-toggle {
    display: inline-block;
    cursor: pointer;
    padding: 2px 8px;
    margin: 4px 0 4px 12px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.06);
    color: var(--text-secondary);
    font-size: 12px;
    border: none;
    line-height: 1;
    font-family: var(--font-family);
    transition: background 0.1s;
  }
  .plain-quote-toggle:hover {
    background: rgba(0, 0, 0, 0.1);
  }
  .plain-quoted {
    opacity: 0.7;
    border-left: 2px solid var(--border-color);
  }
  .plain-quote-hidden {
    display: none;
  }
  .no-body {
    color: var(--text-secondary);
    font-style: italic;
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
  }
  .empty-state {
    padding: 2rem;
    text-align: center;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 16px;
    letter-spacing: -0.08px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .centered-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    opacity: 0.5;
  }
  .empty-icon {
    width: 48px;
    height: 48px;
    color: var(--text-secondary);
    opacity: 0.25;
    margin-bottom: 8px;
  }
  .empty-icon :global(svg) {
    width: 48px;
    height: 48px;
  }
  .empty-hint {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-secondary);
    opacity: 0.5;
  }
  .empty-hint kbd {
    background: var(--sidebar-hover);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 11px;
    font-family: monospace;
    border: 1px solid var(--border-color);
  }
  .skeleton-message {
    padding: 20px;
    margin: 16px 20px;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .skeleton-msg-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 4px;
    gap: 20px;
  }
  .skeleton-line {
    height: 10px;
    border-radius: 4px;
    background: var(--border-color);
    animation: shimmer 1.5s infinite;
  }
  .skeleton-line.w20 {
    width: 20%;
  }
  .skeleton-line.w40 {
    width: 40%;
  }
  .skeleton-line.w60 {
    width: 60%;
  }
  .skeleton-line.w70 {
    width: 70%;
  }
  .skeleton-line.w90 {
    width: 90%;
  }
  .skeleton-line.w100 {
    width: 100%;
  }
  @keyframes shimmer {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 0.8;
    }
  }
</style>
