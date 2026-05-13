import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { selectedThreadId } from './messages';
import { addToast } from './toast';

export interface AISummaryState {
  summary: string | null;
  isLoading: boolean;
  statusMessage: string | null;
  isOpen: boolean;
  threadId: string | null;
  aiAvailable: boolean;
}

export const aiSummaryState = writable<AISummaryState>({
  summary: null,
  isLoading: false,
  statusMessage: null,
  isOpen: false,
  threadId: null,
  aiAvailable: false,
});

async function pollAiStatus(): Promise<void> {
  try {
    const status: any = await invoke('get_ai_status');
    const state = get(aiSummaryState);
    
    if (status === 'NotSetUp') {
      aiSummaryState.update(s => ({ ...s, statusMessage: 'Preparing AI engine...' }));
    } else if (status && typeof status === 'object' && 'Downloading' in status) {
      const pct = Math.round((status.Downloading as any).progress_pct);
      aiSummaryState.update(s => ({ ...s, statusMessage: `Downloading AI model... ${pct}%` }));
    } else if (status === 'Loading') {
      aiSummaryState.update(s => ({ ...s, statusMessage: 'Loading AI model...' }));
    } else if (status === 'Ready') {
      aiSummaryState.update(s => ({ ...s, statusMessage: 'Generating summary...' }));
    }
  } catch {
    // Ignore polling errors
  }
}

export async function generateSummary(threadId: string): Promise<void> {
  if (!threadId || get(aiSummaryState).isLoading) return;

  aiSummaryState.update(s => ({
    ...s,
    isLoading: true,
    summary: null,
    statusMessage: 'Preparing AI...',
    isOpen: true,
  }));

  let pollInterval: ReturnType<typeof setInterval> | null = null;
  pollInterval = setInterval(pollAiStatus, 500);

  try {
    await invoke('ensure_ai_ready');
    aiSummaryState.update(s => ({ ...s, statusMessage: 'Generating summary...' }));
    
    const result = await invoke('summarize_thread', { threadId });
    const summary = result as string;

    aiSummaryState.update(s => ({
      ...s,
      summary,
      isLoading: false,
      statusMessage: null,
      threadId,
    }));
  } catch (e: any) {
    addToast(`AI summarization failed: ${e}`, 'error', 5000);
    aiSummaryState.update(s => ({
      ...s,
      isLoading: false,
      statusMessage: null,
    }));
  } finally {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }
}

export function resetSummary(): void {
  aiSummaryState.update(s => ({
    ...s,
    summary: null,
    isLoading: false,
    statusMessage: null,
    isOpen: false,
    threadId: null,
  }));
}

export function togglePanel(): void {
  const state = get(aiSummaryState);
  if (state.isOpen) {
    aiSummaryState.update(s => ({ ...s, isOpen: false }));
  } else {
    aiSummaryState.update(s => ({ ...s, isOpen: true }));
  }
}

export function setAiAvailable(available: boolean): void {
  aiSummaryState.update(s => ({ ...s, aiAvailable: available }));
}

// Reset summary when thread changes
selectedThreadId.subscribe(threadId => {
  const state = get(aiSummaryState);
  if (state.threadId && state.threadId !== threadId) {
    aiSummaryState.update(s => ({
      ...s,
      summary: null,
      isLoading: false,
      statusMessage: null,
      isOpen: false,
      threadId,
    }));
  }
});
