<script lang="ts">
  interface Props {
    onsnooze: (until: number) => void;
    onclose: () => void;
  }

  let { onsnooze, onclose }: Props = $props();

  function laterToday(): number {
    const now = new Date();
    // If after 6 PM, roll to tomorrow 9 AM
    if (now.getHours() >= 18) {
      const tomorrow = new Date(now);
      tomorrow.setDate(tomorrow.getDate() + 1);
      tomorrow.setHours(9, 0, 0, 0);
      return Math.floor(tomorrow.getTime() / 1000);
    }
    // Otherwise +3 hours
    return Math.floor((now.getTime() + 3 * 60 * 60 * 1000) / 1000);
  }

  function tomorrowMorning(): number {
    const now = new Date();
    const tomorrow = new Date(now);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(9, 0, 0, 0);
    return Math.floor(tomorrow.getTime() / 1000);
  }

  function nextWeek(): number {
    const now = new Date();
    const daysUntilMonday = (8 - now.getDay()) % 7 || 7;
    const monday = new Date(now);
    monday.setDate(monday.getDate() + daysUntilMonday);
    monday.setHours(9, 0, 0, 0);
    return Math.floor(monday.getTime() / 1000);
  }

  function formatPreview(timestamp: number): string {
    const date = new Date(timestamp * 1000);
    return date.toLocaleString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  const options = [
    { label: "Later Today", icon: "☀️", compute: laterToday },
    { label: "Tomorrow Morning", icon: "🌅", compute: tomorrowMorning },
    { label: "Next Week", icon: "📅", compute: nextWeek },
  ];

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
    }
  }

  function handleSelect(compute: () => number) {
    onsnooze(compute());
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="snooze-popover" onkeydown={handleKeydown}>
  <div class="snooze-header">Snooze until...</div>
  {#each options as opt}
    <button
      class="snooze-option"
      onclick={() => handleSelect(opt.compute)}
    >
      <span class="snooze-icon">{opt.icon}</span>
      <div class="snooze-text">
        <span class="snooze-label">{opt.label}</span>
        <span class="snooze-preview">{formatPreview(opt.compute())}</span>
      </div>
    </button>
  {/each}
</div>

<style>
  .snooze-popover {
    background: var(--bg-primary, #1a1a2e);
    border: 1px solid var(--border-color, #2a2a4a);
    border-radius: 8px;
    padding: 4px;
    min-width: 240px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    z-index: 100;
  }
  .snooze-header {
    padding: 8px 12px 4px;
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .snooze-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: none;
    cursor: pointer;
    border-radius: 6px;
    color: var(--text-primary, #e0e0e0);
    text-align: left;
  }
  .snooze-option:hover {
    background: var(--bg-hover, rgba(255, 255, 255, 0.08));
  }
  .snooze-icon {
    font-size: 1.2rem;
  }
  .snooze-text {
    display: flex;
    flex-direction: column;
  }
  .snooze-label {
    font-size: 0.875rem;
    font-weight: 500;
  }
  .snooze-preview {
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
  }
</style>
