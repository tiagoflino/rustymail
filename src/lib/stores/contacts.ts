import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface ContactEmail {
    id: string;
    contact_id: string;
    email: string;
    type: string;
    is_primary: boolean;
}

export interface ContactWithEmails {
    id: string;
    account_id: string;
    display_name: string;
    given_name: string | null;
    surname: string | null;
    nickname: string | null;
    company: string | null;
    job_title: string | null;
    department: string | null;
    notes: string | null;
    birthday: string | null;
    photo_uri: string | null;
    phones: string;
    addresses: string;
    social_profiles: string;
    urls: string;
    relations: string;
    is_starred: boolean;
    source: string;
    email_count_sent: number;
    email_count_received: number;
    first_seen_at: number | null;
    last_contacted_at: number | null;
    is_promoted: boolean;
    created_at: number;
    updated_at: number;
    emails: ContactEmail[];
    groups: string[];
}

export interface ContactGroup {
    id: string;
    account_id: string;
    name: string;
    color: string | null;
    remote_id: string | null;
    created_at: number;
}

export const contacts = writable<ContactWithEmails[]>([]);
export const selectedContactId = writable<string | null>(null);
export const contactSearchQuery = writable<string>('');
export const contactFilter = writable<{ group?: string; starred?: boolean }>({});
export const isContactsSyncing = writable(false);
export const contactGroups = writable<ContactGroup[]>([]);

export async function loadContacts(search?: string, groupId?: string) {
    const result = await invoke<ContactWithEmails[]>('get_contacts', {
        search: search || null,
        groupId: groupId || null,
        offset: 0,
        limit: 50,
        accountId: null,
    });
    contacts.set(result);
    return result;
}

export async function searchContacts(query: string) {
    return loadContacts(query);
}

export async function loadContactGroups() {
    const result = await invoke<ContactGroup[]>('get_contact_groups', { accountId: null });
    contactGroups.set(result);
    return result;
}

export async function createContact(input: any) {
    const result = await invoke<ContactWithEmails>('create_contact', { input, accountId: null });
    contacts.update(list => [...list, result]);
    return result;
}

export async function updateContact(contactId: string, input: any) {
    const result = await invoke<ContactWithEmails>('update_contact', { contactId, input });
    contacts.update(list => list.map(c => c.id === contactId ? result : c));
    return result;
}

export async function deleteContact(contactId: string) {
    await invoke('delete_contact', { contactId });
    contacts.update(list => list.filter(c => c.id !== contactId));
    selectedContactId.update(id => id === contactId ? null : id);
}

export async function mergeContacts(primaryId: string, secondaryId: string) {
    const result = await invoke<ContactWithEmails>('merge_contacts', { primaryId, secondaryId });
    contacts.update(list => list.filter(c => c.id !== secondaryId).map(c => c.id === primaryId ? result : c));
    return result;
}
