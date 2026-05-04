<script lang="ts">
    import type { ContactWithEmails } from '$lib/stores/contacts';

    let { contact, x, y }: { contact: Partial<ContactWithEmails>; x: number; y: number } = $props();

    function getInitials(name: string): string {
        return name.split(' ').map(w => w[0]).slice(0, 2).join('').toUpperCase();
    }

    let subtitle = $derived(
        [contact.job_title, contact.company].filter(Boolean).join(' at ')
    );
</script>

<div class="hover-card" style="left:{x}px; top:{y}px">
    <div class="hc-header">
        <div class="hc-avatar">{getInitials(contact.display_name || '?')}</div>
        <div class="hc-info">
            <div class="hc-name">{contact.display_name}</div>
            {#if subtitle}
                <div class="hc-subtitle">{subtitle}</div>
            {/if}
        </div>
    </div>
    {#if contact.emails?.length}
        <div class="hc-email">{contact.emails[0].email}</div>
    {/if}
</div>

<style>
    .hover-card {
        position: fixed;
        background: var(--bg-primary);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 12px;
        box-shadow: 0 4px 12px rgba(0,0,0,0.15);
        z-index: 9999;
        min-width: 200px;
        max-width: 280px;
    }
    .hc-header { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
    .hc-avatar { width: 28px; height: 28px; border-radius: 50%; background: var(--accent-color); color: white; display: flex; align-items: center; justify-content: center; font-size: 10px; font-weight: 600; }
    .hc-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
    .hc-subtitle { font-size: 11px; color: var(--text-secondary); }
    .hc-email { font-size: 11px; color: var(--text-secondary); padding-left: 38px; }
</style>
