<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";

  interface TrackerSender {
    sender_email: string;
    tracker_count: number;
    tracker_types: string;
  }

  interface TrendPoint {
    day: string;
    count: number;
  }

  interface PrivacyReportData {
    total_blocked: number;
    unique_senders_tracked: number;
    blocked_this_week: number;
    top_trackers: TrackerSender[];
    trend: TrendPoint[];
  }

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  let loading = $state(true);
  let error = $state<string | null>(null);
  let report = $state<PrivacyReportData | null>(null);

  onMount(async () => {
    try {
      report = await invoke<PrivacyReportData>("get_privacy_report");
    } catch (e: unknown) {
      const msg = String(e);
      if (msg.includes("No active account found")) {
        report = null;
        error = null;
      } else {
        error = msg;
      }
    } finally {
      loading = false;
    }
  });

  function formatCount(n: number): string {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
    return n.toString();
  }

  let maxTrend = $derived(
    report ? Math.max(1, ...report.trend.map((p) => p.count)) : 1,
  );

  function barHeight(count: number): number {
    return Math.max(4, Math.round((count / maxTrend) * 100));
  }

  function shortDay(day: string): string {
    const parts = day.split("-");
    return parts.length === 3 ? `${parts[1]}/${parts[2]}` : day;
  }

  function trackerTypeLabel(types: string): string {
    const parts = types.split(",");
    const labels: Record<string, string> = {
      tracking_pixel: "Pixels",
      remote_image: "Images",
      read_receipt: "Receipts",
      tracking_link: "Links",
    };
    return parts.map((p) => labels[p.trim()] || p.trim()).join(", ");
  }
</script>

<div class="privacy-overlay" onclick={onclose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="privacy-panel"
    role="dialog"
    aria-label="Privacy Report"
    onclick={(e) => e.stopPropagation()}
    transition:fade={{ duration: 200 }}
  >
    <div class="privacy-header">
      <h2 class="privacy-title">Privacy Report</h2>
      <p class="privacy-subtitle">Tracking protection summary</p>
      <button class="privacy-close" onclick={onclose} aria-label="Close">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    </div>

    {#if loading}
      <div class="privacy-loading">
        <p>Loading report…</p>
      </div>
    {:else if error}
      <div class="privacy-error" role="alert">
        <p class="privacy-error-title">Couldn't load your privacy report</p>
        <p class="privacy-error-detail">{error}</p>
      </div>
    {:else if !report || (report.total_blocked === 0 && report.unique_senders_tracked === 0)}
      <div class="privacy-empty">
        <p>No tracking data yet. Trackers are detected and blocked as emails arrive.</p>
      </div>
    {:else}
      <div class="privacy-stats">
        <div class="stat-card">
          <span class="stat-value">{formatCount(report.total_blocked)}</span>
          <span class="stat-label">Trackers Blocked</span>
          <span class="stat-desc">All time</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{formatCount(report.blocked_this_week)}</span>
          <span class="stat-label">Blocked This Week</span>
          <span class="stat-desc">Last 7 days</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{formatCount(report.unique_senders_tracked)}</span>
          <span class="stat-label">Senders Tracking You</span>
          <span class="stat-desc">Unique senders</span>
        </div>
      </div>

      {#if report.trend.length > 0}
        <div class="privacy-trend">
          <h3 class="section-label">Blocked Over Time</h3>
          <ul class="trend-chart" aria-label="Trackers blocked per day, last 30 days">
            {#each report.trend as point}
              <li
                class="trend-bar-wrap"
                title="{point.day}: {point.count} blocked"
              >
                <span
                  class="trend-bar"
                  style="height: {barHeight(point.count)}%"
                  aria-hidden="true"
                ></span>
                <span class="trend-day-label">{shortDay(point.day)}</span>
                <span class="visually-hidden">{point.count} blocked on {point.day}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if report.top_trackers.length > 0}
        <div class="privacy-trackers">
          <h3 class="section-label">Top Trackers</h3>
          <div class="tracker-list">
            {#each report.top_trackers as tracker}
              <div class="tracker-row">
                <div class="tracker-sender">
                  <span class="tracker-email">{tracker.sender_email}</span>
                  <span class="tracker-types">{trackerTypeLabel(tracker.tracker_types)}</span>
                </div>
                <span class="tracker-count">{tracker.tracker_count} blocked</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="privacy-footer">
        <p class="privacy-note">
          All detection runs locally on your device. No email data is sent to third parties.
        </p>
      </div>
    {/if}
  </div>
</div>

<style>
  .privacy-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 300;
  }
  .privacy-panel {
    background: var(--bg-view);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
    width: 480px;
    max-width: 90vw;
    max-height: 80vh;
    overflow-y: auto;
    padding: 24px;
  }
  :global([data-theme="dark"]) .privacy-panel {
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }
  .privacy-header {
    display: flex;
    flex-direction: column;
    margin-bottom: 24px;
    position: relative;
  }
  .privacy-title {
    font-size: 20px;
    font-weight: 600;
    margin: 0;
    color: var(--text-primary);
  }
  .privacy-subtitle {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 4px 0 0;
  }
  .privacy-close {
    position: absolute;
    top: 0;
    right: 0;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    padding: 4px;
    border-radius: 6px;
  }
  .privacy-close:hover {
    background: var(--sidebar-hover);
  }
  .privacy-stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin-bottom: 24px;
  }
  .stat-card {
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 16px 12px;
    text-align: center;
  }
  .stat-value {
    display: block;
    font-size: 24px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }
  .stat-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-top: 4px;
  }
  .stat-desc {
    display: block;
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 2px;
  }
  .privacy-trackers {
    margin-bottom: 16px;
  }
  .section-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin: 0 0 8px;
  }
  .tracker-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tracker-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--bg-primary);
  }
  .tracker-sender {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .tracker-email {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tracker-types {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .tracker-count {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    flex-shrink: 0;
    margin-left: 12px;
  }
  .privacy-footer {
    text-align: center;
  }
  .privacy-note {
    font-size: 11px;
    color: var(--text-secondary);
    margin: 0;
  }
  .privacy-loading,
  .privacy-empty {
    text-align: center;
    padding: 24px;
    color: var(--text-secondary);
    font-size: 13px;
  }
  .privacy-error {
    text-align: center;
    padding: 24px;
    font-size: 13px;
  }
  .privacy-error-title {
    color: var(--text-primary);
    font-weight: 600;
    margin: 0 0 6px;
  }
  .privacy-error-detail {
    color: var(--text-secondary);
    margin: 0;
    word-break: break-word;
  }
  .privacy-trend {
    margin-bottom: 24px;
  }
  .trend-chart {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 80px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .trend-bar-wrap {
    flex: 1 1 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    height: 100%;
    min-width: 0;
  }
  .trend-bar {
    width: 100%;
    max-width: 14px;
    background: var(--accent-color, #4a7dff);
    border-radius: 2px 2px 0 0;
    display: block;
  }
  .trend-day-label {
    font-size: 9px;
    color: var(--text-secondary);
    margin-top: 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
