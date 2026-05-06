<script lang="ts">
  import { createContact, updateContact, type ContactWithEmails } from '$lib/stores/contacts';
  import { addToast } from '$lib/stores/toast';

  let {
    contact = null,
    onClose,
    onSaved,
  }: {
    contact?: ContactWithEmails | null;
    onClose: () => void;
    onSaved: (c: ContactWithEmails) => void;
  } = $props();

  let displayName = $state(contact?.display_name ?? '');
  let firstName = $state(contact?.given_name ?? '');
  let lastName = $state(contact?.surname ?? '');
  let company = $state(contact?.company ?? '');
  let jobTitle = $state(contact?.job_title ?? '');
  let notes = $state(contact?.notes ?? '');

  let emails = $state<{ email: string; type: string }[]>(
    contact?.emails?.length
      ? contact.emails.map(e => ({ email: e.email, type: e.type || 'personal' }))
      : [{ email: '', type: 'personal' }]
  );

  let phones = $state<{ number: string; type: string }[]>(
    parseJsonField(contact?.phones).length
      ? parseJsonField(contact?.phones).map((p: any) => ({ number: p.number || p.value || '', type: p.type || 'mobile' }))
      : []
  );

  let saving = $state(false);

  function parseJsonField(field: string | null | undefined): any[] {
    if (!field) return [];
    try {
      return JSON.parse(field) ?? [];
    } catch {
      return [];
    }
  }

  function addEmail() {
    emails = [...emails, { email: '', type: 'personal' }];
  }

  function removeEmail(index: number) {
    if (emails.length <= 1) return;
    emails = emails.filter((_, i) => i !== index);
  }

  function addPhone() {
    phones = [...phones, { number: '', type: 'mobile' }];
  }

  function removePhone(index: number) {
    phones = phones.filter((_, i) => i !== index);
  }

  async function handleSave() {
    if (!displayName.trim()) {
      addToast('Display name is required', 'error');
      return;
    }

    const validEmails = emails.filter(e => e.email.trim());
    if (validEmails.length === 0) {
      addToast('At least one email is required', 'error');
      return;
    }

    saving = true;

    const input = {
      display_name: displayName.trim(),
      given_name: firstName.trim() || null,
      surname: lastName.trim() || null,
      company: company.trim() || null,
      job_title: jobTitle.trim() || null,
      notes: notes.trim() || null,
      emails: validEmails.map((e, i) => ({
        email: e.email.trim(),
        type: e.type,
        is_primary: i === 0,
      })),
      phones: phones.filter(p => p.number.trim()).map(p => ({
        number: p.number.trim(),
        type: p.type,
      })),
    };

    try {
      let result: ContactWithEmails;
      if (contact) {
        result = await updateContact(contact.id, input);
      } else {
        result = await createContact(input);
      }
      addToast(contact ? 'Contact updated' : 'Contact created', 'info');
      onSaved(result);
    } catch (e) {
      addToast(`Failed to save contact: ${e}`, 'error', 5000);
    } finally {
      saving = false;
    }
  }

  function handleOverlayClick() {
    onClose();
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }
</script>

<div class="modal-overlay" onclick={handleOverlayClick} onkeydown={(e) => { if (e.key === 'Escape') onClose(); }} role="button" tabindex="-1" aria-label="Close modal">
  <div class="modal-card" onclick={handleModalClick} role="presentation">
    <h3 class="modal-title">{contact ? 'Edit Contact' : 'New Contact'}</h3>

    <form onsubmit={(e) => { e.preventDefault(); handleSave(); }}>
      <div class="form-group">
        <input
          type="text"
          placeholder="Display name"
          bind:value={displayName}
          class="form-input"
          required
        />
      </div>

      <div class="form-row">
        <input
          type="text"
          placeholder="First name"
          bind:value={firstName}
          class="form-input"
        />
        <input
          type="text"
          placeholder="Last name"
          bind:value={lastName}
          class="form-input"
        />
      </div>

      <div class="form-row">
        <input
          type="text"
          placeholder="Company"
          bind:value={company}
          class="form-input"
        />
        <input
          type="text"
          placeholder="Job title"
          bind:value={jobTitle}
          class="form-input"
        />
      </div>

      <div class="form-section">
        <span class="section-label">Emails</span>
        {#each emails as emailEntry, index}
          <div class="dynamic-row">
            <input
              type="email"
              placeholder="Email"
              bind:value={emailEntry.email}
              class="form-input flex-grow"
            />
            <select bind:value={emailEntry.type} class="form-select">
              <option value="personal">Personal</option>
              <option value="work">Work</option>
              <option value="other">Other</option>
            </select>
            {#if emails.length > 1}
              <button type="button" class="remove-btn" onclick={() => removeEmail(index)}>
                &times;
              </button>
            {/if}
          </div>
        {/each}
        <button type="button" class="add-btn" onclick={addEmail}>+ Add email</button>
      </div>

      <div class="form-section">
        <span class="section-label">Phones</span>
        {#each phones as phoneEntry, index}
          <div class="dynamic-row">
            <input
              type="tel"
              placeholder="Phone"
              bind:value={phoneEntry.number}
              class="form-input flex-grow"
            />
            <select bind:value={phoneEntry.type} class="form-select">
              <option value="mobile">Mobile</option>
              <option value="home">Home</option>
              <option value="work">Work</option>
              <option value="other">Other</option>
            </select>
            <button type="button" class="remove-btn" onclick={() => removePhone(index)}>
              &times;
            </button>
          </div>
        {/each}
        <button type="button" class="add-btn" onclick={addPhone}>+ Add phone</button>
      </div>

      <div class="form-group">
        <textarea
          placeholder="Notes"
          bind:value={notes}
          class="form-input form-textarea"
          rows="3"
        ></textarea>
      </div>

      <div class="form-actions">
        <button type="button" class="btn btn-cancel" onclick={onClose}>Cancel</button>
        <button type="submit" class="btn btn-save" disabled={saving}>
          {saving ? 'Saving...' : 'Save'}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(var(--blur-modal));
    -webkit-backdrop-filter: blur(var(--blur-modal));
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-card {
    background: var(--bg-view);
    border-radius: var(--radius-modal);
    padding: 24px;
    width: 100%;
    max-width: 480px;
    max-height: 85vh;
    overflow-y: auto;
    box-shadow: 0 24px 48px rgba(0, 0, 0, 0.2), 0 0 0 1px rgba(0, 0, 0, 0.1);
  }

  .modal-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 16px;
  }

  .form-group {
    margin-bottom: 12px;
  }

  .form-row {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .form-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--bg-list);
    color: var(--text-primary);
    font-size: 13px;
    outline: none;
    transition: border-color 0.15s;
  }

  .form-input:focus {
    border-color: var(--accent-blue);
  }

  .form-input::placeholder {
    color: var(--text-secondary);
  }

  .form-textarea {
    resize: vertical;
    min-height: 60px;
  }

  .form-select {
    padding: 8px 10px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--bg-list);
    color: var(--text-primary);
    font-size: 13px;
    outline: none;
    min-width: 90px;
  }

  .form-section {
    margin-bottom: 16px;
  }

  .section-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 6px;
  }

  .dynamic-row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 6px;
  }

  .flex-grow {
    flex: 1;
  }

  .remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 16px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .remove-btn:hover {
    background: #ff3b30;
    color: #fff;
  }

  .add-btn {
    border: none;
    background: transparent;
    color: var(--accent-blue);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    padding: 4px 0;
  }

  .add-btn:hover {
    text-decoration: underline;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }

  .btn {
    padding: 8px 16px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, opacity 0.15s;
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-cancel {
    background: var(--sidebar-hover);
    color: var(--text-primary);
  }

  .btn-cancel:hover {
    background: var(--border-color);
  }

  .btn-save {
    background: var(--accent-blue);
    color: #fff;
  }

  .btn-save:hover:not(:disabled) {
    opacity: 0.9;
  }
</style>
