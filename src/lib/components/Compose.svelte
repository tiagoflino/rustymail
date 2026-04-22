<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open as dialogOpen } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy, untrack } from "svelte";
  import { iconClose, iconTrash, iconSent, iconCheck } from "./icons";
  import { fly } from "svelte/transition";
  import { toasts, addToast, removeToast } from "$lib/stores/toast";

  let {
    onClose,
    initialTo = "",
    initialCc = "",
    initialSubject = "",
    initialBodyHTML = "",
    threadId = null,
    inReplyTo = null,
    references = null,
    initialDraftId = null,
    accountId = null,
    onDraftSaved = (id: string | null) => {},
  }: {
    onClose: () => void;
    initialTo?: string;
    initialCc?: string;
    initialSubject?: string;
    initialBodyHTML?: string;
    threadId?: string | null;
    inReplyTo?: string | null;
    references?: string | null;
    initialDraftId?: string | null;
    accountId?: string | null;
    onDraftSaved?: (id: string | null) => void;
  } = $props();

  let isMinimized = $state(false);
  let isExpanded = $state(false);

  let to = $state(untrack(() => "" + initialTo));
  let cc = $state(untrack(() => "" + initialCc));
  let bcc = $state("");
  let subject = $state(untrack(() => "" + initialSubject));
  let bodyHTML = $state(untrack(() => "" + initialBodyHTML));

  let showCc = $state(untrack(() => initialCc.length > 0));
  let showBcc = $state(false);

  let suggestions = $state<any[]>([]);
  let suggestionIndex = $state(0);
  let activeField = $state<"to" | "cc" | "bcc" | null>(null);
  let suggestionDebounce: any;
  let currentDraftId = $state<string | null>(untrack(() => initialDraftId));

  interface ComposeAttachment {
    path: string;
    name: string;
    size: number;
  }
  let attachments = $state<ComposeAttachment[]>([]);
  let totalAttachmentSize = $derived(attachments.reduce((sum, a) => sum + a.size, 0));
  let isDragging = $state(false);
  let isSending = $state(false);
  let showScheduleMenu = $state(false);
  let showDatePicker = $state(false);
  let customDateTime = $state('');
  let showTemplatePicker = $state(false);
  let templates = $state<Array<{id: string, name: string, subject: string, body_html: string}>>([]);
  let showSaveTemplateInput = $state(false);
  let newTemplateName = $state('');
  const MAX_ATTACHMENT_SIZE = 25 * 1024 * 1024;

  function formatSize(bytes: number): string {
    if (bytes < 1024) return bytes + " B";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(0) + " KB";
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  }

  async function addFiles(paths: string[]) {
    for (const p of paths) {
      const name = p.split("/").pop() || p.split("\\").pop() || "file";
      let size = 0;
      try {
        const stat: any = await invoke("get_file_size", { path: p });
        size = stat;
      } catch {
        size = 0;
      }
      const newTotal = totalAttachmentSize + size;
      if (newTotal > MAX_ATTACHMENT_SIZE) {
        // Offer Drive upload for oversized files
        const filePath = p;
        const fileName = name;
        const fileSize = size;
        addToast(
          `"${fileName}" (${formatSize(fileSize)}) exceeds 25MB limit. Upload to Google Drive?`,
          "info",
          0,
          {
            label: "Upload",
            onClick: () => handleDriveUpload(filePath, fileName, fileSize),
          }
        );
        continue;
      }
      if (!attachments.some((a) => a.path === p)) {
        attachments = [...attachments, { path: p, name, size }];
      }
    }
  }

  function removeAttachment(index: number) {
    attachments = attachments.filter((_, i) => i !== index);
  }

  async function handleDriveUpload(filePath: string, fileName: string, fileSize: number) {
    const loadingId = addToast(`Uploading "${fileName}" to Google Drive...`, "info", 0);
    try {
      const link: string = await invoke("upload_to_drive", { filePath });
      removeToast(loadingId);
      if (editorEl) {
        const linkHtml = `<p><a href="${link}">${fileName} (${formatSize(fileSize)})</a></p>`;
        editorEl.innerHTML += linkHtml;
        bodyHTML = editorEl.innerHTML;
      }
      addToast(`"${fileName}" uploaded and link inserted.`, "success");
    } catch (e) {
      removeToast(loadingId);
      addToast(`Drive upload failed: ${e}`, "error", 7000);
    }
  }

  async function pickFiles() {
    const selected = await dialogOpen({ multiple: true, title: "Attach files" });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      addFiles(paths);
    }
  }

  let unlistenDrop: (() => void) | null = null;

  async function handleInput(field: "to" | "cc" | "bcc", val: string) {
    activeField = field;
    if (suggestionDebounce) clearTimeout(suggestionDebounce);

    const lastPart = val.split(",").pop()?.trim() || "";
    if (lastPart.length < 2) {
      suggestions = [];
      return;
    }

    suggestionDebounce = setTimeout(async () => {
      try {
        suggestions = await invoke("search_contacts", { query: lastPart });
        suggestionIndex = 0;
      } catch (e) {
        suggestions = [];
      }
    }, 200);
  }

  function selectSuggestion(suggestion: any) {
    if (!activeField) return;

    let currentVal =
      activeField === "to" ? to : activeField === "cc" ? cc : bcc;
    const parts = currentVal.split(",").map((p) => p.trim());
    parts.pop(); // remove the partial

    const formatted = suggestion.name
      ? `${suggestion.name} <${suggestion.email}>`
      : suggestion.email;
    parts.push(formatted);

    const newVal = parts.join(", ") + ", ";
    if (activeField === "to") to = newVal;
    else if (activeField === "cc") cc = newVal;
    else bcc = newVal;

    suggestions = [];
  }

  function handleKeydown(e: KeyboardEvent) {
    if (suggestions.length === 0) return;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      suggestionIndex = (suggestionIndex + 1) % suggestions.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      suggestionIndex =
        (suggestionIndex - 1 + suggestions.length) % suggestions.length;
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      selectSuggestion(suggestions[suggestionIndex]);
    } else if (e.key === "Escape") {
      suggestions = [];
    }
  }

  let editorEl = $state<HTMLDivElement>();

  const fonts = [
    { label: "Sans Serif", value: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif" },
    { label: "Serif", value: "Georgia, 'Times New Roman', Times, serif" },
    { label: "Monospace", value: "'SF Mono', 'Fira Code', 'Cascadia Code', Menlo, Consolas, monospace" },
    { label: "Arial", value: "Arial, Helvetica, sans-serif" },
    { label: "Verdana", value: "Verdana, Geneva, sans-serif" },
    { label: "Tahoma", value: "Tahoma, Geneva, sans-serif" },
    { label: "Georgia", value: "Georgia, 'Times New Roman', serif" },
    { label: "Courier", value: "'Courier New', Courier, monospace" },
  ];
  let selectedFont = $state(fonts[0].value);
  let selectedFontLabel = $derived(fonts.find(f => f.value === selectedFont)?.label ?? "Sans Serif");
  let showFontDropdown = $state(false);
  let fontPickerEl = $state<HTMLDivElement>();

  function handleWindowClick(e: MouseEvent) {
    if (showFontDropdown && fontPickerEl && !fontPickerEl.contains(e.target as Node)) {
      showFontDropdown = false;
    }
  }

  function format(command: string, value: string | undefined = undefined) {
    document.execCommand(command, false, value);
    editorEl?.focus();
  }

  function applyFont(fontFamily: string) {
    selectedFont = fontFamily;
    document.execCommand("fontName", false, fontFamily);
    editorEl?.focus();
  }

  function toggleTemplatePicker() {
    showTemplatePicker = !showTemplatePicker;
    showSaveTemplateInput = false;
    newTemplateName = '';
    if (showTemplatePicker) {
      invoke("get_templates").then((t: any) => { templates = t; }).catch(() => {});
    }
  }

  function insertTemplate(tmpl: typeof templates[0]) {
    showTemplatePicker = false;

    if (!subject.trim() && tmpl.subject) {
      subject = tmpl.subject;
    }

    if (editorEl) {
      const sigEl = editorEl.querySelector('.rustymail-signature');
      if (sigEl) {
        // Preserve signature and everything after it
        const sigHtml = sigEl.outerHTML;
        let afterSig = '';
        let sibling = sigEl.nextSibling;
        while (sibling) {
          afterSig += sibling instanceof Element ? sibling.outerHTML : sibling.textContent || '';
          sibling = sibling.nextSibling;
        }
        editorEl.innerHTML = tmpl.body_html + sigHtml + afterSig;
      } else {
        editorEl.innerHTML = tmpl.body_html;
      }
      bodyHTML = editorEl.innerHTML;
      editorEl.focus();
    }
  }

  async function saveAsTemplate() {
    const name = newTemplateName.trim();
    if (!name) return;

    let content = '';
    if (editorEl) {
      const sigEl = editorEl.querySelector('.rustymail-signature');
      if (sigEl) {
        const range = document.createRange();
        range.setStartBefore(editorEl.firstChild || editorEl);
        range.setEndBefore(sigEl);
        const frag = range.cloneContents();
        const temp = document.createElement('div');
        temp.appendChild(frag);
        content = temp.innerHTML;
      } else {
        content = editorEl.innerHTML;
      }
    }

    try {
      await invoke("create_template", {
        name,
        subject: subject || '',
        bodyHtml: content,
      });
      templates = await invoke("get_templates") as typeof templates;
      showSaveTemplateInput = false;
      showTemplatePicker = false;
      newTemplateName = '';
      addToast(`Template "${name}" saved`, "success", 3000);
    } catch (e: any) {
      addToast(`Failed to save template: ${e}`, "error");
    }
  }

  onMount(async () => {
    try {
      const sig = (await invoke("get_setting", { key: "signature" })) as string;
      let newHtml = initialBodyHTML;
      if (sig) {
        newHtml =
          `<br><br><div class="rustymail-signature" style="color: var(--text-secondary); opacity: 0.8; font-size: 13px;">${sig}</div>` +
          (initialBodyHTML ? `<br>${initialBodyHTML}` : "");
      }
      if (newHtml) {
        if (editorEl) editorEl.innerHTML = newHtml;
      }

      const range = document.createRange();
      const sel = window.getSelection();
      if (editorEl) range.setStart(editorEl, 0);
      range.collapse(true);
      sel?.removeAllRanges();
      sel?.addRange(range);
    } catch (e) {
      if (initialBodyHTML) {
        if (editorEl) editorEl.innerHTML = initialBodyHTML;
      }
    }

    try {
      templates = await invoke("get_templates") as typeof templates;
    } catch {}

    unlistenDrop = (await listen<{ paths: string[] }>("tauri://drag-drop", (event) => {
      if (event.payload.paths?.length) {
        addFiles(event.payload.paths);
      }
      isDragging = false;
    })) as unknown as () => void;
  });

  onDestroy(() => {
    if (unlistenDrop) unlistenDrop();
  });

  async function send() {
    if (!to) {
      addToast("Please specify at least one recipient.", "info");
      return;
    }

    // Capture compose data before closing
    const sendPayload = {
      to: `${to}${cc ? "," + cc : ""}${bcc ? "," + bcc : ""}`,
      subject,
      body: editorEl?.innerHTML || "",
      threadId,
      inReplyTo: inReplyTo,
      references: references,
      draftId: currentDraftId,
      attachmentPaths: attachments.length > 0 ? attachments.map((a) => a.path) : null,
      accountId: accountId,
    };

    // Read the undo send delay setting
    let delaySec = 5;
    try {
      const val = (await invoke("get_setting", { key: "undo_send_delay" })) as string;
      delaySec = parseInt(val) || 0;
    } catch {
      delaySec = 5;
    }

    // Close compose immediately
    onClose();

    // If delay is 0 (Off), send immediately
    if (delaySec <= 0) {
      try {
        await invoke("send_message", sendPayload);
        addToast("Message sent successfully.", "success", 5000);
      } catch (e) {
        addToast(`Failed to send: ${e}`, "error", 7000);
      }
      return;
    }

    // Delayed send with undo
    let cancelled = false;
    const delayMs = delaySec * 1000;

    const toastId = addToast(
      `Sending in ${delaySec}s…`,
      "info",
      0, // persistent — we manage removal ourselves
      {
        label: "Undo",
        onClick: () => {
          cancelled = true;
          addToast("Send cancelled.", "info", 3000);
        },
      }
    );

    // Also cancel if user dismisses the toast via the X button
    const unsub = toasts.subscribe((all) => {
      if (!cancelled && !all.some((t) => t.id === toastId)) {
        cancelled = true;
      }
    });

    // Countdown update
    let remaining = delaySec;
    const countdownInterval = setInterval(() => {
      remaining--;
      if (remaining > 0 && !cancelled) {
        toasts.update((all) =>
          all.map((t) =>
            t.id === toastId ? { ...t, message: `Sending in ${remaining}s…` } : t
          )
        );
      } else {
        clearInterval(countdownInterval);
      }
    }, 1000);

    // Wait for the delay
    await new Promise((resolve) => setTimeout(resolve, delayMs));
    clearInterval(countdownInterval);
    unsub();

    if (cancelled) {
      removeToast(toastId);
      // Save as draft so the message isn't lost
      try {
        await invoke("save_draft", {
          to: sendPayload.to,
          subject: sendPayload.subject,
          body: sendPayload.body,
          threadId: sendPayload.threadId,
          inReplyTo: sendPayload.inReplyTo,
          references: sendPayload.references,
          draftId: sendPayload.draftId,
          attachmentPaths: sendPayload.attachmentPaths,
          accountId: sendPayload.accountId,
        });
        
        // Ensure UI updates
        if (sendPayload.threadId) {
          await invoke("sync_thread_messages", { threadId: sendPayload.threadId }).catch(() => {});
        }
        await invoke("fetch_label_threads", { labelId: "DRAFT", accountId: null }).catch(() => {});
        
        addToast("Send cancelled — message saved as draft.", "info", 4000);
      } catch (e) {
        addToast(`Failed to save draft: ${e}`, "error", 5000);
      }
      return;
    }

    // Actually send
    removeToast(toastId);
    try {
      await invoke("send_message", sendPayload);
      addToast("Message sent successfully.", "success", 5000);
    } catch (e) {
      addToast(`Failed to send: ${e}`, "error", 7000);
    }
  }


  async function saveDraft() {
    try {
      currentDraftId = (await invoke("save_draft", {
        to: `${to}${cc ? "," + cc : ""}${bcc ? "," + bcc : ""}`,
        subject,
        body: editorEl?.innerHTML || "",
        threadId,
        inReplyTo,
        references,
        draftId: currentDraftId,
        attachmentPaths: attachments.length > 0 ? attachments.map((a) => a.path) : null,
        accountId: accountId,
      })) as string;
      if (currentDraftId) {
        onDraftSaved(currentDraftId);
      }

      // Re-sync thread messages from Gmail to replace stale local records.
      // Gmail changes the message ID on draft update, so without this,
      // old and new messages both appear in the thread view.
      if (threadId) {
        try {
          await invoke("sync_thread_messages", { threadId });
        } catch (_) {
          // Non-critical — thread will self-heal on next full sync
        }
      }

      addToast("Draft saved.", "success");
      onClose();
    } catch (e) {
      addToast(`Failed to save draft: ${e}`, "error", 7000);
    }
  }

  async function discardDraft() {
    if (currentDraftId) {
      try {
        await invoke("delete_draft", { draftId: currentDraftId });
        onDraftSaved(null);
        addToast("Draft discarded.", "info");
      } catch (e) {
        console.error("Failed to delete draft:", e);
        addToast(`Failed to discard draft: ${e}`, "error", 5000);
      }
    }
    // Re-sync thread to remove stale draft message from local DB
    if (threadId) {
      try {
        await invoke("sync_thread_messages", { threadId });
      } catch (_) {}
    }
    onClose();
  }

  function computeTomorrowMorning(): number {
    const d = new Date();
    d.setDate(d.getDate() + 1);
    d.setHours(9, 0, 0, 0);
    return Math.floor(d.getTime() / 1000);
  }

  function computeNextMonday(): number {
    const d = new Date();
    const day = d.getDay();
    const daysUntilMonday = day === 0 ? 1 : day === 1 ? 7 : 8 - day;
    d.setDate(d.getDate() + daysUntilMonday);
    d.setHours(9, 0, 0, 0);
    return Math.floor(d.getTime() / 1000);
  }

  function formatScheduleTime(ts: number): string {
    return new Date(ts * 1000).toLocaleString(undefined, {
      weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit'
    });
  }

  function getEditorContent(): string {
    return editorEl?.innerHTML || "";
  }

  async function scheduleSend(sendAt: number) {
    if (!to.trim()) {
      addToast("Please add a recipient", "error");
      return;
    }
    showScheduleMenu = false;
    showDatePicker = false;

    try {
      const draftId = await invoke("save_draft", {
        to: `${to.trim()}${cc ? "," + cc : ""}${bcc ? "," + bcc : ""}`,
        subject,
        body: getEditorContent(),
        threadId: threadId || null,
        inReplyTo: inReplyTo || null,
        references: references || null,
        draftId: currentDraftId || null,
        attachmentPaths: attachments.length > 0 ? attachments.map(a => a.path) : null,
        accountId: accountId || null,
      }) as string;

      const scheduleId = await invoke("schedule_send", {
        draftId,
        threadId: threadId || null,
        toRecipients: to.trim(),
        subject: subject || "(no subject)",
        sendAt,
      });

      const timeStr = formatScheduleTime(sendAt);
      addToast(`Scheduled for ${timeStr}`, "info", 8000, {
        label: "Undo",
        onClick: async () => {
          try {
            await invoke("cancel_scheduled_send", { id: scheduleId });
            addToast("Schedule cancelled", "info");
          } catch (e) {
            addToast(`Failed to cancel: ${e}`, "error");
          }
        }
      });

      onClose();
    } catch (e: any) {
      addToast(`Failed to schedule: ${e}`, "error");
    }
  }

  const iconMinimize = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="5" y1="12" x2="19" y2="12"/></svg>`;
  const iconMaximize = `<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg>`;
  const iconBold = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"></path><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"></path></svg>`;
  const iconItalic = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="19" y1="4" x2="10" y2="4"></line><line x1="14" y1="20" x2="5" y2="20"></line><line x1="15" y1="4" x2="9" y2="20"></line></svg>`;
  const iconUnderline = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 3v7a6 6 0 0 0 6 6 6 6 0 0 0 6-6V3"></path><line x1="4" y1="21" x2="20" y2="21"></line></svg>`;
</script>

<svelte:window onclick={handleWindowClick} />

<div
  class="compose-window"
  class:minimized={isMinimized}
  class:expanded={isExpanded}
  transition:fly={{ y: 20, duration: 250 }}
>
  <header
    class="compose-header"
    onclick={() => (isMinimized = !isMinimized)}
    onkeydown={(e) =>
      (e.key === "Enter" || e.key === " ") && (isMinimized = !isMinimized)}
    tabindex="0"
    role="button"
  >
    <span class="title">New Message</span>
    <div class="header-actions" role="none">
      <button
        class="action-btn"
        onclick={(e) => {
          e.stopPropagation();
          isMinimized = !isMinimized;
        }}
      >
        {@html iconMinimize}
      </button>
      <button
        class="action-btn"
        onclick={(e) => {
          e.stopPropagation();
          isExpanded = !isExpanded;
          isMinimized = false;
        }}
      >
        {@html iconMaximize}
      </button>
      <button
        class="action-btn close-btn"
        onclick={(e) => {
          e.stopPropagation();
          saveDraft();
        }}
      >
        {@html iconClose}
      </button>
    </div>
  </header>

  <div class="compose-body" class:compose-hidden={isMinimized}>
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
              oninput={() => handleInput("to", to)}
              onkeydown={handleKeydown}
              onblur={() =>
                setTimeout(() => {
                  if (activeField === "to") suggestions = [];
                }, 200)}
            />
            {#if activeField === "to" && suggestions.length > 0}
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
            {#if !showCc}<button onclick={() => (showCc = true)}>Cc</button
              >{/if}
            {#if !showBcc}<button onclick={() => (showBcc = true)}>Bcc</button
              >{/if}
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
                oninput={() => handleInput("cc", cc)}
                onkeydown={handleKeydown}
                onblur={() =>
                  setTimeout(() => {
                    if (activeField === "cc") suggestions = [];
                  }, 200)}
              />
              {#if activeField === "cc" && suggestions.length > 0}
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
                oninput={() => handleInput("bcc", bcc)}
                onkeydown={handleKeydown}
                onblur={() =>
                  setTimeout(() => {
                    if (activeField === "bcc") suggestions = [];
                  }, 200)}
              />
              {#if activeField === "bcc" && suggestions.length > 0}
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
          <input
            type="text"
            class="field-input"
            bind:value={subject}
            placeholder="Subject"
          />
        </div>
      </div>

      <div
        class="body-editor-container"
        role="region"
        ondragover={(e) => { e.preventDefault(); isDragging = true; }}
        ondragleave={() => isDragging = false}
        ondrop={(e) => e.preventDefault()}
      >
        {#if isDragging}
          <div class="drop-overlay">Drop files to attach</div>
        {/if}
        <div
          class="rich-text-editor"
          contenteditable="true"
          bind:this={editorEl}
          oninput={(e) => (bodyHTML = e.currentTarget.innerHTML)}
        ></div>
      </div>
    </div>

    {#if attachments.length > 0}
      <div class="compose-attachments">
        {#each attachments as att, i}
          <div class="compose-attachment-chip">
            <svg class="att-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48"/></svg>
            <span class="att-name">{att.name}</span>
            <span class="att-size">{formatSize(att.size)}</span>
            <button class="att-remove" onclick={() => removeAttachment(i)} title="Remove">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
          </div>
        {/each}
        <span class="att-total">{formatSize(totalAttachmentSize)} / 25 MB</span>
      </div>
    {/if}

    <footer class="compose-toolbar">
      <div class="formatting-tools">
        {#if showScheduleMenu}
          <div class="schedule-backdrop" onclick={() => { showScheduleMenu = false; showDatePicker = false; }} role="presentation"></div>
        {/if}
        <div class="send-split">
          <button class="send-btn" disabled={isSending} onclick={send}>
            {isSending ? "Sending..." : "Send"}
          </button>
          <button class="send-dropdown" disabled={isSending} onclick={(e) => { e.stopPropagation(); showScheduleMenu = !showScheduleMenu; }} title="Schedule Send">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
          </button>
          {#if showScheduleMenu}
            <div class="schedule-menu">
              <div class="schedule-header">Schedule Send</div>
              <button class="schedule-option" onclick={() => scheduleSend(computeTomorrowMorning())}>
                <span class="schedule-label">Tomorrow Morning</span>
                <span class="schedule-time">{new Date(computeTomorrowMorning() * 1000).toLocaleDateString(undefined, { weekday: 'short' })} 9:00 AM</span>
              </button>
              <button class="schedule-option" onclick={() => scheduleSend(computeNextMonday())}>
                <span class="schedule-label">Monday Morning</span>
                <span class="schedule-time">{new Date(computeNextMonday() * 1000).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' })} 9:00 AM</span>
              </button>
              <div class="schedule-divider"></div>
              <button class="schedule-option" onclick={() => { showDatePicker = true; }}>
                <span class="schedule-label">Pick date & time...</span>
              </button>
              {#if showDatePicker}
                <div class="schedule-picker">
                  <input
                    type="datetime-local"
                    class="schedule-datetime"
                    bind:value={customDateTime}
                    min={new Date(Date.now() + 60000).toISOString().slice(0, 16)}
                  />
                  <button
                    class="schedule-confirm"
                    disabled={!customDateTime}
                    onclick={() => {
                      const ts = Math.floor(new Date(customDateTime).getTime() / 1000);
                      scheduleSend(ts);
                    }}
                  >Schedule</button>
                </div>
              {/if}
            </div>
          {/if}
        </div>
        <div class="divider"></div>
        <div class="font-picker" bind:this={fontPickerEl}>
          <button
            class="font-select"
            onclick={() => showFontDropdown = !showFontDropdown}
            title="Font"
          >
            <span style:font-family={selectedFont}>{selectedFontLabel}</span>
            <svg width="8" height="5" viewBox="0 0 8 5"><path d="M0.5 0.5L4 4L7.5 0.5" stroke="currentColor" fill="none" stroke-linecap="round"/></svg>
          </button>
          {#if showFontDropdown}
            <div class="font-dropdown">
              {#each fonts as f}
                <button
                  class="font-dropdown-item"
                  class:active={selectedFont === f.value}
                  style:font-family={f.value}
                  onmousedown={(e) => { e.preventDefault(); applyFont(f.value); showFontDropdown = false; }}
                >{f.label}</button>
              {/each}
            </div>
          {/if}
        </div>
        <div class="divider"></div>
        <div class="template-picker-wrap">
          <button class="toolbar-btn template-btn" onclick={toggleTemplatePicker} title="Templates">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
          </button>
          {#if showTemplatePicker}
            <div class="template-backdrop" onclick={() => { showTemplatePicker = false; showSaveTemplateInput = false; }} role="presentation"></div>
            <div class="template-dropdown">
              <div class="template-dd-header">Templates</div>
              {#if templates.length === 0 && !showSaveTemplateInput}
                <div class="template-empty">No templates yet</div>
              {:else}
                <div class="template-list">
                  {#each templates as tmpl}
                    <button class="template-option" onclick={() => insertTemplate(tmpl)}>
                      <span class="template-opt-name">{tmpl.name}</span>
                      {#if tmpl.subject}
                        <span class="template-opt-subject">{tmpl.subject}</span>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
              <div class="template-dd-divider"></div>
              {#if showSaveTemplateInput}
                <div class="template-save-form">
                  <input
                    type="text"
                    class="template-name-input"
                    placeholder="Template name"
                    bind:value={newTemplateName}
                    onkeydown={(e) => { if (e.key === 'Enter') saveAsTemplate(); if (e.key === 'Escape') { showSaveTemplateInput = false; } }}
                  />
                  <button class="template-save-btn" disabled={!newTemplateName.trim()} onclick={saveAsTemplate}>Save</button>
                </div>
              {:else}
                <button class="template-option template-action" onclick={() => { showSaveTemplateInput = true; setTimeout(() => (document.querySelector('.template-name-input') as HTMLElement)?.focus(), 50); }}>
                  Save current email as template…
                </button>
              {/if}
            </div>
          {/if}
        </div>
        <div class="divider"></div>
        <button class="format-btn" title="Bold" onclick={() => format("bold")}
          >{@html iconBold}</button
        >
        <button
          class="format-btn"
          title="Italic"
          onclick={() => format("italic")}>{@html iconItalic}</button
        >
        <button
          class="format-btn"
          title="Underline"
          onclick={() => format("underline")}>{@html iconUnderline}</button
        >
        <div class="divider"></div>
        <button class="format-btn" title="Attach file" onclick={pickFiles}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48"/></svg>
        </button>
      </div>
      <div class="trailing-actions">
        <button class="trash-btn" title="Discard" onclick={discardDraft}
          >{@html iconTrash}</button
        >
      </div>
    </footer>
  </div>
</div>

<style>
  .compose-window {
    position: fixed;
    bottom: 0;
    right: 80px;
    width: 620px;
    height: 580px;
    background: var(--bg-view, #ffffff);
    border-radius: var(--radius-modal) var(--radius-modal) 0 0;
    box-shadow:
      0 8px 30px rgba(0, 0, 0, 0.15),
      0 0 1px rgba(0, 0, 0, 0.2);
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
  .compose-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: hidden;
  }
  .compose-hidden {
    display: none;
  }

  .compose-window.expanded {
    width: 80vw;
    height: 80vh;
    right: 10vw;
    bottom: 10vh;
    border-radius: var(--radius-modal);
  }

  .compose-header {
    height: var(--toolbar-height);
    background: var(--bg-panel);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 0 16px;
    cursor: pointer;
    border-bottom: 1px solid var(--border-color);
  }

  .compose-header .title {
    font-size: var(--font-size-detail);
    font-weight: 500;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
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

  .action-btn:focus-visible {
    outline: 2px solid var(--accent-blue);
    outline-offset: -2px;
  }

  .action-btn:hover {
    background: rgba(0, 0, 0, 0.05);
  }

  .close-btn:hover {
    background: var(--destructive-red, #ff3b30);
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
    min-height: 32px;
  }

  .field-row:last-child {
    border-bottom: none;
  }

  .field-label {
    color: var(--text-secondary);
    font-size: var(--font-size-detail);
    width: 40px;
    flex-shrink: 0;
  }

  .field-input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: var(--font-size-detail);
    color: var(--text-primary);
    outline: none;
    padding: 8px 0;
    width: 100%;
  }

  .field-input:focus-visible {
    outline: none;
    box-shadow: 0 1px 0 0 var(--accent-blue);
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
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
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

  .suggestion-item:focus-visible {
    outline: 2px solid var(--accent-blue);
    outline-offset: -2px;
  }

  .suggestion-item:hover,
  .suggestion-item.active {
    background: var(--sidebar-hover);
  }

  .s-name {
    font-size: var(--font-size-base);
    font-weight: 500;
    color: var(--text-primary);
  }

  .s-email {
    font-size: var(--font-size-toolbar);
    line-height: 15px;
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
    font-size: var(--font-size-base);
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
    font-size: var(--font-size-detail);
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

  .send-split {
    display: flex;
    position: relative;
  }
  .send-btn {
    background: var(--accent-blue, #0a84ff);
    color: white;
    font-weight: 500;
    font-size: var(--font-size-detail);
    border: none;
    border-radius: var(--radius-pill) 0 0 var(--radius-pill);
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
  .send-dropdown {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 8px;
    border: none;
    background: var(--accent-blue, #0a84ff);
    color: white;
    cursor: pointer;
    border-radius: 0 var(--radius-pill) var(--radius-pill) 0;
    border-left: 1px solid rgba(255,255,255,0.2);
    transition: background 0.1s;
  }
  .send-dropdown:hover { background: #0070e0; }
  .send-dropdown:disabled { opacity: 0.5; cursor: not-allowed; }
  .schedule-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }
  .schedule-menu {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 6px;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    box-shadow: 0 4px 16px rgba(0,0,0,0.15);
    z-index: 100;
    min-width: 260px;
    overflow: hidden;
  }
  .schedule-header {
    padding: 8px 12px;
    font-size: var(--font-size-small);
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    border-bottom: 1px solid var(--border-color);
  }
  .schedule-option {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: var(--font-size-base);
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }
  .schedule-option:hover { background: var(--sidebar-hover); }
  .schedule-label { font-weight: 500; }
  .schedule-time { font-size: var(--font-size-small); color: var(--text-secondary); }
  .schedule-divider { height: 1px; background: var(--border-color); margin: 2px 0; }
  .schedule-picker {
    padding: 8px 12px;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .schedule-datetime {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    background: var(--bg-view);
    color: var(--text-primary);
    font-size: var(--font-size-small);
    font-family: inherit;
  }
  .schedule-confirm {
    padding: 6px 12px;
    border: none;
    background: var(--accent-blue);
    color: white;
    border-radius: var(--radius-standard);
    font-size: var(--font-size-small);
    font-family: inherit;
    cursor: pointer;
  }
  .schedule-confirm:disabled { opacity: 0.5; cursor: not-allowed; }

  .divider {
    width: 1px;
    height: 20px;
    background: var(--border-color);
    margin: 0 6px;
  }

  .font-picker {
    position: relative;
  }
  .font-select {
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    background: transparent;
    color: var(--text-primary);
    font-size: var(--font-size-toolbar);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 90px;
    font-family: var(--font-family);
    transition: border-color 0.1s;
  }
  .font-select svg {
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .font-select:focus-visible {
    border-color: var(--accent-blue);
    box-shadow: 0 0 0 2px rgba(10, 132, 255, 0.2);
  }
  .font-select:hover {
    border-color: var(--text-secondary);
  }
  .font-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 4px;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
    min-width: 160px;
    padding: 4px;
    z-index: 100;
  }
  .font-dropdown-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-primary);
    font-size: var(--font-size-base);
    line-height: 16px;
    text-align: left;
    cursor: pointer;
    transition: background 0.1s;
  }
  .font-dropdown-item:hover {
    background: var(--sidebar-hover);
  }
  .font-dropdown-item.active {
    background: rgba(10, 132, 255, 0.1);
    color: var(--accent-blue);
  }
  .format-btn {
    width: 32px;
    height: 32px;
    background: transparent;
    border: none;
    border-radius: var(--radius-standard);
    transition: background 0.15s, color 0.15s;
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

  .format-btn:focus-visible {
    outline: 2px solid var(--accent-blue);
    outline-offset: -2px;
  }

  .trash-btn {
    width: 32px;
    height: 32px;
    background: transparent;
    border: none;
    border-radius: var(--radius-standard);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .trash-btn:focus-visible {
    outline: 2px solid var(--accent-blue);
    outline-offset: -2px;
  }

  .trash-btn:hover {
    background: var(--sidebar-hover);
    color: var(--destructive-red, #ff3b30);
  }

  .body-editor-container {
    position: relative;
  }
  .drop-overlay {
    position: absolute;
    inset: 0;
    background: rgba(10, 132, 255, 0.08);
    border: 2px dashed var(--accent-blue, #0a84ff);
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 500;
    color: var(--accent-blue, #0a84ff);
    z-index: 10;
    pointer-events: none;
  }

  .compose-attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 16px;
    border-top: 1px solid var(--border-color);
    align-items: center;
  }
  .compose-attachment-chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    background: transparent;
    font-family: var(--font-family);
    max-width: 220px;
  }
  .att-icon {
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .att-name {
    font-size: var(--font-size-small);
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .att-size {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .att-remove {
    background: none;
    border: none;
    cursor: pointer;
    padding: 2px;
    display: flex;
    align-items: center;
    color: var(--text-secondary);
    border-radius: 4px;
    flex-shrink: 0;
    transition: color 0.1s, background 0.1s;
  }
  .att-remove:hover {
    color: var(--destructive-red, #ff3b30);
    background: rgba(255, 59, 48, 0.08);
  }
  .att-total {
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    margin-left: auto;
  }

  .template-picker-wrap {
    position: relative;
    display: flex;
  }
  .template-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 32px;
    height: 32px;
    background: transparent;
    border: none;
    border-radius: var(--radius-standard);
    color: var(--text-secondary);
    cursor: pointer;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }
  .template-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }
  .template-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }
  .template-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 6px;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    box-shadow: 0 4px 16px rgba(0,0,0,0.15);
    z-index: 100;
    min-width: 280px;
    max-width: 360px;
    overflow: hidden;
  }
  .template-dd-header {
    padding: 8px 12px;
    font-size: var(--font-size-small);
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    border-bottom: 1px solid var(--border-color);
  }
  .template-list {
    max-height: 200px;
    overflow-y: auto;
  }
  .template-option {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: var(--font-size-base);
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    gap: 2px;
  }
  .template-option:hover { background: var(--sidebar-hover); }
  .template-opt-name { font-weight: 500; }
  .template-opt-subject { font-size: var(--font-size-small); color: var(--text-secondary); }
  .template-dd-divider { height: 1px; background: var(--border-color); }
  .template-action { color: var(--accent-blue); font-weight: 500; }
  .template-empty {
    padding: 12px;
    font-size: var(--font-size-small);
    color: var(--text-secondary);
    text-align: center;
  }
  .template-save-form {
    display: flex;
    gap: 6px;
    padding: 8px 12px;
  }
  .template-name-input {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-standard);
    background: var(--bg-view);
    color: var(--text-primary);
    font-size: var(--font-size-small);
    font-family: inherit;
  }
  .template-name-input:focus { outline: none; border-color: var(--accent-blue); }
  .template-save-btn {
    padding: 6px 12px;
    border: none;
    background: var(--accent-blue);
    color: white;
    border-radius: var(--radius-standard);
    font-size: var(--font-size-small);
    font-family: inherit;
    cursor: pointer;
    white-space: nowrap;
  }
  .template-save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
