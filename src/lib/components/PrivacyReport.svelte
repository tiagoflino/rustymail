<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";

  interface TrackerSender {
    sender_email: string;
    tracker_count: number;
    tracker_types: string;
  }

  interface PrivacyReportData {
    total_blocked: number;
    unique_senders_tracked: number;
    blocked_this_week: number;
    top_trackers: TrackerSender[];
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
    } catch (e: any) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function formatCount(n: number): string {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
    return n.toString();
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
      <div class="privacy-error">
        <p>No tracking data yet. Trackers are detected and blocked as emails arrive.</p>
      </div>
    {:else if report}
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
          <span class="stat-value">{report.unique_senders_tracked}</span>
          <span class="stat-label">Senders Tracking You</span>
          <span class="stat-desc">Unique senders</span>
        </div>
      </div>

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
  .privacy-error {
    text-align: center;
    padding: 24px;
    color: var(--text-secondary);
    font-size: 13px;
  }
</style>
