# Rustymail

A cross-platform desktop email client built on Rust and Tauri. Connects to Gmail (REST API), Microsoft Outlook (Graph API), and any IMAP/SMTP provider. All data stored locally in SQLite. Optional local AI features via on-device LLM inference.

## Architecture

```
+------------------+     +--------------------+     +------------------+
|   SvelteKit UI   | <-> |   Tauri Commands   | <-> |  Provider Layer  |
|  (TypeScript)    |     |      (Rust)        |     | Gmail / IMAP /   |
|                  |     |                    |     | Graph / CalDAV   |
+------------------+     +--------------------+     +------------------+
                                  |
                          +-------v--------+
                          |    SQLite DB    |
                          | (local cache)  |
                          +----------------+
```

- **Frontend**: Svelte 5 / SvelteKit with static adapter, Vitest
- **Backend**: Tauri 2.x, Rust, sqlx (SQLite), tokio async runtime
- **AI**: llama-cpp-2 via `rustymail-premium` crate, local inference only
- **Providers**: Gmail REST API, Microsoft Graph API, IMAP/SMTP with OAuth2

## Features

### Multi-Provider Email

| Feature | Gmail | Outlook | IMAP |
|---------|-------|---------|------|
| Email sync (incremental) | Yes | Yes | Yes |
| Folder/label management | Yes | Yes | Yes |
| Send / reply / forward | Yes | Yes | Yes |
| Drafts | Yes | Yes | Yes |
| Threaded view | Yes | -- | -- |
| Server-side filters | Yes | -- | -- |
| OAuth authentication | Yes | Yes | Yes |
| IDLE push (instant sync) | -- | -- | Yes |
| CONDSTORE (fast sync) | -- | -- | Yes |
| Auto-discover (SRV/MX) | -- | -- | Yes |

### Productivity

| Feature | Description |
|---------|-------------|
| Unified Inbox | View mail from all accounts in one list |
| Scheduled Send | Compose now, send later |
| Snooze | Hide threads until a specific time |
| Email Templates | Save and reuse common replies |
| Subscription Detection | Automatic newsletter / marketing detection |
| One-click Unsubscribe | RFC 8058 compliant unsubscribe |
| Newsletter Feed | Dedicated reading view for subscriptions |
| Batch Operations | Archive, trash, star, mark read in bulk |
| Keyboard Shortcuts | Configurable via command palette |
| Calendar Integration | Gmail, Outlook, and CalDAV calendar sync |

### Privacy & Security

| Feature | Description |
|---------|-------------|
| Local Storage | All data stored in SQLite on device |
| No Telemetry | No usage data or email content collected |
| BYO OAuth | Use your own Google Cloud credentials |
| Tracker Blocking | Block tracking pixels in HTML email |
| Link Safety | Check URLs against known malicious domains |
| Privacy Policy | Publicly available, includes Google Limited Use compliance |
| AI on Device | LLM inference runs locally, no data leaves the device |

### AI Features (Premium)

| Feature | Description |
|---------|-------------|
| Thread Summarization | Generate concise summaries of long threads |
| Compose from Prompt | Draft emails from natural language instructions |
| Smart Reply Suggestions | Context-aware quick reply options |
| Action Item Extraction | Identify tasks and deadlines from emails |
| Sentiment Analysis | Detect urgency and emotional tone |

All AI features run entirely on-device using a local language model. No email content is sent to external services.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 18+
- Platform SDKs: Xcode (macOS), Visual Studio Build Tools (Windows), or `libgtk-3-dev libwebkit2gtk-4.1-dev` (Linux)

### Build and Run

```bash
git clone https://github.com/tiagoflino/rustymail.git
cd rustymail
make install
make dev
```

### Build with AI Features

```bash
make dev-premium
```

The premium build requires access to the private `rustymail-premium` repository.

### Release Build

```bash
make build
```

Outputs to `src-tauri/target/release/`.

## Configuration

### Environment Variables

Copy `.env.example` to `src-tauri/.env`:

| Variable | Description |
|----------|-------------|
| `RUSTYMAIL_CLIENT_ID` | Google OAuth client ID |
| `RUSTYMAIL_CLIENT_SECRET` | Google OAuth client secret |
| `RUSTYMAIL_MICROSOFT_CLIENT_ID` | Microsoft Entra application ID |

### BYO OAuth

You can use your own Google Cloud project credentials instead of the built-in ones. See [BYO OAuth Setup](docs/byo-oauth-setup.md) for step-by-step instructions.

### Settings

All preferences are stored locally in SQLite. Key settings include:

- Theme (system / light / dark)
- Density (compact / default / comfortable)
- Reading pane (right / bottom / off)
- Sync frequency (manual to 60 minutes)
- Threads per page
- Mark as read delay
- Notification preferences
- AI model settings (threads, auto-unload, extract actions)

## Documentation

- [Privacy Policy](docs/PRIVACY.md)
- [Terms of Service](docs/TERMS.md)
- [BYO OAuth Setup](docs/byo-oauth-setup.md)

## Development

```bash
make lint      # TypeScript check + Rust clippy
make test      # Frontend tests + Rust tests
make dev       # Development server
```

### Architecture Decision Records

Key architectural decisions:

- **Dual Gmail backend**: Gmail uses REST API (superior threading, labels, push notifications). IMAP/SMTP is a separate backend behind a provider trait.
- **Local-first**: All data cached in SQLite. The app works offline. Sync brings changes incrementally.
- **AI on device**: llama-cpp-2 with Metal (macOS), Vulkan (Linux/Windows), or CUDA (NVIDIA). Model downloads on first use.
- **Premium split**: AI features live in a private crate (`rustymail-premium`) behind a Cargo feature flag. The core app is fully open-source.

## Technology Stack

| Component | Technology |
|-----------|-----------|
| Desktop framework | Tauri 2.x |
| UI framework | Svelte 5 / SvelteKit |
| Language (backend) | Rust |
| Language (frontend) | TypeScript |
| Database | SQLite via sqlx |
| HTTP client | reqwest |
| Email parsing | lettre (SMTP), async-imap |
| HTML sanitization | ammonia / css-inline |
| LLM inference | llama-cpp-2 (Granite 3.2 2B) |
| Testing (backend) | Rust built-in + tokio-test + httpmock |
| Testing (frontend) | Vitest + @testing-library/svelte |
| CI/CD | GitHub Actions |

## License

Source code is available under the terms in the LICENSE file. The `rustymail-premium` crate is a separate private repository.
