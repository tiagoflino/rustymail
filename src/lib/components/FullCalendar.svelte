<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { addToast } from "$lib/stores/toast";
  import EventModal from "./EventModal.svelte";

  let { isMacOS = false }: { isMacOS?: boolean } = $props();

  const iconChevronLeft = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>';
  const iconChevronRight = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>';
  const iconPlus = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>';
  const iconLocation = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z"/><circle cx="12" cy="10" r="3"/></svg>`;
  const iconVideo = `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>`;

  type ViewMode = "day" | "week" | "month" | "year";
  
  let viewType = $state<ViewMode>("month");
  let currentDate = $state(new Date());
  let events = $state<any[]>([]);
  let isLoading = $state(false);

  let showModal = $state(false);
  let editingEvent = $state<any | null>(null);
  let selectedDate = $state<Date | null>(null);

  // Popover State
  let hoveredEvent = $state<any | null>(null);
  let hoverPos = $state({ x: 0, y: 0, transform: "translate(-50%, -100%)" });
  let hoverTimeout: any;

  const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const MONTHS = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
  ];
  const SHORT_MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

  function getDaysInMonthView(date: Date) {
    const year = date.getFullYear();
    const month = date.getMonth();
    const firstDay = new Date(year, month, 1);
    const lastDay = new Date(year, month + 1, 0);
    const start = new Date(firstDay);
    start.setDate(start.getDate() - start.getDay());
    const end = new Date(lastDay);
    if (end.getDay() !== 6) end.setDate(end.getDate() + (6 - end.getDay()));
    const days = [];
    let current = new Date(start);
    while (current <= end) {
      days.push(new Date(current));
      current.setDate(current.getDate() + 1);
    }
    return days;
  }

  function getDaysInMonthStrict(year: number, month: number) {
    const firstDay = new Date(year, month, 1);
    const lastDay = new Date(year, month + 1, 0);
    const start = new Date(firstDay);
    start.setDate(start.getDate() - start.getDay());
    const end = new Date(lastDay);
    if (end.getDay() !== 6) end.setDate(end.getDate() + (6 - end.getDay()));
    const days = [];
    let current = new Date(start);
    while (current <= end) {
      days.push(new Date(current));
      current.setDate(current.getDate() + 1);
    }
    return days;
  }

  let monthDays = $derived(getDaysInMonthView(currentDate));
  
  let weekDays = $derived((() => {
    const start = new Date(currentDate);
    start.setDate(start.getDate() - start.getDay());
    const week = [];
    for(let i=0; i<7; i++) {
        const d = new Date(start);
        d.setDate(d.getDate() + i);
        week.push(d);
    }
    return week;
  })());

  let yearMonths = $derived((() => {
    const y = currentDate.getFullYear();
    const arr = [];
    for(let i=0; i<12; i++) arr.push(new Date(y, i, 1));
    return arr;
  })());

  let displayTitle = $derived((() => {
    if (viewType === 'day') {
      return `${WEEKDAYS[currentDate.getDay()]}, ${MONTHS[currentDate.getMonth()]} ${currentDate.getDate()}, ${currentDate.getFullYear()}`;
    } else if (viewType === 'week') {
      const start = weekDays[0];
      const end = weekDays[6];
      if (start.getMonth() === end.getMonth()) return `${MONTHS[start.getMonth()]} ${start.getFullYear()}`;
      if (start.getFullYear() === end.getFullYear()) return `${MONTHS[start.getMonth()]} – ${MONTHS[end.getMonth()]} ${start.getFullYear()}`;
      return `${MONTHS[start.getMonth()]} ${start.getFullYear()} – ${MONTHS[end.getMonth()]} ${end.getFullYear()}`;
    } else if (viewType === 'year') {
      return `${currentDate.getFullYear()}`;
    }
    return `${MONTHS[currentDate.getMonth()]} ${currentDate.getFullYear()}`;
  })());

  async function loadEvents() {
    isLoading = true;
    try {
      let minInfo: Date;
      let maxInfo: Date;

      if (viewType === 'day') {
        minInfo = new Date(currentDate);
        maxInfo = new Date(currentDate);
      } else if (viewType === 'week') {
        minInfo = weekDays[0];
        maxInfo = weekDays[6];
      } else if (viewType === 'month') {
        minInfo = monthDays[0];
        maxInfo = monthDays[monthDays.length - 1];
      } else {
        minInfo = new Date(currentDate.getFullYear(), 0, 1);
        maxInfo = new Date(currentDate.getFullYear(), 11, 31);
      }

      const timeMin = new Date(minInfo);
      timeMin.setHours(0, 0, 0, 0);
      const timeMax = new Date(maxInfo);
      timeMax.setHours(23, 59, 59, 999);

      events = await invoke("get_events", {
        timeMin: timeMin.toISOString(),
        timeMax: timeMax.toISOString()
      });
    } catch (e) {
      addToast(`Failed to load events: ${e}`, "error");
    } finally {
      isLoading = false;
    }
  }

  onMount(() => loadEvents());

  function handleViewChange() {
    loadEvents();
  }

  function goToPrev() {
    const d = new Date(currentDate);
    if (viewType === 'day') d.setDate(d.getDate() - 1);
    else if (viewType === 'week') d.setDate(d.getDate() - 7);
    else if (viewType === 'month') d.setMonth(d.getMonth() - 1);
    else if (viewType === 'year') d.setFullYear(d.getFullYear() - 1);
    currentDate = d;
    loadEvents();
  }

  function goToNext() {
    const d = new Date(currentDate);
    if (viewType === 'day') d.setDate(d.getDate() + 1);
    else if (viewType === 'week') d.setDate(d.getDate() + 7);
    else if (viewType === 'month') d.setMonth(d.getMonth() + 1);
    else if (viewType === 'year') d.setFullYear(d.getFullYear() + 1);
    currentDate = d;
    loadEvents();
  }

  function goToToday() {
    currentDate = new Date();
    loadEvents();
  }

  const isToday = (d: Date) => {
    const t = new Date();
    return t.getDate() === d.getDate() && t.getMonth() === d.getMonth() && t.getFullYear() === d.getFullYear();
  };

  const isCurrentMonth = (d: Date) => {
    if (viewType === 'month') {
        return d.getMonth() === currentDate.getMonth() && d.getFullYear() === currentDate.getFullYear();
    }
    return true; // Weak concept outside month view
  };

  function getEventsForDay(day: Date) {
    const dayStart = new Date(day);
    dayStart.setHours(0, 0, 0, 0);
    const dayEnd = new Date(day);
    dayEnd.setHours(23, 59, 59, 999);

    let filtered = events.filter(e => {
       const startStr = e.start?.dateTime || e.start?.date;
       const endStr = e.end?.dateTime || e.end?.date;
       if (!startStr) return false;
       
       const eStart = new Date(startStr);
       const eEnd = endStr ? new Date(endStr) : eStart;
       if (!endStr && e.start?.date) eEnd.setHours(23, 59, 59, 999);
       return (eStart <= dayEnd && eEnd >= dayStart);
    });

    // Sort by time within the day list (pure lists for week/day views)
    filtered.sort((a, b) => {
        const as = new Date(a.start?.dateTime || a.start?.date).getTime();
        const bs = new Date(b.start?.dateTime || b.start?.date).getTime();
        return as - bs;
    });
    return filtered;
  }

  function formatEventTime(e: any) {
    if (e.start?.date && !e.start?.dateTime) return "All day";
    const d = new Date(e.start.dateTime);
    return getFormattedTime(d);
  }

  function getFormattedTime(date: Date) {
      let hours = date.getHours();
      let minutes = date.getMinutes();
      const ampm = hours >= 12 ? 'pm' : 'am';
      hours = hours % 12;
      hours = hours ? hours : 12;
      const mStr = minutes < 10 ? '0' + minutes : minutes;
      return hours + ':' + mStr + ampm;
  }

  function handleDayClick(day: Date) {
    if (viewType === 'year') {
        currentDate = day;
        viewType = 'day';
        loadEvents();
        return;
    }
    selectedDate = day;
    editingEvent = null;
    showModal = true;
  }

  function handleEventClick(e: MouseEvent, event: any) {
    e.stopPropagation();
    editingEvent = event;
    selectedDate = null;
    showModal = true;
  }

  function handleModalClose() { showModal = false; }
  function handleModalSave() { showModal = false; loadEvents(); }
  function handleModalDelete() { showModal = false; loadEvents(); }

  // Fancy Location & URL Logic
  function openExternalDirect(e: MouseEvent, url: string) {
    if (e) e.stopPropagation();
    invoke("open_external_url", { url });
  }

  function openUrl(e: MouseEvent, url: string) {
    if (e) e.stopPropagation();
    window.postMessage({ type: 'rustymail-link', url }, '*');
  }

  function getMapUrl(loc: string) {
    if (loc.startsWith("http://") || loc.startsWith("https://")) return loc;
    return `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(loc)}`;
  }

  function extractMeetingLink(ev: any): string | null {
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
    if (/<[a-z][\s\S]*>/i.test(text)) return text;
    const urlRegex = /(https?:\/\/[^\s]+)/g;
    let formatted = text.replace(urlRegex, '<a href="$1">$1</a>');
    return formatted.replace(/\n/g, '<br>');
  }

  function handleHtmlClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    const anchor = target.closest('a');
    if (anchor && anchor.href) {
      e.preventDefault();
      e.stopPropagation();
      window.postMessage({ type: 'rustymail-link', url: anchor.href }, '*');
    }
  }

  // Hover Popover Logic
  function handleMouseEnter(e: MouseEvent, event: any) {
    if (showModal) return;
    clearTimeout(hoverTimeout);
    
    const target = e.currentTarget as HTMLElement;
    
    hoverTimeout = setTimeout(() => {
      const rect = target.getBoundingClientRect();
      const popWidth = 280;
      const popHeightGuess = 200; // estimated max height
      
      let x = rect.left + rect.width / 2;
      let y = rect.top - 10;
      let transform = "translate(-50%, -100%)";
      
      // Vertical bounds (flip below if too close to top)
      if (rect.top < popHeightGuess + 20) {
        y = rect.bottom + 10;
        transform = "translate(-50%, 0)";
      }
      
      // Horizontal bounds (clamp to screen width)
      if (x - popWidth / 2 < 20) {
        x = popWidth / 2 + 20;
      } else if (x + popWidth / 2 > window.innerWidth - 20) {
        x = window.innerWidth - popWidth / 2 - 20;
      }
      
      hoverPos = { x, y, transform };
      hoveredEvent = event;
    }, 400); // Wait 400ms before popping up
  }

  function handleMouseLeave() {
    clearTimeout(hoverTimeout);
    hoverTimeout = setTimeout(() => {
      hoveredEvent = null;
    }, 150);
  }
</script>

<div class="calendar-wrapper">
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
  <!-- Header -->
  <header class="calendar-header" data-tauri-drag-region>
    <div class="header-left">
      <h1 class="month-title">{displayTitle}</h1>
      <div class="nav-controls">
        <button class="nav-btn" onclick={goToPrev}>{@html iconChevronLeft}</button>
        <button class="nav-btn btn-today" onclick={goToToday}>Today</button>
        <button class="nav-btn" onclick={goToNext}>{@html iconChevronRight}</button>
      </div>
      <!-- View Type Selector -->
      <div class="view-selector">
        <div class="segmented-control">
          {#each ['day', 'week', 'month', 'year'] as type}
            <button 
              class="segment {viewType === type ? 'active' : ''}" 
              onclick={() => { viewType = type as ViewMode; handleViewChange(); }}>
              {type.charAt(0).toUpperCase() + type.slice(1)}
            </button>
          {/each}
        </div>
      </div>
      <div class="loading-container">
        {#if isLoading}
          <div class="loading-spinner small"></div>
        {/if}
      </div>
    </div>
    <div class="header-right">
      <button class="btn-new-event" onclick={() => { selectedDate = currentDate; editingEvent = null; showModal = true; }}>
        <span class="icon">{@html iconPlus}</span> New Event
      </button>
    </div>
  </header>

  <!-- Content Grid Container -->
  <div class="calendar-grid">
    
    <!-- MONTH VIEW -->
    {#if viewType === 'month'}
      <div class="weekdays-row">
        {#each WEEKDAYS as wd}<div class="weekday-cell">{wd}</div>{/each}
      </div>
      <div class="days-matrix">
        {#each monthDays as day}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="day-cell {isCurrentMonth(day) ? '' : 'outside-month'} {isToday(day) ? 'today' : ''}" onclick={() => handleDayClick(day)}>
            <div class="day-number"><span>{day.getDate()}</span></div>
            <div class="events-container">
              {#each getEventsForDay(day) as ev}
                <button class="event-chip" onclick={(e) => handleEventClick(e, ev)} onmouseenter={(e) => handleMouseEnter(e, ev)} onmouseleave={handleMouseLeave}>
                    <div class="chip-dot {ev.start?.date && !ev.start?.dateTime ? 'all-day' : ''}"></div>
                    <span class="event-time">{formatEventTime(ev)}</span>
                    <span class="event-title">{ev.summary}</span>
                    <!-- Show small video icon indicator on week/month chips if meeting exists -->
                    {#if extractMeetingLink(ev)}<span class="video-indicator">{@html iconVideo}</span>{/if}
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    
    <!-- WEEK VIEW -->
    {:else if viewType === 'week'}
      <div class="weekdays-row week-view-header">
        {#each weekDays as day, i}
          <div class="weekday-cell {isToday(day) ? 'today-text' : ''}">
             <div class="week-name">{WEEKDAYS[i]}</div>
             <div class="week-date {isToday(day) ? 'today-badge' : ''}">{day.getDate()}</div>
          </div>
        {/each}
      </div>
      <div class="days-matrix list-layout">
        {#each weekDays as day}
          <div class="day-cell list-cell {isToday(day) ? 'today-bg' : ''}" onclick={() => handleDayClick(day)} onkeydown={(e) => e.key === "Enter" && handleDayClick(day)} tabindex="0" role="button">
            <div class="events-container pad">
              {#each getEventsForDay(day) as ev}
                <button class="event-chip list-chip" onclick={(e) => handleEventClick(e, ev)} onmouseenter={(e) => handleMouseEnter(e, ev)} onmouseleave={handleMouseLeave}>
                    <div class="chip-dot {ev.start?.date && !ev.start?.dateTime ? 'all-day' : ''}"></div>
                    <div class="event-info-col">
                        <span class="event-title">{ev.summary}</span>
                        <span class="event-time">{formatEventTime(ev)}</span>
                    </div>
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    
    <!-- DAY VIEW -->
    {:else if viewType === 'day'}
      <div class="day-view-container" onclick={() => handleDayClick(currentDate)} onkeydown={(e) => e.key === "Enter" && handleDayClick(currentDate)} role="button" tabindex="0">
         <div class="day-view-header {isToday(currentDate) ? 'today-text' : ''}">
            <span class="day-view-number {isToday(currentDate) ? 'today-badge' : ''}">{currentDate.getDate()}</span>
            <span class="day-view-name">{WEEKDAYS[currentDate.getDay()]}</span>
         </div>
         <div class="day-view-body">
            <!-- pure list rendering -->
            <div class="day-events-list">
              {#each getEventsForDay(currentDate) as ev}
                <button class="event-card" onclick={(e: MouseEvent) => handleEventClick(e, ev)} onkeydown={(e: KeyboardEvent) => { if (e.key === "Enter") handleEventClick(new MouseEvent('click'), ev); }}>
                    <div class="event-card-stripe"></div>
                    <div class="event-card-content">
                        <span class="event-card-title">{ev.summary}</span>
                        <span class="event-card-time">{formatEventTime(ev)}</span>
                        {#if ev.location}
                          <a href={getMapUrl(ev.location)} target="_blank" rel="noopener noreferrer" title="Open map/link">
                            <span class="icon">{@html iconLocation}</span> <span class="loc-text">{ev.location}</span>
                          </a>
                        {/if}
                        {#if extractMeetingLink(ev)}
                           <a href={extractMeetingLink(ev) || ''} target="_blank" rel="noopener noreferrer">
                             <span class="icon">{@html iconVideo}</span> Join Meeting
                           </a>
                        {/if}
                        {#if ev.description}
                          <!-- svelte-ignore a11y_no_static_element_interactions -->
                          <div class="event-card-desc html-content" onclick={handleHtmlClick} onkeydown={(e: KeyboardEvent) => e.key === "Enter" && handleHtmlClick(e as any)}>{@html formatDescription(ev.description)}</div>
                        {/if}
                    </div>
                </button>
              {:else}
                 <div class="no-events">No events scheduled for this day</div>
              {/each}
            </div>
         </div>
         <!-- svelte-ignore a11y_click_events_have_key_events -->
         <!-- svelte-ignore a11y_no_static_element_interactions -->
         <div class="day-view-backdrop"></div>
      </div>

    <!-- YEAR VIEW -->
    {:else if viewType === 'year'}
      <div class="year-grid">
        {#each yearMonths as yMonth, i}
          <div class="mini-month">
            <h3 class="mini-month-title">{MONTHS[i]}</h3>
            <div class="mini-weekdays">
              {#each ["S","M","T","W","T","F","S"] as wd}<span>{wd}</span>{/each}
            </div>
            <div class="mini-days">
              {#each getDaysInMonthStrict(yMonth.getFullYear(), i) as day}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div class="mini-day-wrap {day.getMonth() !== i ? 'outside' : ''}">
                      <div class="mini-day {isToday(day) ? 'today-badge' : ''}" onclick={() => handleDayClick(day)}>
                         {day.getDate()}
                         {#if day.getMonth() === i && getEventsForDay(day).length > 0}
                            <div class="mini-dot"></div>
                         {/if}
                      </div>
                  </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if hoveredEvent && !showModal}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_mouse_events_have_key_events -->
  <div class="event-popover" 
       style="left: {hoverPos.x}px; top: {hoverPos.y}px; transform: {hoverPos.transform};"
       onmouseenter={() => clearTimeout(hoverTimeout)}
       onmouseleave={handleMouseLeave}>
    <h3 class="popover-title">{hoveredEvent.summary}</h3>
    <div class="popover-time">{formatEventTime(hoveredEvent)}</div>
    {#if hoveredEvent.location}
      <div class="popover-location">
        <button class="location-btn link-style" onclick={(e) => openExternalDirect(e, getMapUrl(hoveredEvent.location))} title="Open Map">
           <span class="icon">{@html iconLocation}</span> <span class="loc-text">{hoveredEvent.location}</span>
        </button>
      </div>
    {/if}
    {#if extractMeetingLink(hoveredEvent)}
       <button class="join-btn popover-join" onclick={(e) => openUrl(e, extractMeetingLink(hoveredEvent) || '')}>
         <span class="icon">{@html iconVideo}</span> Join Video Call
       </button>
    {/if}
    {#if hoveredEvent.description}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="popover-desc html-content" onclick={handleHtmlClick}>
        {@html formatDescription(hoveredEvent.description)}
      </div>
    {/if}
  </div>
{/if}

{#if showModal}
  <EventModal event={editingEvent} {selectedDate} onClose={handleModalClose} onSave={handleModalSave} onDelete={handleModalDelete} />
{/if}


<style>
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
  .calendar-wrapper { display: flex; flex-direction: column; height: 100%; width: 100%; background: var(--bg-view); color: var(--text-primary); }
  .calendar-header { 
    display: flex; 
    justify-content: space-between; 
    align-items: center; 
    padding: 0 20px;
    height: 50px; 
    border-bottom: 1px solid var(--border-color); 
    background: var(--bg-view);
    flex-shrink: 0;
  }
  .header-left { display: flex; align-items: center; gap: 16px; }
  .month-title { font-size: var(--font-size-heading); font-weight: 600; margin: 0; min-width: 160px; white-space: nowrap; }
  .loading-container { width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
  
  .nav-controls { display: flex; align-items: center; background: var(--bg-control); border: 1px solid var(--border-color); border-radius: 8px; overflow: hidden; }
  .nav-btn { background: transparent; border: none; border-right: 1px solid var(--border-color); padding: 6px 10px; color: var(--text-primary); cursor: pointer; font-size: var(--font-size-base); font-weight: 500; display: flex; align-items: center; transition: background 0.1s; }
  .nav-btn:last-child { border-right: none; }
  .nav-btn:hover { background: var(--sidebar-hover); }
  .btn-today { padding: 6px 12px; }
  
  .segmented-control { display: flex; background: var(--bg-control); border: none; border-radius: 8px; padding: 2px; }
  .segment { background: transparent; border: none; border-radius: var(--radius-standard); padding: 5px 12px; font-size: var(--font-size-base); font-weight: 500; color: var(--text-secondary); cursor: pointer; transition: all 0.2s; }
  .segment:hover { color: var(--text-primary); }
  .segment.active { background: var(--bg-view); color: var(--text-primary); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12), 0 0 0 0.5px rgba(0, 0, 0, 0.04); }
  
  .header-right { display: flex; align-items: center; }
  .btn-new-event { 
    display: flex; 
    align-items: center; 
    gap: 6px; 
    background: var(--accent-blue); 
    color: white; 
    border: none; 
    padding: 6px 12px;
    border-radius: var(--radius-standard);
    font-weight: 500;
    font-size: var(--font-size-toolbar); 
    cursor: pointer; 
    box-shadow: 0 2px 6px rgba(10, 132, 255, 0.2); 
    transition: opacity 0.1s; 
    white-space: nowrap;
  }
  .btn-new-event:hover { opacity: 0.9; }
  .btn-new-event .icon { display: flex; align-items: center; }

  .calendar-grid { display: flex; flex-direction: column; flex: 1; overflow: hidden; position: relative; }
  
  /* Month/Week common */
  .weekdays-row { display: grid; grid-template-columns: repeat(7, 1fr); border-bottom: 1px solid var(--border-color); }
  .weekday-cell { padding: 12px; text-align: center; font-size: var(--font-size-toolbar); font-weight: 600; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.5px; }
  .days-matrix { display: grid; grid-template-columns: repeat(7, 1fr); grid-auto-rows: 1fr; flex: 1; overflow-y: auto; }
  .day-cell { border-right: 1px solid var(--border-color); border-bottom: 1px solid var(--border-color); padding: 8px; min-height: 120px; display: flex; flex-direction: column; cursor: pointer; transition: background 0.1s; min-width: 0; }
  .day-cell:hover { background: var(--sidebar-hover); }
  .day-cell:nth-child(7n) { border-right: none; }
  .outside-month .day-number { color: var(--text-secondary); opacity: 0.5; }
  .day-number { font-size: 14px; font-weight: 500; margin-bottom: 8px; display: flex; justify-content: flex-end; }
  .day-number span { width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; border-radius: 50%; }
  .today .day-number span { background: var(--accent-blue); color: white; }
  .today { background: rgba(10, 132, 255, 0.03); }

  /* Week specifics */
  .week-view-header .weekday-cell { display: flex; flex-direction: column; gap: 4px; padding: 16px 8px; align-items: center; }
  .week-name { font-size: var(--font-size-small); text-transform: uppercase; color: var(--text-secondary); }
  .week-date { font-size: 22px; font-weight: 400; width: 36px; height: 36px; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: var(--text-primary); }
  .today-text .week-name { color: var(--accent-blue); font-weight: 600;}
  .today-badge { background: var(--accent-blue) !important; color: white !important; font-weight: 500 !important; }
  .today-bg { background: rgba(10, 132, 255, 0.03); }
  
  /* Month/Week Chip */
  .events-container { display: flex; flex-direction: column; gap: 4px; overflow-y: auto; flex: 1; scrollbar-width: none; }
  .events-container::-webkit-scrollbar { display: none; }
  .events-container.pad { padding: 4px 0; gap: 6px; }
  
  .event-chip { display: flex; align-items: center; gap: 6px; padding: 4px 6px; border-radius: 4px; background: var(--bg-control); border: 1px solid var(--border-color); font-size: var(--font-size-small); text-align: left; cursor: pointer; width: 100%; color: var(--text-primary); transition: border-color 0.1s, box-shadow 0.1s; }
  .event-chip:hover { border-color: var(--accent-blue); box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05); }
  .list-chip { align-items: flex-start; padding: 8px; border-radius: var(--radius-standard); }
  .event-info-col { display: flex; flex-direction: column; gap: 2px; overflow: hidden; }
  .event-info-col .event-title { font-size: var(--font-size-toolbar); font-weight: 500; }
  .event-info-col .event-time { font-size: var(--font-size-small); opacity: 0.8; }
  .video-indicator { display: flex; align-items: center; color: var(--text-secondary); opacity: 0.7; }
  
  .chip-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent-blue); flex-shrink: 0; }
  .chip-dot.all-day { border-radius: 2px; }
  .event-time { font-weight: 500; color: var(--text-secondary); flex-shrink: 0; }
  .event-title { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* Day View strictly ordered list */
  .day-view-container { display: flex; flex-direction: column; flex: 1; overflow-y: auto; position: relative; }
  .day-view-header { padding: 24px 32px; border-bottom: 1px solid var(--border-color); display: flex; align-items:baseline; gap: 12px; z-index: 10; position: sticky; top: 0; background: var(--bg-view); }
  .day-view-number { font-size: 32px; font-weight: 400; border-radius: 50%; width: 48px; height: 48px; display: flex; justify-content: center; align-items: center; }
  .day-view-name { font-size: var(--font-size-heading); color: var(--text-secondary); }
  .day-view-body { padding: 24px 32px; max-width: 800px; z-index: 2; position: relative; }
  .day-view-backdrop { position: absolute; top:0; left:0; width:100%; height:100%; z-index: 1; cursor: pointer; }
  
  .day-events-list { display: flex; flex-direction: column; gap: 12px; }
  .no-events { color: var(--text-secondary); font-style: italic; padding: 20px 0; }
  .event-card { display: flex; background: var(--bg-control); border: 1px solid var(--border-color); border-radius: var(--radius-modal); overflow: hidden; text-align: left; cursor: pointer; transition: transform 0.1s, box-shadow 0.1s; width: 100%; box-shadow: 0 4px 12px rgba(0,0,0,0.03); }
  .event-card:hover { transform: translateY(-2px); box-shadow: 0 8px 24px rgba(0,0,0,0.08); border-color: var(--accent-blue); }
  .event-card-stripe { width: 6px; background: var(--accent-blue); flex-shrink: 0; }
  .event-card-content { display: flex; flex-direction: column; gap: 6px; padding: 16px 20px; flex: 1; overflow: hidden; }
  .event-card-title { font-size: var(--font-size-title); font-weight: 600; color: var(--text-primary); }
  .event-card-time { font-size: var(--font-size-base); color: var(--accent-blue); font-weight: 500; }

  .link-style { background: transparent; border: none; padding: 0; cursor: pointer; text-align: left; transition: opacity 0.2s, color 0.1s; display: flex; align-items: flex-start; gap: 6px; font-family: inherit;}
  .link-style:hover { color: var(--accent-blue) !important; }
  .link-style:hover .loc-text { text-decoration: underline; }
  .link-style .icon { display: flex; align-items: center; margin-top: 1px; flex-shrink: 0; opacity: 0.8;}
  .loc-text { padding-top: 1px; }

  
  .join-btn { background: var(--bg-view); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: var(--radius-standard); padding: 6px 10px; font-size: var(--font-size-toolbar); font-weight: 500; display: inline-flex; align-items: center; gap: 6px; cursor: pointer; transition: background 0.1s, border-color 0.1s; align-self: flex-start; margin-top: 4px;}
  .join-btn:hover { background: var(--sidebar-hover); border-color: var(--accent-blue); }
  .join-btn .icon { display: flex; align-items: center; opacity: 0.8; }
  .popover-join { margin-top: 6px; font-size: 11px; padding: 4px 8px;}

  /* Year View */
  .year-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); padding: 32px; gap: 48px 32px; overflow-y: auto; flex: 1;}
  .mini-month { display: flex; flex-direction: column; }
  .mini-month-title { font-size: var(--font-size-detail); margin: 0 0 12px 6px; color: var(--text-primary); font-weight: 600; }
  .mini-weekdays { display: grid; grid-template-columns: repeat(7, 1fr); margin-bottom: 8px; }
  .mini-weekdays span { font-size: var(--font-size-small); font-weight: 600; color: var(--text-secondary); text-align: center; }
  .mini-days { display: grid; grid-template-columns: repeat(7, 1fr); grid-auto-rows: 1fr; }
  .mini-day-wrap { aspect-ratio: 1; display: flex; padding: 2px; }
  .outside { visibility: hidden; }
  .mini-day { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; font-size: var(--font-size-toolbar); border-radius: 50%; color: var(--text-primary); cursor: pointer; transition: background 0.1s; position: relative;}
  .mini-day:hover { background: var(--sidebar-hover); }
  .mini-dot { width: 4px; height: 4px; background: var(--accent-blue); border-radius: 50%; position: absolute; bottom: 2px; }

  .event-popover {
    position: fixed;
    background: var(--bg-view);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-modal);
    padding: 16px;
    width: 280px;
    box-shadow: 0 12px 32px rgba(0,0,0,0.15);
    z-index: 1000;
    pointer-events: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text-primary);
  }
  .popover-title { font-size: var(--font-size-detail); font-weight: 600; margin: 0; line-height: 1.3; }
  .popover-time { font-size: var(--font-size-toolbar); font-weight: 500; color: var(--accent-blue); }
  .popover-location { font-size: var(--font-size-toolbar); color: var(--text-secondary); margin-top: 4px; }
  .location-btn { color: var(--text-secondary); }
  .popover-desc { font-size: var(--font-size-toolbar); color: var(--text-secondary); line-height: 1.4; margin-top: 8px; max-height: 120px; overflow-y: auto; background: rgba(0,0,0,0.02); padding: 8px; border-radius: var(--radius-standard); }
  .popover-desc :global(a) { color: var(--accent-blue); text-decoration: none; }
  .popover-desc :global(a:hover) { text-decoration: underline; }

  .event-card-desc { font-size: var(--font-size-base); color: var(--text-secondary); margin-top: 4px; line-height: 1.4; }
  .event-card-desc :global(a) { color: var(--accent-blue); text-decoration: none; }
  .event-card-desc :global(a:hover) { text-decoration: underline; }

  .loading-spinner.small { width: 20px; height: 20px; border-width: 2px; }
</style>
