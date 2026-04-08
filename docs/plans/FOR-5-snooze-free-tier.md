# FOR-5: Snooze Emails — Free Tier Implementation Plan

## Overview

Allow users to snooze emails so they disappear from inbox and reappear at a scheduled time.
Free tier: 3 preset times (Later Today +3h, Tomorrow Morning 9 AM, Next Week Monday 9 AM).

## Design Decision: Client-Side Snooze

Gmail API does **not** expose a snooze endpoint. We implement snooze client-side:
- On snooze: remove `INBOX` label via Gmail API, store `snoozed_until` timestamp locally
- On un-snooze (timer fires): re-add `INBOX` label via Gmail API
- A local `SNOOZED` virtual label lets users browse snoozed threads in the sidebar

## Architecture

```
┌─────────────────────────────────────────────────┐
│ Svelte Frontend                                  │
│  SnoozePopover.svelte  — preset picker UI        │
│  MessageDetail toolbar — snooze button            │
│  ThreadList            — right-click snooze       │
│  Sidebar               — SNOOZED virtual label    │
│  +page.svelte          — check_snoozed on sync    │
└──────────────┬──────────────────────────────────┘
               │ invoke()
┌──────────────▼──────────────────────────────────┐
│ Rust Backend (Tauri commands)                    │
│  snooze_thread     — set snoozed_until + Gmail   │
│  unsnooze_thread   — clear + re-add INBOX        │
│  get_snoozed       — list snoozed threads        │
│  check_snoozed     — process expired snoozes     │
└──────────────┬──────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────┐
│ SQLite (new table)                               │
│  snoozed_threads(thread_id, account_id,          │
│                  snoozed_until, created_at)       │
└─────────────────────────────────────────────────┘
```

## Implementation Steps

### Phase 1: Database & Migration

**File: `src-tauri/src/db.rs`**

1. Add `snoozed_threads` table to `apply_schema`:
```sql
CREATE TABLE IF NOT EXISTS snoozed_threads (
    thread_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    snoozed_until INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, account_id)
);
CREATE INDEX IF NOT EXISTS idx_snoozed_until ON snoozed_threads(snoozed_until);
```

2. Add migration `m006_create_snoozed_threads` (idempotent — check table existence)
3. Update `run_pending_migrations` range to `1..=6`
4. Bump `CURRENT_SCHEMA_VERSION` to `"3"`

**Tests (db.rs):**
- `test_apply_schema_creates_all_tables` — add `snoozed_threads` to expected tables
- `test_m006_creates_snoozed_threads_table` — run migration, verify table + index
- `test_migration_idempotent_with_existing_snoozed_table`

### Phase 2: Rust Backend Commands

**New file: `src-tauri/src/commands/snooze.rs`**

#### `snooze_thread` command
```rust
#[tauri::command]
pub async fn snooze_thread(
    pool: State<'_, SqlitePool>,
    account_id: String,
    thread_id: String,
    snoozed_until: i64,  // Unix timestamp (seconds)
) -> Result<(), String>
```
- Validate `snoozed_until` is in the future
- Get access token via `credentials::get_valid_token`
- Call `modify_thread` to remove `INBOX` label
- Insert into `snoozed_threads` table
- If Gmail API fails, don't insert into DB (atomic behavior)

#### `unsnooze_thread` command
```rust
#[tauri::command]
pub async fn unsnooze_thread(
    pool: State<'_, SqlitePool>,
    account_id: String,
    thread_id: String,
) -> Result<(), String>
```
- Get access token
- Call `modify_thread` to add `INBOX` label
- Delete from `snoozed_threads`
- Tolerant of Gmail failures (log warning, still remove local snooze)

#### `get_snoozed_threads` command
```rust
#[tauri::command]
pub async fn get_snoozed_threads(
    pool: State<'_, SqlitePool>,
    account_id: String,
) -> Result<Vec<SnoozedThreadInfo>, String>
```
- Join `snoozed_threads` with `threads` to get thread metadata
- Return thread info + `snoozed_until` timestamp
- Order by `snoozed_until ASC` (next to expire first)

#### `check_snoozed_threads` command
```rust
#[tauri::command]
pub async fn check_snoozed_threads(
    pool: State<'_, SqlitePool>,
    account_id: String,
) -> Result<Vec<String>, String>  // returns un-snoozed thread IDs
```
- Query `snoozed_threads WHERE snoozed_until <= now()`
- For each expired: call `unsnooze_thread` logic
- Return list of thread IDs that were un-snoozed (so frontend can refresh)

**Update: `src-tauri/src/commands/mod.rs`** — add `pub mod snooze;`
**Update: `src-tauri/src/lib.rs`** — register all 4 commands

**Tests (snooze.rs) — ~12 tests:**
- `test_snooze_thread_removes_inbox_and_stores`
- `test_snooze_thread_rejects_past_timestamp`
- `test_snooze_thread_duplicate_updates_timestamp`
- `test_unsnooze_thread_readds_inbox_and_removes_record`
- `test_unsnooze_nonexistent_thread_is_noop`
- `test_get_snoozed_threads_returns_ordered`
- `test_get_snoozed_threads_empty`
- `test_check_snoozed_processes_expired`
- `test_check_snoozed_skips_future`
- `test_check_snoozed_handles_gmail_failure_gracefully`
- `test_snooze_gmail_failure_no_db_insert`
- `test_snooze_thread_idempotent`

### Phase 3: Frontend — SnoozePopover Component

**New file: `src/lib/components/SnoozePopover.svelte`**

Dropdown popover with 3 preset options:
- **Later Today** — current time + 3 hours (or tomorrow 9 AM if after 6 PM)
- **Tomorrow Morning** — next day 9:00 AM local
- **Next Week** — next Monday 9:00 AM local

```svelte
<script lang="ts">
  interface Props {
    onsnooze: (until: number) => void;
    onclose: () => void;
  }
</script>

<!-- Positioned popover with 3 clickable options -->
<!-- Each shows the computed date/time as subtitle -->
<!-- Click outside or Escape to close -->
```

Design: matches existing toolbar button popover patterns (similar to star picker).
Each option shows: icon + label + computed datetime preview (e.g., "Today, 5:30 PM").

**Tests (SnoozePopover.test.js) — ~6 tests:**
- Renders 3 options with correct labels
- "Later Today" computes +3h from now
- "Later Today" after 6 PM rolls to tomorrow 9 AM
- "Tomorrow Morning" computes next day 9 AM
- "Next Week" computes next Monday 9 AM
- Fires `onsnooze` callback with correct timestamp
- Escape key calls `onclose`

### Phase 4: Frontend — Integration Points

#### MessageDetail.svelte toolbar
- Add snooze button (clock icon) between archive and trash buttons
- Click toggles `SnoozePopover` positioned below the button
- When `onsnooze` fires: `invoke("snooze_thread", {...})`, then `onaction("snooze")`

#### ThreadList.svelte — right-click context menu
- Add basic right-click context menu on thread rows (doesn't exist yet)
- Items: Archive, Trash, Mark Unread, Snooze > (sub-popover with presets)
- This is a new `ThreadContextMenu.svelte` component

#### Sidebar.svelte — SNOOZED virtual label
- Add "Snoozed" entry in system labels section (with clock icon)
- When selected: fetch via `get_snoozed_threads` instead of normal label query
- Show `snoozed_until` as secondary text on each thread row

#### +page.svelte — sync integration
- In `performSync()`: call `check_snoozed_threads` before the normal sync
- If any threads were un-snoozed, trigger a thread list refresh
- Also call `check_snoozed_threads` in `checkAndSetupSync` interval

#### icons.ts
- Add `iconSnooze` (clock with z) and map in `getLabelIcon` for 'SNOOZED'

**Tests:**
- MessageDetail.test.js: snooze button renders, triggers popover
- ThreadList context menu: basic render + snooze option
- Integration: snooze flow end-to-end (mock invoke)

### Phase 5: CommandPalette Integration

**File: `src/lib/components/CommandPalette.svelte`**

Add snooze actions:
- "Snooze: Later Today"
- "Snooze: Tomorrow Morning"  
- "Snooze: Next Week"

Only visible when a thread is selected.

## File Change Summary

| File | Change |
|------|--------|
| `src-tauri/src/db.rs` | New table + migration m006 |
| `src-tauri/src/commands/snooze.rs` | **NEW** — 4 commands |
| `src-tauri/src/commands/mod.rs` | Add `pub mod snooze` |
| `src-tauri/src/lib.rs` | Register 4 new commands |
| `src/lib/components/SnoozePopover.svelte` | **NEW** — preset picker |
| `src/lib/components/ThreadContextMenu.svelte` | **NEW** — right-click menu |
| `src/lib/components/MessageDetail.svelte` | Snooze button in toolbar |
| `src/lib/components/ThreadList.svelte` | Right-click handler |
| `src/lib/components/Sidebar.svelte` | SNOOZED virtual label |
| `src/lib/components/icons.ts` | iconSnooze + getLabelIcon |
| `src/lib/components/CommandPalette.svelte` | Snooze actions |
| `src/routes/+page.svelte` | check_snoozed in sync loop |

## Test Summary

- **Rust**: ~15 tests (db migration + 12 snooze command tests)
- **Svelte**: ~10 tests (SnoozePopover + integration)
- **Total**: ~25 new tests

## Edge Cases & Pitfalls

1. **App closed during snooze period** — `check_snoozed_threads` runs on app launch and every sync interval, so un-snooze happens on next open
2. **Thread synced back to INBOX by Gmail** — if another client moves it back, the snooze record still exists; `check_snoozed` will be a no-op since INBOX label is already there
3. **Thread deleted while snoozed** — `unsnooze` should handle missing thread gracefully (just clean up DB record)
4. **Multiple accounts** — all operations are scoped by `account_id`
5. **Timezone handling** — all timestamps in UTC (Unix epoch). Frontend converts to local time for display only
6. **Snooze same thread twice** — use UPSERT to update `snoozed_until`
