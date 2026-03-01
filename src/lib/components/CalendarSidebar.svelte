<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';

  interface CalendarDateTime {
    date?: string;
    dateTime?: string;
    timeZone?: string;
  }

  interface CalendarEvent {
    id: string;
    summary?: string;
    description?: string;
    location?: string;
    start?: CalendarDateTime;
    end?: CalendarDateTime;
    htmlLink?: string;
  }

  let { onClose }: { onClose: () => void } = $props();
  let events = $state<CalendarEvent[]>([]);
  let isLoading = $state(true);
  let errorMsg = $state('');

  onMount(async () => {
    try {
      events = await invoke('get_upcoming_events');
    } catch (e) {
      errorMsg = String(e);
    } finally {
      isLoading = false;
    }
  });

  function formatEventTime(dt?: CalendarDateTime) {
    if (!dt) return '';
    if (dt.date) return 'All Day';
    if (dt.dateTime) {
      const d = new Date(dt.dateTime);
      return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
    }
    return '';
  }

  function formatEventDate(dt?: CalendarDateTime) {
    if (!dt) return '';
    const dateStr = dt.date || dt.dateTime;
    if (!dateStr) return '';
    const d = new Date(dateStr);
    return d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
  }

  function groupEventsByDate(eventsList: CalendarEvent[]) {
    const groups: Record<string, CalendarEvent[]> = {};
    for (const e of eventsList) {
      const dateKey = formatEventDate(e.start);
      if (!groups[dateKey]) groups[dateKey] = [];
      groups[dateKey].push(e);
    }
    return Object.entries(groups);
  }

  const iconClose = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`;
  const iconCalendar = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>`;
  const iconLink = `<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6M15 3h6v6M10 14L21 3"/></svg>`;
</script>

<aside class="calendar-sidebar" transition:fly={{ x: 300, duration: 250 }}>
  <header class="cal-header">
    <div class="cal-title-wrapper">
      <span class="cal-icon">{@html iconCalendar}</span>
      <span class="cal-title">Upcoming</span>
    </div>
    <button class="close-btn" onclick={onClose}>{@html iconClose}</button>
  </header>

  <div class="cal-content">
    {#if isLoading}
      <div class="cal-loading">
        <div class="spinner"></div>
      </div>
    {:else if errorMsg}
      <div class="cal-error">
        <p>Could not load events.</p>
        <span class="error-detail">{errorMsg}</span>
        <button class="retry-btn" onclick={() => { isLoading = true; errorMsg = ''; invoke('get_upcoming_events').then(res => events = res as any).catch(e => errorMsg = String(e)).finally(() => isLoading = false); }}>Retry</button>
      </div>
    {:else if events.length === 0}
      <div class="cal-empty">
        <div class="empty-icon">{@html iconCalendar}</div>
        <p>No upcoming events.</p>
      </div>
    {:else}
      <div class="event-list">
        {#each groupEventsByDate(events) as [dateLabel, dayEvents]}
          <div class="day-group">
            <h3 class="day-header">{dateLabel}</h3>
            {#each dayEvents as ev}
              <div class="event-card">
                <div class="event-time">{formatEventTime(ev.start)}</div>
                <div class="event-details">
                  <div class="event-summary">{ev.summary || 'Busy'}</div>
                  {#if ev.htmlLink}
                    <a href={ev.htmlLink} target="_blank" rel="noopener noreferrer" class="event-link" title="Open in Google Calendar">{@html iconLink}</a>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</aside>

<style>
  .calendar-sidebar {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 300px;
    background: var(--bg-sidebar);
    border-left: 1px solid var(--border-color);
    box-shadow: -4px 0 20px rgba(0,0,0,0.05);
    display: flex;
    flex-direction: column;
    z-index: 50;
    font-family: var(--font-family);
  }

  .cal-header {
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    border-bottom: 1px solid var(--border-color);
  }

  .cal-title-wrapper {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--accent-blue);
  }
  .cal-title {
    font-weight: 600;
    font-size: 14px;
    color: var(--text-primary);
  }

  .close-btn {
    background: transparent;
    border: none;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--text-secondary);
    transition: all 0.15s;
  }
  .close-btn:hover {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  .cal-content {
    flex: 1;
    overflow-y: auto;
  }

  .cal-loading, .cal-error, .cal-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 24px;
    text-align: center;
    color: var(--text-secondary);
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid var(--border-color);
    border-top-color: var(--accent-blue);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { 100% { transform: rotate(360deg); } }

  .cal-error .error-detail {
    font-size: 11px;
    color: #ff3b30;
    margin-top: 8px;
    opacity: 0.8;
  }
  .retry-btn {
    margin-top: 16px;
    padding: 4px 12px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 12px;
  }
  .retry-btn:hover { background: var(--sidebar-hover); }

  .empty-icon {
    font-size: 32px;
    opacity: 0.3;
    margin-bottom: 12px;
  }
  
  .event-list {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .day-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .day-header {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    color: var(--text-secondary);
    font-weight: 600;
    margin: 4px 0 0 4px;
  }

  .event-card {
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    box-shadow: 0 1px 3px rgba(0,0,0,0.02);
    border-left: 3px solid var(--accent-blue);
    transition: transform 0.1s ease;
  }
  
  .event-card:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0,0,0,0.05);
  }

  .event-time {
    font-size: 11px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .event-details {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }

  .event-summary {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    line-height: 1.4;
  }

  .event-link {
    color: var(--text-secondary);
    opacity: 0;
    transition: opacity 0.2s;
    display: flex;
    align-items: center;
  }
  .event-card:hover .event-link {
    opacity: 1;
  }
  .event-link:hover {
    color: var(--accent-blue);
  }
</style>
