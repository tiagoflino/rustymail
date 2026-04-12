<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { addToast } from "$lib/stores/toast";

  interface Props {
    event?: any | null;
    selectedDate?: Date | null;
    onClose: () => void;
    onSave: () => void;
    onDelete: () => void;
  }
  let { event = null, selectedDate = null, onClose, onSave, onDelete }: Props = $props();

  let isSaving = $state(false);
  let isDeleting = $state(false);

  let title = $state("");
  let location = $state("");
  let description = $state("");
  let isEditMode = $state(true);

  function updateState() {
    if (event) {
      title = event.summary || "";
      location = event.location || "";
      description = event.description || "";
    } else {
      title = "";
      location = "";
      description = "";
    }
    isEditMode = !event;
  }

  $effect(() => {
    updateState();
  });

  const iconLocation = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z"/><circle cx="12" cy="10" r="3"/></svg>`;
  const iconVideo = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>`;
  const iconCalendar = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>`;

  function openExternalDirect(url: string) {
    invoke("open_external_url", { url });
  }

  function openUrl(url: string) {
    window.postMessage({ type: 'rustymail-link', url }, '*');
  }

  function getMapUrl(loc: string) {
    if (loc.startsWith("http://") || loc.startsWith("https://")) return loc;
    return `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(loc)}`;
  }

  function extractMeetingLink(ev: any): string | null {
    if (!ev) return null;
    if (ev.hangoutLink) return ev.hangoutLink;
    const textToSearch = `${ev.location || ''} ${ev.description || ''}`;
    const zoomMatch = textToSearch.match(/https:\/\/[a-zA-Z0-9-]+\.zoom\.us\/j\/[a-zA-Z0-9_.-]+/i);
    if (zoomMatch) return zoomMatch[0];
    const teamsMatch = textToSearch.match(/https:\/\/teams\.microsoft\.com\/l\/meetup-join\/[a-zA-Z0-9_.-]+/i);
    if (teamsMatch) return teamsMatch[0];
    const meetMatch = textToSearch.match(/https:\/\/meet\.google\.com\/[a-z-]+/i);
    if (meetMatch) return meetMatch[0];
    return null;
  }

  function formatDescription(text: string) {
    if (!text) return "";
    // If it looks like HTML, don't double format
    if (/<[a-z][\s\S]*>/i.test(text)) {
      return text;
    }
    const urlRegex = /(https?:\/\/[^\s]+)/g;
    let formatted = text.replace(urlRegex, '<a href="$1">$1</a>');
    return formatted.replace(/\n/g, '<br>');
  }

  function handleHtmlClick(e: Event) {
    const target = e.target as HTMLElement;
    const anchor = target.closest('a');
    if (anchor && anchor.href) {
      e.preventDefault();
      window.postMessage({ type: 'rustymail-link', url: anchor.href }, '*');
    }
  }

  // Date formatting helpers
  function formatDateForInput(d: Date) {
    const yy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    return `${yy}-${mm}-${dd}`;
  }
  function formatTimeForInput(d: Date) {
    const hh = String(d.getHours()).padStart(2, "0");
    const min = String(d.getMinutes()).padStart(2, "0");
    return `${hh}:${min}`;
  }

  // Initialize start/end with default values, sync from props in effects
  let startDate = $state(formatDateForInput(new Date()));
  let startTime = $state(formatTimeForInput(new Date()));

  let endDate = $state(formatDateForInput(new Date()));
  let endTime = $state(formatTimeForInput(new Date()));

  let isAllDay = $state(false);

  function updateFromEvent() {
    if (event) {
      const startDt = event.start?.dateTime || event.start?.date || new Date();
      const endDt = event.end?.dateTime || event.end?.date || new Date();
      startDate = formatDateForInput(new Date(startDt));
      startTime = formatTimeForInput(new Date(startDt));
      endDate = formatDateForInput(new Date(endDt));
      endTime = formatTimeForInput(new Date(endDt));
      isAllDay = !!event.start?.date && !event.start?.dateTime;
    } else {
      startDate = formatDateForInput(selectedDate || new Date());
      startTime = formatTimeForInput(selectedDate || new Date());
      endDate = formatDateForInput(selectedDate || new Date());
      endTime = formatTimeForInput(selectedDate || new Date());
      isAllDay = false;
    }
  }

  $effect(() => {
    updateFromEvent();
  });

  async function handleSave() {
    if (!title.trim()) {
      addToast("Event title is required", "error");
      return;
    }

    isSaving = true;

    // Timezone: use local
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;

    let payloadStart: any = {};
    let payloadEnd: any = {};

    if (isAllDay) {
      payloadStart = { date: startDate };
      // Google API expects end date for all-day events to be the day *after* the event
      const eDateObj = new Date(endDate);
      eDateObj.setDate(eDateObj.getDate() + 1);
      payloadEnd = { date: formatDateForInput(eDateObj) };
    } else {
      const startDateTime = new Date(`${startDate}T${startTime}`);
      const endDateTime = new Date(`${endDate}T${endTime}`);
      payloadStart = { dateTime: startDateTime.toISOString(), timeZone: tz };
      payloadEnd = { dateTime: endDateTime.toISOString(), timeZone: tz };
    }

    const payload = {
      summary: title,
      location: location,
      description: description,
      start: payloadStart,
      end: payloadEnd,
    };

    try {
      if (event?.id) {
        await invoke("update_event", { eventId: event.id, event: payload });
        addToast("Event updated", "success");
      } else {
        await invoke("create_event", { event: payload });
        addToast("Event created", "success");
      }
      onSave();
    } catch (e: any) {
      addToast(`Error: ${e}`, "error");
    } finally {
      isSaving = false;
    }
  }

  async function handleDelete() {
    if (!event?.id) return;
    if (!confirm("Are you sure you want to delete this event?")) return;

    isDeleting = true;
    try {
      await invoke("delete_event", { eventId: event.id });
      addToast("Event deleted", "success");
      onDelete();
    } catch (e: any) {
      addToast(`Error deleting event: ${e}`, "error");
    } finally {
      isDeleting = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={onClose} onkeydown={(e) => e.key === 'Enter' && onClose()} role="button" tabindex="0"></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-content" onpointerdown={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <h2>{!event ? "New Event" : (isEditMode ? "Edit Event" : "Event Details")}</h2>
      <button class="close-btn" onclick={onClose}>✕</button>
    </div>

    {#if isEditMode}
      <div class="modal-body">
      <div class="form-group title-group">
        <label for="event-title">Title</label>
        <input
          id="event-title"
          type="text"
          placeholder="Add title"
          bind:value={title}
          class="title-input"
        />
      </div>

      <div class="form-row">
        <label class="toggle-label">
          <input type="checkbox" bind:checked={isAllDay} /> All day
        </label>
      </div>

      <div class="form-row datetime-row">
        <div class="date-col">
          <input type="date" bind:value={startDate} />
          {#if !isAllDay}
            <input type="time" bind:value={startTime} />
          {/if}
        </div>
        <span class="to-span">to</span>
        <div class="date-col">
          {#if !isAllDay}
            <input type="time" bind:value={endTime} />
          {/if}
          <input type="date" bind:value={endDate} />
        </div>
      </div>

      <div class="form-group">
        <label for="event-location">Location</label>
        <input id="event-location" type="text" placeholder="Add location" bind:value={location} />
      </div>

      <div class="form-group">
        <label for="event-description">Description</label>
        <textarea
          id="event-description"
          placeholder="Add description"
          bind:value={description}
          rows="4"
        ></textarea>
      </div>
    </div>

      <div class="modal-footer">
        {#if event?.id}
          <button class="btn-delete" onclick={handleDelete} disabled={isDeleting || isSaving}>
            {isDeleting ? "Deleting..." : "Delete"}
          </button>
        {/if}
        <div class="spacer"></div>
        <button class="btn-cancel" onclick={onClose} disabled={isSaving || isDeleting}>Cancel</button>
        <button class="btn-save" onclick={handleSave} disabled={isSaving || isDeleting}>
          {isSaving ? "Saving..." : "Save"}
        </button>
      </div>

    {:else}
      <!-- Read Only View -->
      <div class="modal-body read-view">
        <h1 class="read-title">{event.summary}</h1>
        <div class="read-time">
           <span class="icon">{@html iconCalendar}</span> {isAllDay ? `All Day, ${startDate}` : `${startDate}  ${startTime} — ${endTime}`}
        </div>
        
        {#if event.location}
          <div class="read-section">
             <button class="location-btn link-style" onclick={() => openExternalDirect(getMapUrl(event.location))} onkeydown={(e) => e.key === "Enter" && openExternalDirect(getMapUrl(event.location))} title="Open Map">
                <span class="icon">{@html iconLocation}</span> <span class="loc-text">{event.location}</span>
             </button>
          </div>
        {/if}

        {#if extractMeetingLink(event)}
          <div class="read-section">
             <button class="join-btn" onclick={() => openUrl(extractMeetingLink(event) || '')} onkeydown={(e) => e.key === "Enter" && openUrl(extractMeetingLink(event) || '')}>
               <span class="icon">{@html iconVideo}</span> Join Video Call
             </button>
          </div>
        {/if}

        {#if event.description}
          <div class="read-desc section-block">
             <div class="html-content" onclick={handleHtmlClick} onkeydown={(e) => e.key === "Enter" && handleHtmlClick(e)}>
               {@html formatDescription(event.description)}
             </div>
          </div>
        {/if}
      </div>

      <div class="modal-footer">
        <button class="btn-delete" onclick={handleDelete} disabled={isDeleting || isSaving}>
          {isDeleting ? "Deleting..." : "Delete"}
        </button>
        <div class="spacer"></div>
        <button class="btn-cancel" onclick={onClose}>Close</button>
        <button class="btn-save" onclick={() => isEditMode = true}>Edit Event</button>
      </div>
    {/if}
  </div>

<style>
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .modal-content {
    background: var(--bg-view, #ffffff);
    width: 480px;
    border-radius: 12px;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    color: var(--text-primary);
    overflow: hidden;
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }
  .modal-header h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .close-btn {
    background: none;
    border: none;
    font-size: 16px;
    color: var(--text-secondary);
    cursor: pointer;
    line-height: 1;
  }
  .close-btn:hover {
    color: var(--text-primary);
  }
  .modal-body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .title-input {
    width: 100%;
    font-size: 24px;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 4px 0;
    background: transparent;
    color: var(--text-primary);
    outline: none;
  }
  .title-input:focus {
    border-bottom-color: var(--accent-blue);
  }
  .title-input::placeholder {
    color: var(--text-secondary);
  }
  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .form-group label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
  }
  .form-group input,
  .form-group textarea {
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--bg-body);
    color: var(--text-primary);
    font-size: 14px;
    outline: none;
    resize: vertical;
  }
  .form-group input:focus,
  .form-group textarea:focus {
    border-color: var(--accent-blue);
  }
  .form-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .datetime-row {
    display: flex;
    background: var(--bg-body);
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    justify-content: space-between;
  }
  .date-col {
    display: flex;
    gap: 8px;
  }
  .date-col input {
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: 14px;
    outline: none;
  }
  .to-span {
    color: var(--text-secondary);
    font-size: 14px;
    font-weight: 500;
  }
  .toggle-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    cursor: pointer;
  }
  .modal-footer {
    padding: 16px 20px;
    border-top: 1px solid var(--border-color);
    display: flex;
    gap: 8px;
    background: var(--bg-body);
  }
  .spacer {
    flex: 1;
  }
  .modal-footer button {
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    border: none;
  }
  .btn-save {
    background: var(--accent-blue);
    color: white;
  }
  .btn-save:hover {
    opacity: 0.9;
  }
  .btn-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-cancel {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-color) !important;
  }
  .btn-cancel:hover {
    background: var(--sidebar-hover);
  }
  .btn-delete {
    background: rgba(255, 69, 58, 0.1);
    color: #ff453a;
  }
  .btn-delete:hover {
    background: rgba(255, 69, 58, 0.2);
  }

  /* Read View Styles */
  .read-view { gap: 16px; padding: 24px 32px; flex: 1; overflow-y: auto;}
  .read-title { font-size: 24px; font-weight: 600; color: var(--text-primary); margin: 0; line-height: 1.3;}
  .read-time { font-size: 15px; color: var(--accent-blue); font-weight: 500; display: flex; align-items: center; gap: 8px; }
  .read-time .icon { display: flex; align-items: center; margin-top: -1px; }
  
  .read-section { margin-top: 4px; }
  .link-style { background: transparent; border: none; padding: 0; cursor: pointer; text-align: left; transition: color 0.1s, opacity 0.2s; display: flex; align-items: flex-start; gap: 8px; font-family: inherit; font-size: 15px; color: var(--text-secondary); line-height: 1.4;}
  .link-style:hover { color: var(--accent-blue) !important; }
  .link-style:hover .loc-text { text-decoration: underline; }
  .link-style .icon { display: flex; align-items: center; margin-top: 2px; flex-shrink: 0; opacity: 0.8; }
  .loc-text { padding-top: 1px; }
  
  .join-btn { background: var(--bg-view); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: 8px; padding: 8px 14px; font-size: 14px; font-weight: 500; display: inline-flex; align-items: center; gap: 8px; cursor: pointer; transition: background 0.1s, border-color 0.1s; margin-top: 4px;}
  .join-btn:hover { background: var(--sidebar-hover); border-color: var(--accent-blue); }
  .join-btn .icon { display: flex; align-items: center; opacity: 0.8; }

  .section-block { margin-top: 16px; border-top: 1px solid var(--border-color); padding-top: 16px; }
  .read-desc { font-size: 14px; color: var(--text-secondary); line-height: 1.6; }
  .html-content :global(a) { color: var(--accent-blue); text-decoration: none; }
  .html-content :global(a:hover) { text-decoration: underline; }
  .html-content :global(p) { margin-top: 0; margin-bottom: 1em; }
  .html-content :global(br) { display: block; content: ""; margin-top: 4px; }
  .html-content :global(u) { text-decoration: underline; }
  
</style>
