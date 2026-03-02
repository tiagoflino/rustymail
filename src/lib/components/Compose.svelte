<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { iconClose, iconTrash, iconSent, iconCheck } from './icons';
  import { fly } from 'svelte/transition';
  import { addToast } from '$lib/stores/toast';

  let { 
    onClose, 
    initialTo = '', 
    initialCc = '', 
    initialSubject = '', 
    initialBodyHTML = '', 
    threadId = null, 
    inReplyTo = null, 
    references = null 
  }: { 
    onClose: () => void, 
    initialTo?: string, 
    initialCc?: string, 
    initialSubject?: string, 
    initialBodyHTML?: string, 
    threadId?: string | null, 
    inReplyTo?: string | null, 
    references?: string | null 
  } = $props();

  let isMinimized = $state(false);
  let isExpanded = $state(false);
  let isSending = $state(false);
  
  let to = $state(initialTo);
  let cc = $state(initialCc);
  let bcc = $state('');
  let subject = $state(initialSubject);
  let bodyHTML = $state(initialBodyHTML);

  let showCc = $state(initialCc.length > 0);
  let showBcc = $state(false);

  let suggestions = $state<any[]>([]);
  let suggestionIndex = $state(0);
  let activeField = $state<'to' | 'cc' | 'bcc' | null>(null);
  let suggestionDebounce: any;

  async function handleInput(field: 'to' | 'cc' | 'bcc', val: string) {
    activeField = field;
    if (suggestionDebounce) clearTimeout(suggestionDebounce);
    
    const lastPart = val.split(',').pop()?.trim() || '';
    if (lastPart.length < 2) {
      suggestions = [];
      return;
    }

    suggestionDebounce = setTimeout(async () => {
      try {
        suggestions = await invoke('search_contacts', { query: lastPart });
        suggestionIndex = 0;
      } catch (e) {
        suggestions = [];
      }
    }, 200);
  }

  function selectSuggestion(suggestion: any) {
    if (!activeField) return;
    
    let currentVal = activeField === 'to' ? to : activeField === 'cc' ? cc : bcc;
    const parts = currentVal.split(',').map(p => p.trim());
    parts.pop(); // remove the partial
    
    const formatted = suggestion.name ? `${suggestion.name} <${suggestion.email}>` : suggestion.email;
    parts.push(formatted);
    
    const newVal = parts.join(', ') + ', ';
    if (activeField === 'to') to = newVal;
    else if (activeField === 'cc') cc = newVal;
    else bcc = newVal;

    suggestions = [];
  }

  function handleKeydown(e: KeyboardEvent) {
    if (suggestions.length === 0) return;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      suggestionIndex = (suggestionIndex + 1) % suggestions.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      suggestionIndex = (suggestionIndex - 1 + suggestions.length) % suggestions.length;
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      selectSuggestion(suggestions[suggestionIndex]);
    } else if (e.key === 'Escape') {
      suggestions = [];
    }
  }

  let editorEl: HTMLDivElement;

  function format(command: string, value: string | undefined = undefined) {
    document.execCommand(command, false, value);
    editorEl.focus();
  }

  onMount(async () => {
    try {
      const sig = await invoke('get_setting', { key: 'signature' }) as string;
      let newHtml = initialBodyHTML;
      if (sig) {
        newHtml = `<br><br><div class="rustymail-signature" style="color: var(--text-secondary); opacity: 0.8; font-size: 13px;">${sig}</div>` + (initialBodyHTML ? `<br>${initialBodyHTML}` : '');
      }
      if (newHtml) {
        editorEl.innerHTML = newHtml;
      }
      
      const range = document.createRange();
      const sel = window.getSelection();
      range.setStart(editorEl, 0);
      range.collapse(true);
      sel?.removeAllRanges();
      sel?.addRange(range);
    } catch (e) {
      if (initialBodyHTML) {
        editorEl.innerHTML = initialBodyHTML;
      }
    }
  });

  async function send() {
    if (!to) { addToast("Please specify at least one recipient.", "info"); return; }
    isSending = true;
    try {
      await invoke('send_message', { 
        to: `${to}${cc ? ',' + cc : ''}${bcc ? ',' + bcc : ''}`, 
        subject, 
        body: editorEl.innerHTML,
        threadId,
        inReplyTo,
        references
      });
      addToast("Message sent successfully.", "success", 5000);
      onClose();
    } catch (e) {
      addToast(`Failed to send: ${e}`, "error", 7000);
    } finally {
      isSending = false;
    }
  }

  async function saveDraft() {
    try {
      await invoke('save_draft', { 
        to: `${to}${cc ? ',' + cc : ''}${bcc ? ',' + bcc : ''}`, 
        subject, 
        body: editorEl.innerHTML,
        threadId,
        inReplyTo,
        references
      });
      addToast("Draft saved.", "success");
      onClose();
    } catch (e) {
      addToast(`Failed to save draft: ${e}`, "error", 7000);
    }
  }

  const iconMinimize = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="5" y1="12" x2="19" y2="12"/></svg>`;
  const iconMaximize = `<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg>`;
  const iconBold = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"></path><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"></path></svg>`;
  const iconItalic = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="19" y1="4" x2="10" y2="4"></line><line x1="14" y1="20" x2="5" y2="20"></line><line x1="15" y1="4" x2="9" y2="20"></line></svg>`;
  const iconUnderline = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 3v7a6 6 0 0 0 6 6 6 6 0 0 0 6-6V3"></path><line x1="4" y1="21" x2="20" y2="21"></line></svg>`;

</script>

<div 
  class="compose-window" 
  class:minimized={isMinimized} 
  class:expanded={isExpanded}
  transition:fly={{ y: 20, duration: 250 }}
>
  <header class="compose-header" onclick={() => isMinimized = !isMinimized}>
    <span class="title">New Message</span>
    <div class="header-actions" onclick={(e) => e.stopPropagation()}>
      <button class="action-btn" onclick={() => isMinimized = !isMinimized}>
        {@html iconMinimize}
      </button>
      <button class="action-btn" onclick={() => { isExpanded = !isExpanded; isMinimized = false; }}>
        {@html iconMaximize}
      </button>
      <button class="action-btn close-btn" onclick={saveDraft}>
        {@html iconClose}
      </button>
    </div>
  </header>

  {#if !isMinimized}
    <div class="compose-scroll-area">
      <div class="compose-fields">
        <div class="field-row">
          <span class="field-label">To</span>
          <div class="input-container">
            <input 
              type="text" 
              class="field-input" 
              bind:value={to} 
              placeholder="Recipients" 
              oninput={() => handleInput('to', to)}
              onkeydown={handleKeydown}
              onblur={() => setTimeout(() => { if (activeField === 'to') suggestions = [] }, 200)}
            />
            {#if activeField === 'to' && suggestions.length > 0}
              <div class="suggestions-dropdown">
                {#each suggestions as s, i}
                  <button 
                    class="suggestion-item" 
                    class:active={i === suggestionIndex}
                    onclick={() => selectSuggestion(s)}
                  >
                    <div class="s-name">{s.name || s.email}</div>
                    {#if s.name}<div class="s-email">{s.email}</div>{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
          <div class="cc-bcc-toggles">
            {#if !showCc}<button onclick={() => showCc = true}>Cc</button>{/if}
            {#if !showBcc}<button onclick={() => showBcc = true}>Bcc</button>{/if}
          </div>
        </div>
        
        {#if showCc}
        <div class="field-row">
          <span class="field-label">Cc</span>
          <div class="input-container">
            <input 
              type="text" 
              class="field-input" 
              bind:value={cc} 
              oninput={() => handleInput('cc', cc)}
              onkeydown={handleKeydown}
              onblur={() => setTimeout(() => { if (activeField === 'cc') suggestions = [] }, 200)}
            />
            {#if activeField === 'cc' && suggestions.length > 0}
              <div class="suggestions-dropdown">
                {#each suggestions as s, i}
                  <button 
                    class="suggestion-item" 
                    class:active={i === suggestionIndex}
                    onclick={() => selectSuggestion(s)}
                  >
                    <div class="s-name">{s.name || s.email}</div>
                    {#if s.name}<div class="s-email">{s.email}</div>{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
        {/if}

        {#if showBcc}
        <div class="field-row">
          <span class="field-label">Bcc</span>
          <div class="input-container">
            <input 
              type="text" 
              class="field-input" 
              bind:value={bcc} 
              oninput={() => handleInput('bcc', bcc)}
              onkeydown={handleKeydown}
              onblur={() => setTimeout(() => { if (activeField === 'bcc') suggestions = [] }, 200)}
            />
            {#if activeField === 'bcc' && suggestions.length > 0}
              <div class="suggestions-dropdown">
                {#each suggestions as s, i}
                  <button 
                    class="suggestion-item" 
                    class:active={i === suggestionIndex}
                    onclick={() => selectSuggestion(s)}
                  >
                    <div class="s-name">{s.name || s.email}</div>
                    {#if s.name}<div class="s-email">{s.email}</div>{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
        {/if}

        
        <div class="field-row subject-row">
          <input type="text" class="field-input" bind:value={subject} placeholder="Subject" />
        </div>
      </div>

      
      <div class="body-editor-container">
        <div 
          class="rich-text-editor" 
          contenteditable="true" 
          bind:this={editorEl}
          oninput={(e) => bodyHTML = e.currentTarget.innerHTML}
        ></div>
      </div>
    </div>

    
    <footer class="compose-toolbar">
      <div class="formatting-tools">
        <button class="send-btn" onclick={send} disabled={isSending}>
          {#if isSending} <div class="spinner"></div> {:else} Send {/if}
        </button>
        <div class="divider"></div>
        <button class="format-btn" title="Bold" onclick={() => format('bold')}>{@html iconBold}</button>
        <button class="format-btn" title="Italic" onclick={() => format('italic')}>{@html iconItalic}</button>
        <button class="format-btn" title="Underline" onclick={() => format('underline')}>{@html iconUnderline}</button>
      </div>
      <div class="trailing-actions">
        <button class="trash-btn" title="Discard" onclick={onClose}>{@html iconTrash}</button>
      </div>
    </footer>
  {/if}
</div>

<style>
  .compose-window {
    position: fixed;
    bottom: 0;
    right: 80px;
    width: 500px;
    height: 550px;
    background: var(--bg-view, #ffffff);
    border-radius: 12px 12px 0 0;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.15), 0 0 1px rgba(0,0,0,0.2);
    display: flex;
    flex-direction: column;
    z-index: 9999;
    overflow: hidden;
    transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
    color: var(--text-primary);
  }

  .compose-window.minimized {
    height: 40px;
  }
  
  .compose-window.expanded {
    width: 80vw;
    height: 80vh;
    right: 10vw;
    bottom: 10vh;
    border-radius: 12px;
  }

  .compose-header {
    height: 40px;
    background: var(--bg-panel);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 0 16px;
    cursor: pointer;
    border-bottom: 1px solid var(--border-color);
  }

  .compose-header .title {
    font-size: 14px;
    font-weight: 500;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .action-btn {
    background: transparent;
    border: none;
    width: 28px;
    height: 28px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.15s;
  }

  .action-btn:hover {
    background: rgba(0,0,0,0.05);
  }

  .close-btn:hover {
    background: #ff3b30;
    color: white;
  }

  .compose-scroll-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden; /* Scroll inside the editor container */
  }

  .compose-fields {
    padding: 0 16px;
    border-bottom: 1px solid var(--border-color);
  }

  .field-row {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--border-color);
    min-height: 38px;
  }

  .field-row:last-child {
    border-bottom: none;
  }

  .field-label {
    color: var(--text-secondary);
    font-size: 14px;
    width: 40px;
    flex-shrink: 0;
  }

  .field-input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: 14px;
    color: var(--text-primary);
    outline: none;
    padding: 8px 0;
    width: 100%;
  }

  .input-container {
    flex: 1;
    position: relative;
    display: flex;
  }

  .suggestions-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-top: none;
    border-radius: 0 0 8px 8px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.1);
    z-index: 1000;
    max-height: 200px;
    overflow-y: auto;
  }

  .suggestion-item {
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .suggestion-item:hover, .suggestion-item.active {
    background: var(--sidebar-hover);
  }

  .s-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .s-email {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .subject-row .field-input {
    font-weight: 500;
  }

  .cc-bcc-toggles {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .cc-bcc-toggles button {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 13px;
    cursor: pointer;
    padding: 2px;
  }
  .cc-bcc-toggles button:hover {
    text-decoration: underline;
  }

  .body-editor-container {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }

  .rich-text-editor {
    min-height: 100%;
    outline: none;
    font-size: 14px;
    line-height: 1.5;
  }

  .compose-toolbar {
    height: 52px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border-color);
  }

  .formatting-tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .send-btn {
    background: var(--accent-blue, #0A84FF);
    color: white;
    font-weight: 500;
    font-size: 14px;
    border: none;
    border-radius: 16px;
    padding: 0 20px;
    height: 32px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 80px;
    transition: opacity 0.2s;
  }

  .send-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .send-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .divider {
    width: 1px;
    height: 20px;
    background: var(--border-color);
    margin: 0 6px;
  }

  .format-btn {
    width: 32px;
    height: 32px;
    background: transparent;
    border: none;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .format-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  .trash-btn {
    width: 36px;
    height: 36px;
    background: transparent;
    border: none;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .trash-btn:hover {
    background: var(--sidebar-hover);
    color: #ff3b30;
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255,255,255,0.4);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
</style>
