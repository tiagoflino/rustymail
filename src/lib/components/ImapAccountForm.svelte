<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { addToast } from "$lib/stores/toast";

  interface Props {
    onSuccess: () => void;
    onCancel: () => void;
  }

  let { onSuccess, onCancel }: Props = $props();

  let email = $state('');
  let displayName = $state('');
  let password = $state('');
  let imapHost = $state('');
  let imapPort = $state(993);
  let smtpHost = $state('');
  let smtpPort = $state(587);
  let useTls = $state(true);
  let testResult = $state('');
  let testing = $state(false);
  let adding = $state(false);

  const PRESETS: Record<string, {imap_host: string; imap_port: number; smtp_host: string; smtp_port: number}> = {
    'outlook.com': { imap_host: 'outlook.office365.com', imap_port: 993, smtp_host: 'smtp.office365.com', smtp_port: 587 },
    'hotmail.com': { imap_host: 'outlook.office365.com', imap_port: 993, smtp_host: 'smtp.office365.com', smtp_port: 587 },
    'live.com': { imap_host: 'outlook.office365.com', imap_port: 993, smtp_host: 'smtp.office365.com', smtp_port: 587 },
    'yahoo.com': { imap_host: 'imap.mail.yahoo.com', imap_port: 993, smtp_host: 'smtp.mail.yahoo.com', smtp_port: 587 },
    'fastmail.com': { imap_host: 'imap.fastmail.com', imap_port: 993, smtp_host: 'smtp.fastmail.com', smtp_port: 587 },
    'icloud.com': { imap_host: 'imap.mail.me.com', imap_port: 993, smtp_host: 'smtp.mail.me.com', smtp_port: 587 },
    'me.com': { imap_host: 'imap.mail.me.com', imap_port: 993, smtp_host: 'smtp.mail.me.com', smtp_port: 587 },
    'mac.com': { imap_host: 'imap.mail.me.com', imap_port: 993, smtp_host: 'smtp.mail.me.com', smtp_port: 587 },
    'aol.com': { imap_host: 'imap.aol.com', imap_port: 993, smtp_host: 'smtp.aol.com', smtp_port: 587 },
    'zoho.com': { imap_host: 'imap.zoho.com', imap_port: 993, smtp_host: 'smtp.zoho.com', smtp_port: 587 },
    'protonmail.com': { imap_host: '127.0.0.1', imap_port: 1143, smtp_host: '127.0.0.1', smtp_port: 1025 },
    'pm.me': { imap_host: '127.0.0.1', imap_port: 1143, smtp_host: '127.0.0.1', smtp_port: 1025 },
  };

  async function applyPreset() {
    const domain = email.split('@')[1]?.toLowerCase();
    if (!domain) return;

    if (PRESETS[domain]) {
      const preset = PRESETS[domain];
      imapHost = preset.imap_host;
      imapPort = preset.imap_port;
      smtpHost = preset.smtp_host;
      smtpPort = preset.smtp_port;
      return;
    }

    try {
      const discovered: any = await invoke('autodiscover_imap', { email });
      if (discovered) {
        imapHost = discovered.imap_host;
        imapPort = discovered.imap_port;
        smtpHost = discovered.smtp_host;
        smtpPort = discovered.smtp_port;
        useTls = discovered.use_tls;
      }
    } catch {
    }
  }

  async function testConnection() {
    testing = true;
    testResult = '';
    try {
      await invoke('test_imap_connection', { host: imapHost, port: imapPort, username: email, password });
      await invoke('test_smtp_connection', { host: smtpHost, port: smtpPort, username: email, password });
      testResult = 'Connection successful';
    } catch (e: any) {
      testResult = `Failed: ${e}`;
    }
    testing = false;
  }

  async function addAccount() {
    adding = true;
    try {
      await invoke('add_imap_account', {
        email,
        displayName: displayName || email.split('@')[0],
        password,
        imapHost,
        imapPort,
        smtpHost,
        smtpPort,
        useTls,
      });
      onSuccess();
    } catch (e: any) {
      testResult = `Failed to add account: ${e}`;
    }
    adding = false;
  }
</script>

<div class="imap-form">
  <button class="btn-back" onclick={onCancel}>
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
    Back
  </button>

  <div class="imap-fields">
    <input type="email" class="credential-input" placeholder="Email address" bind:value={email} onblur={applyPreset} />
    <input type="text" class="credential-input" placeholder="Display name (optional)" bind:value={displayName} />
    <input type="password" class="credential-input" placeholder="Password or app password" bind:value={password} />
    <div class="field-row">
      <input type="text" class="credential-input" placeholder="IMAP host" bind:value={imapHost} style="flex: 2;" />
      <input type="number" class="credential-input" placeholder="Port" bind:value={imapPort} style="flex: 1; max-width: 80px;" />
    </div>
    <div class="field-row">
      <input type="text" class="credential-input" placeholder="SMTP host" bind:value={smtpHost} style="flex: 2;" />
      <input type="number" class="credential-input" placeholder="Port" bind:value={smtpPort} style="flex: 1; max-width: 80px;" />
    </div>
    <div class="tls-row">
      <label class="tls-label">
        <input type="checkbox" bind:checked={useTls} /> Use TLS
      </label>
    </div>
    {#if testResult}
      <div class="imap-test-result" class:error={testResult.startsWith('Failed')} class:success={!testResult.startsWith('Failed')}>
        {testResult}
      </div>
    {/if}
    <div class="action-row">
      <button
        class="btn-test"
        disabled={testing || !email || !password || !imapHost || !smtpHost}
        onclick={testConnection}
      >{testing ? 'Testing...' : 'Test Connection'}</button>
      <button
        class="btn-add"
        disabled={adding || !email || !password || !imapHost || !smtpHost}
        onclick={addAccount}
      >{adding ? 'Adding...' : 'Add Account'}</button>
    </div>
  </div>
</div>

<style>
  .imap-form {
    width: 100%;
  }
  .btn-back {
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    color: var(--accent, #0a84ff);
    font-size: 13px;
    cursor: pointer;
    padding: 4px 0;
    margin-bottom: 12px;
    font-family: inherit;
  }
  .btn-back:hover {
    text-decoration: underline;
  }
  .imap-fields {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .field-row {
    display: flex;
    gap: 6px;
  }
  .tls-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .tls-label {
    font-size: 12px;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .credential-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--border-color, var(--border));
    border-radius: var(--radius-standard, 6px);
    background: var(--bg-view, var(--bg-primary));
    color: var(--text-primary);
    font-size: var(--font-size-base, 13px);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }
  .credential-input::placeholder {
    color: var(--text-secondary);
    opacity: 0.5;
  }
  .credential-input:focus {
    outline: none;
    border-color: var(--accent-blue, var(--accent, #0a84ff));
  }
  .imap-test-result {
    font-size: 11px;
    padding: 6px 8px;
    border-radius: 4px;
  }
  .imap-test-result.error {
    background: var(--bg-danger, rgba(255, 59, 48, 0.1));
    color: var(--text-danger, #ff453a);
  }
  .imap-test-result.success {
    background: var(--bg-success, rgba(52, 199, 89, 0.1));
    color: var(--text-success, #30d158);
  }
  .action-row {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .btn-test, .btn-add {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border: 1px solid var(--border-color, var(--border));
    border-radius: var(--radius-standard, 6px);
    background: var(--bg-view, var(--bg-primary));
    color: var(--text-primary);
    font-size: var(--font-size-base, 13px);
    font-family: inherit;
    font-weight: 500;
    cursor: pointer;
    transition: border-color 0.1s;
  }
  .btn-test:hover:not(:disabled), .btn-add:hover:not(:disabled) {
    border-color: var(--accent-blue, var(--accent, #0a84ff));
  }
  .btn-test:disabled, .btn-add:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-add {
    font-weight: 600;
  }
</style>
