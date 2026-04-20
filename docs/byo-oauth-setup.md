# Using Your Own Google OAuth Credentials

This guide explains how to set up your own Google Cloud OAuth credentials for Rustymail. This is an **advanced feature** for power users, self-hosters, or organizations who want full control over API access.

**Most users don't need this.** Rustymail includes built-in credentials that work out of the box. You only need custom credentials if:

- You're self-hosting for an organization
- You've hit the 100-user limit on the built-in credentials
- You want full control over which Google APIs your accounts access
- You're a developer contributing to Rustymail

---

## Prerequisites

- A Google account (personal `@gmail.com` or Google Workspace)
- Access to [Google Cloud Console](https://console.cloud.google.com)
- ~10 minutes

---

## Step 1: Create a Google Cloud Project

1. Go to [console.cloud.google.com](https://console.cloud.google.com)
2. Click the **project dropdown** at the top of the page (next to "Google Cloud")
3. Click **"New Project"**
4. Enter a project name (e.g., "Rustymail" or "My Email Client")
5. Click **"Create"**
6. Wait for the project to be created, then make sure it's selected in the project dropdown

---

## Step 2: Enable Required APIs

Rustymail needs access to Gmail, Google Calendar, and Google Drive APIs.

1. In your project, go to **APIs & Services → Library** (left sidebar)
2. Search for and enable each of these APIs (click on each, then click **"Enable"**):

   | API | Purpose |
   |-----|---------|
   | **Gmail API** | Read, send, and manage emails |
   | **Google Calendar API** | View and manage calendar events |
   | **Google Drive API** | Upload attachments via Google Drive |

3. Verify all three are enabled by going to **APIs & Services → Enabled APIs** — you should see all three listed

---

## Step 3: Configure the OAuth Consent Screen

This is the screen Google shows when a user authorizes your app. Even for personal use, you need to configure it.

1. Go to **APIs & Services → OAuth consent screen**
2. Select **User Type**:
   - **External** — for personal `@gmail.com` accounts (most users)
   - **Internal** — only if you have a Google Workspace organization and want to restrict to your domain
3. Click **"Create"**

### Fill in the consent screen details:

| Field | Value |
|-------|-------|
| **App name** | Rustymail (or any name you like) |
| **User support email** | Your email address |
| **Developer contact information** | Your email address |

Leave all other fields (app logo, app domain, authorized domains) blank — they're not required.

4. Click **"Save and Continue"**

### Add Scopes:

1. Click **"Add or Remove Scopes"**
2. In the filter box, search for and check each of these scopes:

   | Scope | Description |
   |-------|-------------|
   | `openid` | Basic identity |
   | `email` | Email address |
   | `profile` | Name and profile picture |
   | `https://www.googleapis.com/auth/gmail.readonly` | Read emails |
   | `https://www.googleapis.com/auth/gmail.modify` | Modify labels, archive, etc. |
   | `https://www.googleapis.com/auth/gmail.send` | Send emails |
   | `https://www.googleapis.com/auth/gmail.labels` | Manage labels |
   | `https://www.googleapis.com/auth/calendar.events` | Calendar events |
   | `https://www.googleapis.com/auth/drive.file` | Upload attachments |

3. Click **"Update"** at the bottom, then **"Save and Continue"**

### Add Test Users:

> **Important:** While your app is in "Testing" mode (the default), only explicitly listed test users can authenticate. Tokens for test users also **expire every 7 days**, requiring re-authentication.

1. Click **"Add Users"**
2. Enter your email address (the one you'll use with Rustymail)
3. If other people will use your credentials, add their emails too
4. Click **"Add"**, then **"Save and Continue"**

### Summary:

Review and click **"Back to Dashboard"**. Your consent screen should show status: **"Testing"**.

---

## Step 4: Create OAuth 2.0 Credentials

1. Go to **APIs & Services → Credentials**
2. Click **"Create Credentials"** at the top
3. Select **"OAuth client ID"**
4. **Application type:** Select **"Desktop app"**

   > **Critical:** You MUST select "Desktop app", not "Web application". Desktop apps handle redirect URIs automatically. Web application type requires exact redirect URI configuration and will not work with Rustymail.

5. **Name:** Enter any name (e.g., "Rustymail Desktop")
6. Click **"Create"**

### Copy your credentials:

A dialog appears showing your **Client ID** and **Client Secret**. Copy both values — you'll need them in the next step.

- **Client ID** looks like: `123456789012-abcdefghijklmnop.apps.googleusercontent.com`
- **Client Secret** looks like: `GOCSPX-AbCdEfGhIjKlMnOpQrStUvWxYz`

You can always find these again later in **APIs & Services → Credentials** → click on your OAuth client.

---

## Step 5: Add Credentials to Rustymail

1. Open Rustymail
2. Go to **Settings → Accounts**
3. Click **"Add Account"**
4. Click **"Use your own OAuth credentials"** (expandable section)
5. Paste your **Client ID** in the first field
6. Paste your **Client Secret** in the second field
7. Click **"Sign in with custom credentials"**
8. Your browser opens with Google's consent screen — authorize the requested permissions
9. Return to Rustymail — your account is now connected using your custom credentials

The account will show a **"Custom OAuth"** badge in Settings to indicate it uses your credentials.

---

## Important Notes

### Token Expiry in Testing Mode

While your Google Cloud project's OAuth consent screen is in **"Testing"** mode:

- Access tokens expire normally (~1 hour, auto-refreshed by Rustymail)
- **Refresh tokens expire after 7 days** — you'll need to re-authenticate weekly
- Only explicitly listed test users can authenticate

To avoid the 7-day expiry, you must **publish your app**:

1. Go to **OAuth consent screen → Publishing status**
2. Click **"Publish App"**
3. For apps using sensitive scopes (Gmail modify, send), Google requires:
   - A link to your privacy policy
   - Potentially a security review (CASA assessment)
   - This process can take weeks

For **personal use**, staying in Testing mode and re-authenticating every 7 days is the simplest approach.

### Mixing Built-in and Custom Accounts

You can have multiple accounts with different credential sources:

- Click "Sign in with Google" → uses Rustymail's built-in credentials
- Click "Use your own OAuth credentials" → uses your custom credentials
- Each account independently tracks which credentials it was created with
- Token refresh automatically uses the correct credentials for each account

### Changing or Removing Custom Credentials

- If you change your Client ID or Client Secret in Google Cloud Console, existing accounts will fail to refresh tokens. You'll need to remove and re-add those accounts.
- Removing a custom-credential account from Rustymail does NOT delete your Google Cloud project — you can re-use the same credentials later.

### Security

- Google considers OAuth client secrets for desktop apps as **non-confidential** — they are routinely embedded in application binaries
- Your credentials are stored locally on your device in the Rustymail database
- Access and refresh tokens are stored in your operating system's secure credential storage (macOS Keychain, Windows Credential Manager, or Linux Secret Service)
- No credentials or tokens are ever sent to Rustymail's servers — there are no Rustymail servers

---

## Troubleshooting

### "Access blocked: This app's request is invalid"
- Make sure you selected **"Desktop app"** as the application type (not "Web application")
- Verify the APIs are enabled (Step 2)

### "This app isn't verified"
- This is normal for Testing mode. Click **"Continue"** (you may need to click "Advanced" first)
- Only test users you added in Step 3 will see this screen

### "Error 403: access_denied"
- Your email is not listed as a test user. Go to OAuth consent screen → Test users → add your email

### "Missing required permissions"
- You didn't add all required scopes in Step 3. Go back to OAuth consent screen → Scopes and add the missing ones

### "Authentication expired" after 7 days
- Expected behavior in Testing mode. Re-authenticate by removing and re-adding the account, or publish your app

### "Custom OAuth credentials not configured"
- The Client ID or Client Secret field was empty when you clicked "Sign in with custom credentials"
- Make sure both fields are filled in before clicking the button

---

## For Organizations / Self-Hosters

If you're deploying Rustymail for your organization:

1. Create the Google Cloud project under your organization's Google Workspace
2. Set the OAuth consent screen to **"Internal"** — this restricts access to your domain but removes the 100-user limit and 7-day token expiry
3. Share the Client ID and Client Secret with your team members
4. Each team member adds their account using "Use your own OAuth credentials" in Rustymail

For Internal apps, no Google review or CASA assessment is required.
