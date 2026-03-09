use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    TokenResponse, TokenUrl,
};
use std::env;
use tauri::Manager;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[allow(dead_code)]
struct ActiveAccountFull {
    id: String,
    access_token: String,
    refresh_token: Option<String>,
    token_expiry: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct ActiveAccountRow {
    id: String,
    token_expiry: Option<i64>,
}

async fn get_active_account(pool: &sqlx::SqlitePool) -> Result<ActiveAccountFull, String> {
    let row = sqlx::query_as::<_, ActiveAccountRow>(
        "SELECT id, token_expiry FROM accounts WHERE is_active = 1 LIMIT 1"
    )
        .fetch_one(pool)
        .await
        .map_err(|e| format!("No active account found: {}", e))?;

    let now = chrono::Utc::now().timestamp();
    let expiry = row.token_expiry.unwrap_or(0);
    if expiry > 0 && expiry - 300 < now {
        let token_to_use = crate::credentials::get_refresh_token(&row.id).unwrap_or_default();
        if !token_to_use.is_empty() && refresh_and_update(pool, &row.id, &token_to_use).await.is_ok() {
            let refreshed = sqlx::query_as::<_, ActiveAccountRow>(
                "SELECT id, token_expiry FROM accounts WHERE is_active = 1 LIMIT 1"
            )
                .fetch_one(pool)
                .await
                .map_err(|_| "Failed to read account after refresh.".to_string())?;

            let access_token = crate::credentials::get_access_token(&refreshed.id)
                .map_err(|e| format!("Failed to read access token from keyring: {}", e))?;
            let refresh_token = crate::credentials::get_refresh_token(&refreshed.id).ok();

            return Ok(ActiveAccountFull {
                id: refreshed.id,
                access_token,
                refresh_token,
                token_expiry: refreshed.token_expiry,
            });
        }
    }

    let access_token = crate::credentials::get_access_token(&row.id)
        .map_err(|e| format!("Failed to read access token from keyring: {}", e))?;
    let refresh_token = crate::credentials::get_refresh_token(&row.id).ok();

    Ok(ActiveAccountFull {
        id: row.id,
        access_token,
        refresh_token,
        token_expiry: row.token_expiry,
    })
}

#[cfg(not(debug_assertions))]
fn get_client_credentials() -> Result<(String, String), String> {
    Ok((
        env!("RUSTYMAIL_CLIENT_ID").to_string(),
        env!("RUSTYMAIL_CLIENT_SECRET").to_string(),
    ))
}

#[cfg(debug_assertions)]
fn get_client_credentials() -> Result<(String, String), String> {
    let id = env::var("RUSTYMAIL_CLIENT_ID")
        .map_err(|_| "RUSTYMAIL_CLIENT_ID not found in environment".to_string())?;
    let secret = env::var("RUSTYMAIL_CLIENT_SECRET")
        .map_err(|_| "RUSTYMAIL_CLIENT_SECRET not found in environment".to_string())?;
    Ok((id.trim().to_string(), secret.trim().to_string()))
}

pub async fn start_oauth_flow(app_handle: tauri::AppHandle) -> Result<(), String> {
    let (client_id, client_secret) = get_client_credentials()?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_url = format!("http://127.0.0.1:{}", port);

    let client = BasicClient::new(ClientId::new(client_id))
        .set_client_secret(ClientSecret::new(client_secret))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string()).unwrap())
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".to_string()).unwrap())
        .set_redirect_uri(RedirectUrl::new(redirect_url.clone()).unwrap());

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(oauth2::Scope::new("openid".to_string()))
        .add_scope(oauth2::Scope::new("email".to_string()))
        .add_scope(oauth2::Scope::new("profile".to_string()))
        .add_scope(oauth2::Scope::new(
            "https://www.googleapis.com/auth/gmail.readonly".to_string(),
        ))
        .add_scope(oauth2::Scope::new(
            "https://www.googleapis.com/auth/gmail.modify".to_string(),
        ))
        .add_scope(oauth2::Scope::new(
            "https://www.googleapis.com/auth/gmail.send".to_string(),
        ))
        .add_scope(oauth2::Scope::new(
            "https://www.googleapis.com/auth/gmail.labels".to_string(),
        ))
        .add_scope(oauth2::Scope::new(
            "https://www.googleapis.com/auth/calendar.readonly".to_string(),
        ))
        .set_pkce_challenge(pkce_challenge)
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .url();

    match tauri_plugin_opener::open_url(auth_url.as_str(), None::<&str>) {
        Ok(_) => println!("[OAuth] Browser opened successfully"),
        Err(e) => {
            println!("[OAuth] Failed to open browser: {}", e);
            return Err(format!("Failed to open browser: {}", e));
        }
    }

    println!("[OAuth] Waiting for callback...");
    let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|e| e.to_string())?;

    let mut code = String::new();
    let mut state = String::new();

    if request_line.starts_with("GET") {
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() > 1 {
            let path = parts[1];
            if let Some(query) = path.split('?').nth(1) {
                for param in query.split('&') {
                    let mut kv = param.split('=');
                    if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                        if k == "code" {
                            code = v.to_string();
                        } else if k == "state" {
                            state = v.to_string();
                        }
                    }
                }
            }
        }
    }

    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body style='font-family:-apple-system,system-ui,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;color:#333;'><div style='text-align:center'><h2>Authentication successful!</h2><p>You can close this tab and return to Rustymail.</p></div></body></html>";
    let _ = stream.write_all(response.as_bytes()).await;

    if state != *csrf_token.secret() {
        return Err("CSRF token mismatch".to_string());
    }

    println!("[OAuth] Exchanging code for tokens...");
    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| {
            println!("[OAuth] Token exchange failed: {:?}", e);
            e.to_string()
        })?;
    println!("[OAuth] Token exchange succeeded.");

    let access_token = token_result.access_token().secret().to_string();
    let refresh_token = token_result
        .refresh_token()
        .map(|r| r.secret().clone())
        .unwrap_or_default();

    println!("Tokens acquired successfully!");

    let http_client = reqwest::Client::new();
    let (email, display_name, avatar_url) = match http_client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {}", &access_token))
        .send()
        .await
    {
        Ok(res) => {
            if let Ok(body) = res.json::<serde_json::Value>().await {
                (
                    body["email"]
                        .as_str()
                        .unwrap_or("unknown@gmail.com")
                        .to_string(),
                    body["name"].as_str().unwrap_or("").to_string(),
                    body["picture"].as_str().unwrap_or("").to_string(),
                )
            } else {
                (
                    "unknown@gmail.com".to_string(),
                    String::new(),
                    String::new(),
                )
            }
        }
        Err(_) => (
            "unknown@gmail.com".to_string(),
            String::new(),
            String::new(),
        ),
    };

    let account_id = email.clone();

    crate::credentials::store_tokens(&account_id, &access_token, &refresh_token)?;

    let pool = app_handle.state::<sqlx::SqlitePool>();

    let _ = sqlx::query("UPDATE accounts SET is_active = 0")
        .execute(pool.inner())
        .await;

    let sql = "INSERT INTO accounts (id, email, display_name, avatar_url, token_expiry, is_active, created_at)
               VALUES (?, ?, ?, ?, ?, 1, ?)
               ON CONFLICT(id) DO UPDATE SET
                 email = excluded.email,
                 display_name = excluded.display_name,
                 avatar_url = excluded.avatar_url,
                 token_expiry = excluded.token_expiry,
                 is_active = 1";
    sqlx::query(sql)
        .bind(&account_id)
        .bind(&email)
        .bind(&display_name)
        .bind(&avatar_url)
        .bind(chrono::Utc::now().timestamp() + 3500)
        .bind(chrono::Utc::now().timestamp())
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn authenticate_gmail(app_handle: tauri::AppHandle) -> Result<(), String> {
    start_oauth_flow(app_handle).await
}

#[derive(serde::Serialize, Clone)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: String,
    pub is_active: bool,
}

#[derive(serde::Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub active_account: Option<AccountInfo>,
    pub accounts: Vec<AccountInfo>,
}

#[tauri::command]
pub async fn check_auth_status(app_handle: tauri::AppHandle) -> Result<AuthStatus, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct AccountRow {
        id: String,
        email: Option<String>,
        display_name: Option<String>,
        avatar_url: Option<String>,
        token_expiry: Option<i64>,
        is_active: Option<i32>,
    }

    let all_accounts: Vec<AccountRow> = sqlx::query_as("SELECT id, email, display_name, avatar_url, token_expiry, is_active FROM accounts")
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    if all_accounts.is_empty() {
        return Ok(AuthStatus {
            authenticated: false,
            active_account: None,
            accounts: vec![],
        });
    }

    let accounts_info: Vec<AccountInfo> = all_accounts
        .iter()
        .map(|a| AccountInfo {
            id: a.id.clone(),
            email: a.email.clone().unwrap_or_default(),
            display_name: a.display_name.clone().unwrap_or_default(),
            avatar_url: a.avatar_url.clone().unwrap_or_default(),
            is_active: a.is_active.unwrap_or(0) == 1,
        })
        .collect();

    let active = all_accounts.iter().find(|a| a.is_active.unwrap_or(0) == 1);
    let active = match active {
        Some(a) => a,
        None => {
            let first_id = &all_accounts[0].id;
            let _ = sqlx::query("UPDATE accounts SET is_active = 1 WHERE id = ?")
                .bind(first_id)
                .execute(pool.inner())
                .await;
            &all_accounts[0]
        }
    };

    let now = chrono::Utc::now().timestamp();
    let expiry = active.token_expiry.unwrap_or(0);

    if expiry > now {
        let active_info = accounts_info
            .iter()
            .find(|a| a.is_active)
            .cloned()
            .unwrap_or_else(|| accounts_info[0].clone());
        return Ok(AuthStatus {
            authenticated: true,
            active_account: Some(active_info),
            accounts: accounts_info,
        });
    }

    let token_to_use = crate::credentials::get_refresh_token(&active.id).unwrap_or_default();

    if token_to_use.is_empty() {
        return Ok(AuthStatus {
            authenticated: false,
            active_account: None,
            accounts: accounts_info,
        });
    }

    match refresh_and_update(pool.inner(), &active.id, &token_to_use).await {
        Ok(_) => {
            let active_info = accounts_info
                .iter()
                .find(|a| a.is_active)
                .cloned()
                .unwrap_or_else(|| accounts_info[0].clone());
            Ok(AuthStatus {
                authenticated: true,
                active_account: Some(active_info),
                accounts: accounts_info,
            })
        }
        Err(_) => Ok(AuthStatus {
            authenticated: false,
            active_account: None,
            accounts: accounts_info,
        }),
    }
}

async fn refresh_and_update(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    refresh_token: &str,
) -> Result<(), String> {
    let (client_id, client_secret) = get_client_credentials()?;

    let client = reqwest::Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let error_body: serde_json::Value = res.json().await.unwrap_or_default();
        let error_code = error_body["error"].as_str().unwrap_or("");
        if error_code == "invalid_grant" {
            let _ = crate::credentials::delete_tokens(account_id);
            return Err("invalid_grant: Please re-authenticate your account.".to_string());
        }
        return Err(format!("Token refresh failed: {}", error_body));
    }

    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let new_access_token = body["access_token"].as_str().unwrap_or_default();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3500);
    let new_expiry = chrono::Utc::now().timestamp() + expires_in;

    crate::credentials::update_access_token(account_id, new_access_token)?;

    let http = reqwest::Client::new();
    if let Ok(profile_res) = http
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {}", new_access_token))
        .send()
        .await
    {
        if let Ok(profile) = profile_res.json::<serde_json::Value>().await {
            let name = profile["name"].as_str().unwrap_or("");
            let picture = profile["picture"].as_str().unwrap_or("");
            let email = profile["email"].as_str().unwrap_or("");
            sqlx::query("UPDATE accounts SET token_expiry = ?, display_name = COALESCE(NULLIF(display_name, ''), ?), avatar_url = COALESCE(NULLIF(avatar_url, ''), ?), email = COALESCE(NULLIF(email, 'unknown@gmail.com'), ?) WHERE id = ?")
                .bind(new_expiry)
                .bind(name)
                .bind(picture)
                .bind(email)
                .bind(account_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    sqlx::query("UPDATE accounts SET token_expiry = ? WHERE id = ?")
        .bind(new_expiry)
        .bind(account_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_accounts(app_handle: tauri::AppHandle) -> Result<Vec<AccountInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        email: Option<String>,
        display_name: Option<String>,
        avatar_url: Option<String>,
        is_active: Option<i32>,
    }

    let rows: Vec<Row> = sqlx::query_as("SELECT id, email, display_name, avatar_url, is_active FROM accounts ORDER BY created_at ASC")
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| AccountInfo {
            id: r.id,
            email: r.email.unwrap_or_default(),
            display_name: r.display_name.unwrap_or_default(),
            avatar_url: r.avatar_url.unwrap_or_default(),
            is_active: r.is_active.unwrap_or(0) == 1,
        })
        .collect())
}

#[tauri::command]
pub async fn switch_account(
    app_handle: tauri::AppHandle,
    account_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    sqlx::query("UPDATE accounts SET is_active = 0")
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE accounts SET is_active = 1 WHERE id = ?")
        .bind(&account_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_account(
    app_handle: tauri::AppHandle,
    account_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    let _ = crate::credentials::delete_tokens(&account_id);

    let mut tx = pool.inner().begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM messages WHERE account_id = ?")
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM threads WHERE account_id = ?")
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM labels WHERE account_id = ?")
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(&account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    let _ = sqlx::query("UPDATE accounts SET is_active = 1 WHERE rowid = (SELECT MIN(rowid) FROM accounts WHERE is_active = 0)")
        .execute(pool.inner())
        .await;

    Ok(())
}

#[derive(serde::Serialize)]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> Result<Vec<SettingEntry>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct Row {
        key: String,
        value: String,
    }

    let rows: Vec<Row> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| SettingEntry {
            key: r.key,
            value: r.value,
        })
        .collect())
}

#[tauri::command]
pub async fn get_setting(app_handle: tauri::AppHandle, key: String) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(&key)
        .fetch_optional(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.0).unwrap_or_default())
}

#[tauri::command]
pub async fn update_setting(
    app_handle: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(&key)
        .bind(&value)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn sync_gmail_data(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    println!("[Sync] Starting fast sync for: {}", account.id);

    crate::gmail_api::fetch_and_store_labels(pool.inner(), &account.id, &account.access_token)
        .await?;

    let target_labels = if let Some(ref lid) = label_id {
        vec![lid.as_str()]
    } else {
        vec!["INBOX"]
    };

    crate::gmail_api::fetch_and_store_threads(
        pool.inner(),
        &account.id,
        &account.access_token,
        Some(&target_labels),
        100,
    )
    .await?;

    #[derive(sqlx::FromRow)]
    struct PrefetchSetting { value: String }
    let prefetch = sqlx::query_as::<_, PrefetchSetting>("SELECT value FROM settings WHERE key = 'prefetch_bodies'")
        .fetch_optional(pool.inner())
        .await
        .unwrap_or(None)
        .map(|r| r.value == "true")
        .unwrap_or(false);

    let unhydrated = crate::gmail_api::get_unhydrated_thread_ids(pool.inner(), &account.id).await;
    if !unhydrated.is_empty() {
        let limit = if prefetch { unhydrated.len() } else { 100 };
        let batch: Vec<String> = unhydrated.into_iter().take(limit).collect();
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            batch,
        )
        .await;
    }

    let bg_pool = pool.inner().clone();
    let bg_account_id = account.id.clone();
    let bg_token = account.access_token.clone();
    let bg_app = app_handle.clone();
    tokio::spawn(async move {
        let all_unhydrated =
            crate::gmail_api::get_unhydrated_thread_ids(&bg_pool, &bg_account_id).await;
        if !all_unhydrated.is_empty() {
            crate::gmail_api::batch_hydrate_threads(
                &bg_pool,
                &bg_account_id,
                &bg_token,
                all_unhydrated,
            )
            .await;
        }

        let app_dir = bg_app.path().app_data_dir().unwrap_or_default();
        let db_path = app_dir.join("rustymail.db");
        let db_size_mb = std::fs::metadata(&db_path)
            .map(|m| m.len() / (1024 * 1024))
            .unwrap_or(0);
        #[derive(sqlx::FromRow)]
        struct S {
            value: String,
        }
        let max_mb: u64 =
            sqlx::query_as::<_, S>("SELECT value FROM settings WHERE key = 'max_cache_mb'")
                .fetch_optional(&bg_pool)
                .await
                .unwrap_or(None)
                .and_then(|r| r.value.parse().ok())
                .unwrap_or(500);
        if db_size_mb > max_mb {
            crate::gmail_api::evict_old_message_bodies(&bg_pool, &bg_account_id, 200).await;
        }
    });

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalLabel {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub unread_count: i32,
}

#[tauri::command]
pub async fn get_labels(app_handle: tauri::AppHandle) -> Result<Vec<LocalLabel>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct LabelRow {
        id: String,
        name: Option<String>,
        r#type: Option<String>,
        unread_count: Option<i32>,
    }

    let rows: Vec<LabelRow> = sqlx::query_as(
        "SELECT id, name, type, unread_count FROM labels 
         WHERE account_id = ? 
         AND UPPER(id) NOT IN ('YELLOW_STAR', 'CHAT', 'VOICEMAIL')
         AND UPPER(name) NOT IN ('YELLOW_STAR', 'YELLOW STAR', 'CHAT', 'VOICEMAIL')
         ORDER BY CASE WHEN type = 'system' THEN 0 ELSE 1 END, name ASC",
    )
    .bind(&account.id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| LocalLabel {
            id: r.id,
            name: r.name.unwrap_or_default(),
            r#type: r.r#type.unwrap_or_default(),
            unread_count: r.unread_count.unwrap_or(0),
        })
        .collect())
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalThread {
    pub id: String,
    pub snippet: String,
    pub history_id: String,
    pub unread: i32,
    pub sender: String,
    pub subject: String,
    pub internal_date: i64,
    pub starred: bool,
}

fn clean_sender_name(raw: Option<String>) -> String {
    let mut s = raw.unwrap_or_else(|| "Unknown Sender".to_string());
    if let Some(idx) = s.find('<') {
        let name = s[..idx].trim();
        if !name.is_empty() {
            s = name.to_string();
        } else {
            s = s.replace("<", "").replace(">", "").trim().to_string();
        }
    }
    s.replace("\"", "")
}

#[tauri::command]
pub async fn get_threads(
    app_handle: tauri::AppHandle,
    label_id: Option<String>,
    offset: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let lim = limit.unwrap_or(50);
    let off = offset.unwrap_or(0);

    #[derive(sqlx::FromRow)]
    struct TR {
        id: String,
        snippet: Option<String>,
        history_id: Option<String>,
        unread: Option<i32>,
        sender: Option<String>,
        subject: Option<String>,
        msg_date: Option<i64>,
        starred: Option<i32>,
    }

    let rows: Vec<TR> = if let Some(ref lid) = label_id {
        sqlx::query_as(
            "SELECT t.id, t.snippet, t.history_id, t.unread,
                    (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                    (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                    (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date,
                    EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred
             FROM threads t
             INNER JOIN thread_labels tl ON t.id = tl.thread_id AND tl.label_id = ?
             WHERE t.account_id = ?
             ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC, t.rowid DESC
             LIMIT ? OFFSET ?"
        ).bind(lid).bind(&account.id).bind(lim).bind(off)
        .fetch_all(pool.inner()).await.map_err(|e| e.to_string())?
    } else {
        sqlx::query_as(
            "SELECT t.id, t.snippet, t.history_id, t.unread,
                    (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                    (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                    (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date,
                    EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred
             FROM threads t
             WHERE t.account_id = ?
             ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC, t.rowid DESC
             LIMIT ? OFFSET ?"
        ).bind(&account.id).bind(lim).bind(off)
        .fetch_all(pool.inner()).await.map_err(|e| e.to_string())?
    };

    Ok(rows
        .into_iter()
        .map(|r| LocalThread {
            id: r.id,
            snippet: r.snippet.unwrap_or_default(),
            history_id: r.history_id.unwrap_or_default(),
            unread: r.unread.unwrap_or(0),
            sender: clean_sender_name(r.sender),
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            internal_date: r.msg_date.unwrap_or(0),
            starred: r.starred.unwrap_or(0) == 1,
        })
        .collect())
}

#[tauri::command]
pub async fn fetch_label_threads(
    app_handle: tauri::AppHandle,
    label_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    println!("[OnDemand] Fetching threads for label: {}", label_id);
    crate::gmail_api::fetch_and_store_threads(
        pool.inner(),
        &account.id,
        &account.access_token,
        Some(&[label_id.as_str()]),
        50,
    )
    .await?;

    #[derive(sqlx::FromRow)]
    struct PrefetchVal { value: String }
    let prefetch = sqlx::query_as::<_, PrefetchVal>("SELECT value FROM settings WHERE key = 'prefetch_bodies'")
        .fetch_optional(pool.inner())
        .await
        .unwrap_or(None)
        .map(|r| r.value == "true")
        .unwrap_or(false);

    let unhydrated = crate::gmail_api::get_unhydrated_thread_ids(pool.inner(), &account.id).await;
    if !unhydrated.is_empty() {
        let limit = if prefetch { unhydrated.len() } else { 50 };
        let batch: Vec<String> = unhydrated.into_iter().take(limit).collect();
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            batch,
        )
        .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn sync_thread_messages(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::fetch_messages_for_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
    )
    .await
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalMessage {
    pub id: String,
    pub thread_id: String,
    pub sender: String,
    pub recipients: String,
    pub subject: String,
    pub snippet: String,
    pub internal_date: i64,
    pub body_html: String,
    pub body_plain: String,
    pub is_draft: bool,
}

#[tauri::command]
pub async fn get_messages(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<Vec<LocalMessage>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        thread_id: Option<String>,
        sender: Option<String>,
        recipients: Option<String>,
        subject: Option<String>,
        snippet: Option<String>,
        internal_date: Option<i64>,
        body_html: Option<String>,
        body_plain: Option<String>,
        is_draft: bool,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.id, m.thread_id, m.sender, m.recipients, m.subject, m.snippet, m.internal_date, m.body_html, m.body_plain, 
         EXISTS(SELECT 1 FROM message_labels ml WHERE ml.message_id = m.id AND ml.label_id = 'DRAFT') as is_draft
         FROM messages m WHERE m.thread_id = ? ORDER BY m.internal_date ASC"
    ).bind(thread_id).fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| LocalMessage {
            id: r.id,
            thread_id: r.thread_id.unwrap_or_default(),
            sender: r.sender.unwrap_or_default(),
            recipients: r.recipients.unwrap_or_default(),
            subject: r.subject.unwrap_or_default(),
            snippet: r.snippet.unwrap_or_default(),
            internal_date: r.internal_date.unwrap_or(0),
            body_plain: r.body_plain.unwrap_or_default(),
            body_html: r.body_html.unwrap_or_default(),
            is_draft: r.is_draft,
        })
        .collect())
}

#[tauri::command]
pub async fn archive_thread(app_handle: tauri::AppHandle, thread_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        vec![],
        vec!["INBOX".to_string()],
    )
    .await
}

#[tauri::command]
pub async fn move_thread_to_trash(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::trash_thread(pool.inner(), &account.id, &account.access_token, &thread_id)
        .await
}

#[tauri::command]
pub async fn untrash_thread(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::untrash_thread(&account.access_token, &thread_id).await?;
    sqlx::query(
        "INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0) ON CONFLICT(id) DO NOTHING"
    )
    .bind(&thread_id)
    .bind(&account.id)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM thread_labels WHERE thread_id = ?")
        .bind(&thread_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    crate::gmail_api::fetch_messages_for_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn mark_thread_read_status(
    app_handle: tauri::AppHandle,
    thread_id: String,
    is_read: bool,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let (add, remove) = if is_read {
        (vec![], vec!["UNREAD".to_string()])
    } else {
        (vec!["UNREAD".to_string()], vec![])
    };
    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        add,
        remove,
    )
    .await
}

struct ParsedQuery {
    from: Option<String>,
    to: Option<String>,
    subject: Option<String>,
    has_attachment: bool,
    is_unread: Option<bool>,
    free_text: String,
}

fn parse_query_operators(query: &str) -> ParsedQuery {
    let mut from = None;
    let mut to = None;
    let mut subject = None;
    let mut has_attachment = false;
    let mut is_unread = None;
    let mut free_parts = Vec::new();

    let mut chars = query.chars().peekable();
    let mut tokens = Vec::new();
    let mut current = String::new();

    // Tokenize: split on spaces but respect quoted strings
    while let Some(c) = chars.next() {
        if c == '"' {
            current.push(c);
            for c2 in chars.by_ref() {
                current.push(c2);
                if c2 == '"' { break; }
            }
        } else if c == ' ' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    for token in tokens {
        if let Some(val) = token.strip_prefix("from:") {
            if !val.is_empty() { from = Some(val.trim_matches('"').to_string()); }
        } else if let Some(val) = token.strip_prefix("to:") {
            if !val.is_empty() { to = Some(val.trim_matches('"').to_string()); }
        } else if let Some(val) = token.strip_prefix("subject:") {
            if !val.is_empty() { subject = Some(val.trim_matches('"').to_string()); }
        } else if token == "has:attachment" {
            has_attachment = true;
        } else if token == "has:link" {
            // Gmail handles this, not stored locally
            free_parts.push(token);
        } else if token == "is:unread" {
            is_unread = Some(true);
        } else if token == "is:read" {
            is_unread = Some(false);
        } else if token.starts_with("is:") || token.starts_with("before:") || token.starts_with("after:") {
            // Pass through to Gmail API as free text
            free_parts.push(token);
        } else {
            free_parts.push(token);
        }
    }

    ParsedQuery {
        from,
        to,
        subject,
        has_attachment,
        is_unread,
        free_text: free_parts.join(" "),
    }
}

#[tauri::command]
pub async fn search_messages(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let mut all_thread_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let parsed = parse_query_operators(&query);

    // Build targeted SQL conditions from operators
    let has_operators = parsed.from.is_some() || parsed.to.is_some() || parsed.subject.is_some() || parsed.has_attachment || parsed.is_unread.is_some();

    if has_operators {
        let mut conditions = vec!["m.account_id = ?".to_string()];
        let mut binds: Vec<String> = vec![account.id.clone()];

        if let Some(ref f) = parsed.from {
            conditions.push("m.sender LIKE ?".to_string());
            binds.push(format!("%{}%", f));
        }
        if let Some(ref t) = parsed.to {
            conditions.push("m.recipients LIKE ?".to_string());
            binds.push(format!("%{}%", t));
        }
        if let Some(ref s) = parsed.subject {
            conditions.push("m.subject LIKE ?".to_string());
            binds.push(format!("%{}%", s));
        }
        if parsed.has_attachment {
            conditions.push("m.has_attachments = 1".to_string());
        }
        if let Some(unread) = parsed.is_unread {
            conditions.push("m.is_read = ?".to_string());
            binds.push(if unread { "0".to_string() } else { "1".to_string() });
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT DISTINCT m.thread_id FROM messages m WHERE {} LIMIT 50",
            where_clause
        );

        #[derive(sqlx::FromRow)]
        struct TidRow { thread_id: Option<String> }

        let mut q = sqlx::query_as::<_, TidRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        let rows: Vec<TidRow> = q.fetch_all(pool.inner()).await.unwrap_or_default();
        for r in rows {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }
    }

    // FTS5 for free text remainder
    if !parsed.free_text.is_empty() {
        #[derive(sqlx::FromRow)]
        struct FtsRow { thread_id: Option<String> }
        let fts_query = format!("{}*", parsed.free_text.replace('"', ""));
        let local: Vec<FtsRow> = sqlx::query_as(
            "SELECT DISTINCT m.thread_id FROM messages m
             INNER JOIN messages_fts ON messages_fts.rowid = m.rowid
             WHERE messages_fts MATCH ? AND m.account_id = ?
             LIMIT 50",
        )
        .bind(&fts_query)
        .bind(&account.id)
        .fetch_all(pool.inner())
        .await
        .unwrap_or_default();

        for r in local {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }

        // LIKE fallback for free text
        #[derive(sqlx::FromRow)]
        struct LikeRow { thread_id: Option<String> }
        let pattern = format!("%{}%", parsed.free_text);
        let like_results: Vec<LikeRow> = sqlx::query_as(
            "SELECT DISTINCT thread_id FROM messages WHERE account_id = ? AND (sender LIKE ? OR subject LIKE ?) LIMIT 30"
        ).bind(&account.id).bind(&pattern).bind(&pattern)
        .fetch_all(pool.inner()).await.unwrap_or_default();
        for r in like_results {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }
    }

    // Gmail API search with the full original query (already supports operators)
    let api_ids = search_gmail_api(&account.access_token, &query).await;
    for tid in api_ids {
        if seen.insert(tid.clone()) {
            all_thread_ids.push(tid);
        }
    }

    let mut need_hydrate: Vec<String> = Vec::new();
    for tid in &all_thread_ids {
        #[derive(sqlx::FromRow)]
        struct C { cnt: i32 }
        let cnt =
            sqlx::query_as::<_, C>("SELECT COUNT(*) as cnt FROM messages WHERE thread_id = ?")
                .bind(tid)
                .fetch_one(pool.inner())
                .await
                .map(|r| r.cnt)
                .unwrap_or(0);
        if cnt == 0 {
            let _ = sqlx::query("INSERT OR IGNORE INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)")
                .bind(tid).bind(&account.id)
                .execute(pool.inner()).await;
            need_hydrate.push(tid.clone());
        }
    }
    if !need_hydrate.is_empty() {
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            need_hydrate,
        )
        .await;
    }

    fetch_threads_by_ids(pool.inner(), &all_thread_ids, &account.id).await
}

async fn search_gmail_api(access_token: &str, query: &str) -> Vec<String> {
    let client = reqwest::Client::new();
    let res = match client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
        .query(&[("q", query), ("maxResults", "30")])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    if !res.status().is_success() {
        return vec![];
    }

    #[derive(serde::Deserialize)]
    struct MsgRef {
        #[allow(dead_code)]
        id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    }
    #[derive(serde::Deserialize)]
    struct MsgsResponse {
        messages: Option<Vec<MsgRef>>,
    }

    match res.json::<MsgsResponse>().await {
        Ok(api_res) => {
            if let Some(msgs) = api_res.messages {
                let mut seen = std::collections::HashSet::new();
                msgs.into_iter()
                    .filter(|m| seen.insert(m.thread_id.clone()))
                    .map(|m| m.thread_id)
                    .collect()
            } else {
                vec![]
            }
        }
        Err(_) => vec![],
    }
}

async fn fetch_threads_by_ids(
    pool: &sqlx::SqlitePool,
    ids: &[String],
    account_id: &str,
) -> Result<Vec<LocalThread>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT t.id, t.snippet, t.history_id, t.unread,
                (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date,
                EXISTS (SELECT 1 FROM thread_labels tl WHERE tl.thread_id = t.id AND tl.label_id = 'STARRED') as starred
         FROM threads t
         WHERE t.id IN ({}) AND t.account_id = ?
         ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC",
        placeholders.join(",")
    );

    #[derive(sqlx::FromRow)]
    struct TR {
        id: String,
        snippet: Option<String>,
        history_id: Option<String>,
        unread: Option<i32>,
        sender: Option<String>,
        subject: Option<String>,
        msg_date: Option<i64>,
        starred: Option<i32>,
    }

    let mut q = sqlx::query_as::<_, TR>(&sql);
    for tid in ids {
        q = q.bind(tid);
    }
    q = q.bind(account_id);

    let rows = q.fetch_all(pool).await.unwrap_or_default();
    Ok(rows
        .into_iter()
        .map(|r| LocalThread {
            id: r.id,
            snippet: r.snippet.unwrap_or_default(),
            history_id: r.history_id.unwrap_or_default(),
            unread: r.unread.unwrap_or(0),
            sender: clean_sender_name(r.sender),
            subject: r.subject.unwrap_or_else(|| "No Subject".to_string()),
            internal_date: r.msg_date.unwrap_or(0),
            starred: r.starred.unwrap_or(0) == 1,
        })
        .collect())
}

#[derive(serde::Serialize)]
pub struct HydrationProgress {
    pub total: usize,
    pub hydrated: usize,
}

#[tauri::command]
pub async fn get_hydration_progress(
    app_handle: tauri::AppHandle,
) -> Result<HydrationProgress, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct Count {
        cnt: i32,
    }

    let total =
        sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM threads WHERE account_id = ?")
            .bind(&account.id)
            .fetch_one(pool.inner())
            .await
            .map(|r| r.cnt)
            .unwrap_or(0) as usize;

    let hydrated = sqlx::query_as::<_, Count>(
        "SELECT COUNT(DISTINCT t.id) as cnt FROM threads t INNER JOIN messages m ON t.id = m.thread_id WHERE t.account_id = ?"
    ).bind(&account.id).fetch_one(pool.inner()).await.map(|r| r.cnt).unwrap_or(0) as usize;

    Ok(HydrationProgress { total, hydrated })
}

#[tauri::command]
pub async fn ensure_threads_hydrated(
    app_handle: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    let mut need_hydration = Vec::new();
    for tid in &thread_ids {
        #[derive(sqlx::FromRow)]
        struct Count {
            cnt: i32,
        }
        let has_msgs =
            sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM messages WHERE thread_id = ?")
                .bind(tid)
                .fetch_one(pool.inner())
                .await
                .map(|r| r.cnt)
                .unwrap_or(0);
        if has_msgs == 0 {
            need_hydration.push(tid.clone());
        }
    }
    if !need_hydration.is_empty() {
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(),
            &account.id,
            &account.access_token,
            need_hydration,
        )
        .await;
    }

    Ok(())
}

#[derive(serde::Serialize)]
pub struct SearchSuggestion {
    pub kind: String,
    pub text: String,
    pub detail: String,
}

#[tauri::command]
pub async fn get_search_suggestions(
    app_handle: tauri::AppHandle,
    operator: Option<String>,
    value: String,
    full_query: String,
) -> Result<Vec<SearchSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let mut suggestions = Vec::new();

    match operator.as_deref() {
        Some("from") => {
            if value.len() >= 1 {
                #[derive(sqlx::FromRow)]
                struct SenderRow { sender: String }
                let pattern = format!("%{}%", value);
                let contacts: Vec<SenderRow> = sqlx::query_as(
                    "SELECT DISTINCT sender FROM messages WHERE account_id = ? AND sender LIKE ? LIMIT 8",
                )
                .bind(&account.id)
                .bind(&pattern)
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();

                for c in contacts {
                    let display = c.sender.split('<').next().unwrap_or(&c.sender).trim().to_string();
                    suggestions.push(SearchSuggestion {
                        kind: "contact".to_string(),
                        text: display.clone(),
                        detail: c.sender.clone(),
                    });
                }
            }
        }
        Some("to") => {
            if value.len() >= 1 {
                #[derive(sqlx::FromRow)]
                struct RecipRow { recipients: String }
                let pattern = format!("%{}%", value);
                let rows: Vec<RecipRow> = sqlx::query_as(
                    "SELECT DISTINCT recipients FROM messages WHERE account_id = ? AND recipients LIKE ? LIMIT 20",
                )
                .bind(&account.id)
                .bind(&pattern)
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();

                let mut seen = std::collections::HashSet::new();
                for row in rows {
                    for p in row.recipients.split(',') {
                        let p = p.trim();
                        if p.is_empty() || !p.to_lowercase().contains(&value.to_lowercase()) {
                            continue;
                        }
                        if !seen.insert(p.to_string()) {
                            continue;
                        }
                        let display = if let Some(bracket_start) = p.find('<') {
                            p[..bracket_start].trim().trim_matches('"').to_string()
                        } else {
                            p.to_string()
                        };
                        suggestions.push(SearchSuggestion {
                            kind: "contact".to_string(),
                            text: display,
                            detail: p.to_string(),
                        });
                        if suggestions.len() >= 8 { break; }
                    }
                    if suggestions.len() >= 8 { break; }
                }
            }
        }
        Some("subject") => {
            if value.len() >= 1 {
                #[derive(sqlx::FromRow)]
                struct SubjectRow { subject: String }
                let pattern = format!("%{}%", value);
                let subjects: Vec<SubjectRow> = sqlx::query_as(
                    "SELECT DISTINCT subject FROM messages WHERE account_id = ? AND subject LIKE ? LIMIT 8",
                )
                .bind(&account.id)
                .bind(&pattern)
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();

                for s in subjects {
                    suggestions.push(SearchSuggestion {
                        kind: "subject".to_string(),
                        text: s.subject.clone(),
                        detail: String::new(),
                    });
                }
            }
        }
        _ => {
            // Free text: existing behavior using full_query
            #[derive(sqlx::FromRow)]
            struct SettingRow { value: String }
            if let Ok(Some(row)) =
                sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
                    .fetch_optional(pool.inner())
                    .await
            {
                if let Ok(recents) = serde_json::from_str::<Vec<String>>(&row.value) {
                    for r in recents.iter().take(5) {
                        if full_query.is_empty() || r.to_lowercase().contains(&full_query.to_lowercase()) {
                            suggestions.push(SearchSuggestion {
                                kind: "recent".to_string(),
                                text: r.clone(),
                                detail: "Recent search".to_string(),
                            });
                        }
                    }
                }
            }

            if full_query.len() >= 2 {
                #[derive(sqlx::FromRow)]
                struct SenderRow { sender: String }
                let pattern = format!("%{}%", full_query);
                let contacts: Vec<SenderRow> = sqlx::query_as(
                    "SELECT DISTINCT sender FROM messages WHERE account_id = ? AND sender LIKE ? LIMIT 5",
                )
                .bind(&account.id)
                .bind(&pattern)
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();

                for c in contacts {
                    suggestions.push(SearchSuggestion {
                        kind: "contact".to_string(),
                        text: format!("from:{}", c.sender.split('<').next().unwrap_or(&c.sender).trim()),
                        detail: c.sender.clone(),
                    });
                }

                #[derive(sqlx::FromRow)]
                struct SubjectRow { subject: String }
                let subjects: Vec<SubjectRow> = sqlx::query_as(
                    "SELECT DISTINCT subject FROM messages WHERE account_id = ? AND subject LIKE ? LIMIT 3",
                )
                .bind(&account.id)
                .bind(&pattern)
                .fetch_all(pool.inner())
                .await
                .unwrap_or_default();

                for s in subjects {
                    suggestions.push(SearchSuggestion {
                        kind: "subject".to_string(),
                        text: format!("subject:{}", s.subject),
                        detail: s.subject.clone(),
                    });
                }
            }
        }
    }

    Ok(suggestions)
}

#[tauri::command]
pub async fn save_recent_search(app_handle: tauri::AppHandle, query: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    #[derive(sqlx::FromRow)]
    struct SettingRow {
        value: String,
    }
    let mut recents: Vec<String> =
        sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
            .fetch_optional(pool.inner())
            .await
            .unwrap_or(None)
            .and_then(|r| serde_json::from_str(&r.value).ok())
            .unwrap_or_default();

    recents.retain(|r| r != &query);
    recents.insert(0, query);
    recents.truncate(10);

    let json = serde_json::to_string(&recents).unwrap_or_default();
    sqlx::query("INSERT INTO settings (key, value) VALUES ('recent_searches', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(&json).execute(pool.inner()).await.map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn toggle_thread_star(
    app_handle: tauri::AppHandle,
    thread_id: String,
    starred: bool,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let (add, remove) = if starred {
        (vec!["STARRED".to_string()], vec![])
    } else {
        (vec![], vec!["STARRED".to_string()])
    };
    crate::gmail_api::modify_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
        add,
        remove,
    )
    .await
}

#[tauri::command]
pub async fn send_message(
    app_handle: tauri::AppHandle,
    to: String,
    subject: String,
    body: String,
    thread_id: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow {
        email: String,
    }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    crate::gmail_api::send_message(
        &account.id,
        &row.email,
        &account.access_token,
        &to,
        &subject,
        &body,
        thread_id.as_deref(),
        in_reply_to.as_deref(),
        references.as_deref(),
    )
    .await
}

#[derive(serde::Serialize)]
pub struct ContactSuggestion {
    pub name: String,
    pub email: String,
    pub raw: String,
}

#[tauri::command]
pub async fn search_contacts(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<ContactSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let pattern = format!("%{}%", query);

    #[derive(sqlx::FromRow)]
    struct RawContact {
        contact: String,
    }

    let rows: Vec<RawContact> = sqlx::query_as(
        "SELECT DISTINCT sender as contact FROM messages WHERE account_id = ? AND sender LIKE ?
         UNION
         SELECT DISTINCT recipients as contact FROM messages WHERE account_id = ? AND recipients LIKE ?
         LIMIT 20"
    )
    .bind(&account.id).bind(&pattern)
    .bind(&account.id).bind(&pattern)
    .fetch_all(pool.inner()).await.unwrap_or_default();

    let mut seen = std::collections::HashSet::new();
    let mut suggestions = Vec::new();

    for row in rows {
        let parts: Vec<&str> = row.contact.split(',').collect();
        for p in parts {
            let p = p.trim();
            if p.is_empty() || !p.to_lowercase().contains(&query.to_lowercase()) {
                continue;
            }
            if !seen.insert(p.to_string()) {
                continue;
            }

            let (name, email) = if let Some(bracket_start) = p.find('<') {
                let name = p[..bracket_start].trim().trim_matches('"').to_string();
                let email = p[bracket_start + 1..].trim_matches('>').trim().to_string();
                (name, email)
            } else {
                ("".to_string(), p.to_string())
            };

            suggestions.push(ContactSuggestion {
                name,
                email,
                raw: p.to_string(),
            });
        }
    }

    suggestions.sort_by(|a, b| a.email.len().cmp(&b.email.len()));
    suggestions.truncate(10);
    Ok(suggestions)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn save_draft(
    app_handle: tauri::AppHandle,
    to: String,
    subject: String,
    body: String,
    thread_id: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
    draft_id: Option<String>,
) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow {
        email: String,
    }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    let new_draft_id = crate::gmail_api::save_draft(
        &account.id,
        &row.email,
        &account.access_token,
        &to,
        &subject,
        &body,
        thread_id.as_deref(),
        in_reply_to.as_deref(),
        references.as_deref(),
        draft_id.as_deref(),
    )
    .await?;

    // Clean up stale local draft messages for this thread.
    // Gmail changes the message ID on each draft update, leaving orphaned
    // records in the local DB. Delete local draft-labeled messages for the thread
    // so re-sync picks up fresh data without duplicates.
    if let Some(ref tid) = thread_id {
        let _ = sqlx::query(
            "DELETE FROM messages WHERE thread_id = ? AND id IN (
                SELECT message_id FROM message_labels WHERE label_id = 'DRAFT'
            )",
        )
        .bind(tid)
        .execute(pool.inner())
        .await;
    }

    Ok(new_draft_id)
}

#[tauri::command]
pub async fn delete_draft(app_handle: tauri::AppHandle, draft_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow {
        email: String,
    }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    crate::gmail_api::delete_draft(
        pool.inner(),
        &account.id,
        &row.email,
        &account.access_token,
        &draft_id,
    )
    .await
}

#[tauri::command]
pub async fn delete_draft_by_thread(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    crate::gmail_api::delete_draft_by_thread(
        pool.inner(),
        &account.id,
        &account.access_token,
        &thread_id,
    )
    .await
}

#[tauri::command]
pub async fn get_draft_id_by_message_id(
    app_handle: tauri::AppHandle,
    message_id: String,
) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    crate::gmail_api::get_draft_id_by_message_id(&account.id, &account.access_token, &message_id)
        .await
}

#[tauri::command]
pub async fn get_upcoming_events(
    app_handle: tauri::AppHandle,
) -> Result<Vec<crate::calendar_api::CalendarEvent>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::calendar_api::get_upcoming_events(&account.access_token).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
    use std::env;
    use std::str::FromStr;

    // Helper to create an in-memory DB with schema for refresh and FTS5 tests.
    // Using sqlite::memory: avoids tempdir lifetime issues.
    async fn setup_test_db() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePool::connect_with(options).await.unwrap();
        // Schema: accounts, messages, and FTS5 virtual table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                email TEXT,
                display_name TEXT,
                avatar_url TEXT,
                token_expiry INTEGER,
                is_active INTEGER DEFAULT 1,
                created_at INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                thread_id TEXT,
                account_id TEXT,
                sender TEXT,
                recipients TEXT,
                subject TEXT,
                snippet TEXT,
                internal_date INTEGER,
                body_plain TEXT,
                body_html TEXT,
                has_attachments INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(sender, subject, body_plain, content=messages, content_rowid=rowid)"
        ).execute(&pool).await.unwrap();
        pool
    }

    #[test]
    fn test_clean_sender_name() {
        assert_eq!(
            clean_sender_name(Some("John Doe <john@example.com>".to_string())),
            "John Doe"
        );
        assert_eq!(
            clean_sender_name(Some("<only-email@example.com>".to_string())),
            "only-email@example.com"
        );
        assert_eq!(
            clean_sender_name(Some("\"John Doe\" <john@example.com>".to_string())),
            "John Doe"
        );
        assert_eq!(clean_sender_name(None), "Unknown Sender");
    }

    #[tokio::test]
    async fn test_refresh_and_update_missing_client_id() {
        // Ensure env vars are not set
        env::remove_var("RUSTYMAIL_CLIENT_ID");
        env::set_var("RUSTYMAIL_CLIENT_SECRET", "dummy_secret");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_and_update_missing_client_secret() {
        env::set_var("RUSTYMAIL_CLIENT_ID", "dummy_id");
        env::remove_var("RUSTYMAIL_CLIENT_SECRET");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_and_update_http_error() {
        // Set valid env vars but the request will fail because we are hitting the real Google endpoint.
        // The function should return an error indicating a network failure or token refresh failure.
        env::set_var("RUSTYMAIL_CLIENT_ID", "dummy_id");
        env::set_var("RUSTYMAIL_CLIENT_SECRET", "dummy_secret");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        // The exact error string may vary; we just ensure it is an Err.
        assert!(result.is_err());
    }
    #[tokio::test]
    async fn test_search_messages_fts5() {
        // Setup DB with messages and FTS5 virtual table
        let pool = setup_test_db().await;
        // Insert dummy account
        sqlx::query("INSERT INTO accounts (id) VALUES (?)")
            .bind("acc1")
            .execute(&pool)
            .await
            .unwrap();
        // Insert a message with searchable content
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("msg1")
            .bind("thread1")
            .bind("acc1")
            .bind("sender@example.com")
            .bind("recipient@example.com")
            .bind("Test Subject")
            .bind("snippet")
            .bind(0i64)
            .bind("This is a searchterm inside the body")
            .bind("")
            .bind(0)
            .execute(&pool)
            .await
            .unwrap();
        // FTS5 external-content tables must be populated manually after insert
        sqlx::query("INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?")
            .bind("msg1")
            .execute(&pool)
            .await
            .unwrap();
        // Run the same query as in search_messages for FTS5
        let fts_query = "searchterm*".to_string();
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT m.thread_id FROM messages m INNER JOIN messages_fts ON messages_fts.rowid = m.rowid WHERE messages_fts MATCH ? AND m.account_id = ?"
        )
        .bind(&fts_query)
        .bind("acc1")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "thread1");
    }

    #[tokio::test]
    async fn test_accounts_schema_has_no_token_columns() {
        let pool = setup_test_db().await;
        #[derive(sqlx::FromRow)]
        struct ColInfo { name: String }
        let cols: Vec<ColInfo> = sqlx::query_as("PRAGMA table_info(accounts)")
            .fetch_all(&pool)
            .await
            .unwrap();
        let col_names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert!(!col_names.contains(&"access_token"), "access_token should not be in accounts table");
        assert!(!col_names.contains(&"refresh_token"), "refresh_token should not be in accounts table");
        assert!(col_names.contains(&"id"));
        assert!(col_names.contains(&"token_expiry"));
    }
}
