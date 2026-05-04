<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { addToast } from "$lib/stores/toast";
  import type { ContactWithEmails, ContactGroup } from "$lib/stores/contacts";

  let contacts = $state<ContactWithEmails[]>([]);
  let contactGroups = $state<ContactGroup[]>([]);
  let selectedContact = $state<ContactWithEmails | null>(null);
  let searchQuery = $state("");
  let activeGroupFilter = $state<string | null>(null);
  let starredFilter = $state(false);
  let showForm = $state(false);
  let showImportExport = $state(false);
  let editingContact = $state<ContactWithEmails | null>(null);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  async function loadContacts(search?: string, groupId?: string) {
    try {
      const result = await invoke<ContactWithEmails[]>("get_contacts", {
        search: search || null,
        groupId: groupId || null,
        offset: 0,
        limit: 200,
        accountId: null,
      });
      contacts = result ?? [];
    } catch (e) {
      addToast(`Failed to load contacts: ${e}`, "error", 5000);
    }
  }

  async function loadGroups() {
    try {
      const result = await invoke<ContactGroup[]>("get_contact_groups", { accountId: null });
      contactGroups = result ?? [];
    } catch (e) {
      addToast(`Failed to load contact groups: ${e}`, "error", 5000);
    }
  }

  function handleSearchInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    searchQuery = value;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      loadContacts(value, activeGroupFilter ?? undefined);
    }, 200);
  }

  function selectGroup(groupId: string | null) {
    activeGroupFilter = groupId;
    starredFilter = false;
    loadContacts(searchQuery, groupId ?? undefined);
  }

  function selectStarred() {
    starredFilter = true;
    activeGroupFilter = null;
    loadContacts(searchQuery);
  }

  function selectAll() {
    starredFilter = false;
    activeGroupFilter = null;
    loadContacts(searchQuery);
  }

  let filteredContacts = $derived(
    starredFilter ? contacts.filter(c => c.is_starred) : contacts
  );

  function selectContact(contact: ContactWithEmails) {
    selectedContact = contact;
  }

  function getInitials(name: string): string {
    const parts = name.trim().split(/\s+/);
    if (parts.length >= 2) {
      return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
    }
    return name.trim().substring(0, 2).toUpperCase();
  }

  function getAvatarColor(name: string): string {
    const colors = [
      "#FF6B6B", "#4ECDC4", "#45B7D1", "#96CEB4",
      "#FFEAA7", "#DDA0DD", "#98D8C8", "#F7DC6F",
      "#BB8FCE", "#85C1E9", "#F1948A", "#82E0AA",
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
      hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    return colors[Math.abs(hash) % colors.length];
  }

  function getPrimaryEmail(contact: ContactWithEmails): string {
    const primary = contact.emails.find(e => e.is_primary);
    return primary?.email ?? contact.emails[0]?.email ?? "";
  }

  function parseJsonField(field: string): any[] {
    try {
      return JSON.parse(field) ?? [];
    } catch {
      return [];
    }
  }

  async function handleDelete(contact: ContactWithEmails) {
    try {
      await invoke("delete_contact", { contactId: contact.id });
      contacts = contacts.filter(c => c.id !== contact.id);
      if (selectedContact?.id === contact.id) {
        selectedContact = null;
      }
      addToast("Contact deleted", "info");
    } catch (e) {
      addToast(`Failed to delete contact: ${e}`, "error", 5000);
    }
  }

  function handleNew() {
    editingContact = null;
    showForm = true;
  }

  function handleEdit() {
    if (!selectedContact) return;
    editingContact = selectedContact;
    showForm = true;
  }

  onMount(() => {
    loadContacts();
    loadGroups();
  });
</script>

<div class="contacts-container">
  <div class="contacts-sidebar">
    <div class="contacts-header">
      <h2 class="contacts-title">Contacts</h2>
      <div class="contacts-actions">
        <button
          class="action-btn"
          title="Import/Export"
          onclick={() => { showImportExport = true; }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" y1="15" x2="12" y2="3" />
          </svg>
        </button>
        <button
          class="action-btn primary"
          title="New contact"
          onclick={handleNew}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
      </div>
    </div>

    <div class="search-container">
      <svg class="search-icon" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        type="text"
        class="search-input"
        placeholder="Search contacts..."
        value={searchQuery}
        oninput={handleSearchInput}
      />
    </div>

    <div class="group-filters">
      <button
        class="filter-btn"
        class:active={!starredFilter && !activeGroupFilter}
        onclick={selectAll}
      >
        All
      </button>
      <button
        class="filter-btn"
        class:active={starredFilter}
        onclick={selectStarred}
      >
        Starred
      </button>
      {#each contactGroups as group}
        <button
          class="filter-btn"
          class:active={activeGroupFilter === group.id}
          onclick={() => selectGroup(group.id)}
        >
          {group.name}
        </button>
      {/each}
    </div>

    <div class="contact-list">
      {#each filteredContacts as contact (contact.id)}
        <button
          class="contact-item"
          class:selected={selectedContact?.id === contact.id}
          onclick={() => selectContact(contact)}
        >
          <div class="contact-avatar" style="background-color: {getAvatarColor(contact.display_name)}">
            {getInitials(contact.display_name)}
          </div>
          <div class="contact-info">
            <div class="contact-name">{contact.display_name}</div>
            <div class="contact-email">{getPrimaryEmail(contact)}</div>
            {#if contact.company}
              <div class="contact-company">{contact.company}</div>
            {/if}
          </div>
        </button>
      {/each}

      {#if filteredContacts.length === 0}
        <div class="empty-state">
          <p>No contacts found</p>
        </div>
      {/if}
    </div>
  </div>

  <div class="contact-detail-panel">
    {#if selectedContact}
      <div class="detail-header">
        <div class="detail-actions">
          <button class="action-btn" title="Edit contact" onclick={handleEdit}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
              <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
            </svg>
          </button>
          <button class="action-btn danger" title="Delete contact" onclick={() => handleDelete(selectedContact!)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="3 6 5 6 21 6" />
              <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
            </svg>
          </button>
        </div>
      </div>

      <div class="detail-content">
        <div class="detail-avatar" style="background-color: {getAvatarColor(selectedContact.display_name)}">
          {getInitials(selectedContact.display_name)}
        </div>
        <h3 class="detail-name">{selectedContact.display_name}</h3>
        {#if selectedContact.job_title || selectedContact.company}
          <p class="detail-title">
            {selectedContact.job_title ?? ""}{selectedContact.job_title && selectedContact.company ? " at " : ""}{selectedContact.company ?? ""}
          </p>
        {/if}

        <div class="detail-sections">
          {#if selectedContact.emails.length > 0}
            <div class="detail-section">
              <h4 class="section-label">Email</h4>
              {#each selectedContact.emails as email}
                <div class="section-value">
                  <span>{email.email}</span>
                  {#if email.is_primary}
                    <span class="badge">Primary</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          {#if parseJsonField(selectedContact.phones).length > 0}
            <div class="detail-section">
              <h4 class="section-label">Phone</h4>
              {#each parseJsonField(selectedContact.phones) as phone}
                <div class="section-value">
                  <span>{phone.number || phone.value}</span>
                  {#if phone.type}
                    <span class="badge">{phone.type}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          {#if parseJsonField(selectedContact.addresses).length > 0}
            <div class="detail-section">
              <h4 class="section-label">Address</h4>
              {#each parseJsonField(selectedContact.addresses) as addr}
                <div class="section-value">
                  <span>{addr.formatted || addr.street || ""}</span>
                  {#if addr.type}
                    <span class="badge">{addr.type}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          {#if selectedContact.birthday}
            <div class="detail-section">
              <h4 class="section-label">Birthday</h4>
              <div class="section-value">{selectedContact.birthday}</div>
            </div>
          {/if}

          {#if selectedContact.notes}
            <div class="detail-section">
              <h4 class="section-label">Notes</h4>
              <div class="section-value notes">{selectedContact.notes}</div>
            </div>
          {/if}

          {#if selectedContact.groups.length > 0}
            <div class="detail-section">
              <h4 class="section-label">Groups</h4>
              <div class="groups-list">
                {#each selectedContact.groups as groupId}
                  {@const group = contactGroups.find(g => g.id === groupId)}
                  {#if group}
                    <span class="group-tag">{group.name}</span>
                  {/if}
                {/each}
              </div>
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="no-selection">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" opacity="0.3">
          <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
        <p>Select a contact to view details</p>
      </div>
    {/if}
  </div>
</div>

<!-- ContactForm will be added here -->
<!-- ImportExport will be added here -->

<style>
  .contacts-container {
    display: flex;
    flex: 1;
    height: 100%;
    overflow: hidden;
  }

  .contacts-sidebar {
    width: 320px;
    min-width: 320px;
    border-right: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
    overflow: hidden;
  }

  .contacts-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-color);
  }

  .contacts-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .contacts-actions {
    display: flex;
    gap: 4px;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .action-btn:hover {
    background: var(--hover-bg);
    color: var(--text-primary);
  }

  .action-btn.primary {
    color: var(--accent-color);
  }

  .action-btn.primary:hover {
    background: var(--accent-color);
    color: #fff;
  }

  .action-btn.danger:hover {
    background: var(--danger-color);
    color: #fff;
  }

  .search-container {
    position: relative;
    padding: 8px 12px;
  }

  .search-icon {
    position: absolute;
    left: 20px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-tertiary);
    pointer-events: none;
  }

  .search-input {
    width: 100%;
    padding: 6px 8px 6px 28px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--text-primary);
    font-size: 13px;
    outline: none;
    transition: border-color 0.15s;
  }

  .search-input:focus {
    border-color: var(--accent-color);
  }

  .search-input::placeholder {
    color: var(--text-tertiary);
  }

  .group-filters {
    display: flex;
    gap: 4px;
    padding: 4px 12px 8px;
    flex-wrap: wrap;
  }

  .filter-btn {
    padding: 3px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .filter-btn:hover {
    background: var(--hover-bg);
  }

  .filter-btn.active {
    background: var(--selected-bg);
    color: var(--accent-color);
  }

  .contact-list {
    flex: 1;
    overflow-y: auto;
    padding: 0 4px;
  }

  .contact-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
    transition: background 0.15s;
    text-align: left;
  }

  .contact-item:hover {
    background: var(--hover-bg);
  }

  .contact-item.selected {
    background: var(--selected-bg);
  }

  .contact-avatar {
    width: 32px;
    height: 32px;
    min-width: 32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
  }

  .contact-info {
    flex: 1;
    min-width: 0;
  }

  .contact-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .contact-email {
    font-size: 11px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .contact-company {
    font-size: 11px;
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px 20px;
    color: var(--text-tertiary);
    font-size: 13px;
  }

  .contact-detail-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
    overflow-y: auto;
  }

  .detail-header {
    display: flex;
    justify-content: flex-end;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-color);
  }

  .detail-actions {
    display: flex;
    gap: 4px;
  }

  .detail-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 32px 24px;
  }

  .detail-avatar {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: 0.5px;
    margin-bottom: 12px;
  }

  .detail-name {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px;
  }

  .detail-title {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0 0 24px;
  }

  .detail-sections {
    width: 100%;
    max-width: 400px;
  }

  .detail-section {
    margin-bottom: 16px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--border-color);
  }

  .detail-section:last-child {
    border-bottom: none;
  }

  .section-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin: 0 0 6px;
  }

  .section-value {
    font-size: 13px;
    color: var(--text-primary);
    padding: 2px 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .section-value.notes {
    white-space: pre-wrap;
    color: var(--text-secondary);
  }

  .badge {
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--tag-bg);
    color: var(--text-secondary);
    font-weight: 500;
  }

  .groups-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .group-tag {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 4px;
    background: var(--tag-bg);
    color: var(--text-secondary);
  }

  .no-selection {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 12px;
    color: var(--text-tertiary);
    font-size: 13px;
  }
</style>
