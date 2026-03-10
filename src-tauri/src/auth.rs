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

#[allow(dead_code)] // used in tests
pub(crate) async fn get_active_account_row(pool: &sqlx::SqlitePool) -> Result<(String, Option<i64>), String> {
    let row = sqlx::query_as::<_, ActiveAccountRow>(
        "SELECT id, token_expiry FROM accounts WHERE is_active = 1 LIMIT 1"
    )
        .fetch_one(pool)
        .await
        .map_err(|e| format!("No active account found: {}", e))?;
    Ok((row.id, row.token_expiry))
}

#[allow(dead_code)] // used in tests
pub(crate) async fn check_auth_status_db(pool: &sqlx::SqlitePool) -> Result<Option<String>, String> {
    #[derive(sqlx::FromRow)]
    struct IdRow { id: String }
    let row = sqlx::query_as::<_, IdRow>(
        "SELECT id FROM accounts WHERE is_active = 1 LIMIT 1"
    )
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.id))
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

pub(crate) async fn get_accounts_inner(pool: &sqlx::SqlitePool) -> Result<Vec<AccountInfo>, String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        email: Option<String>,
        display_name: Option<String>,
        avatar_url: Option<String>,
        is_active: Option<i32>,
    }

    let rows: Vec<Row> = sqlx::query_as("SELECT id, email, display_name, avatar_url, is_active FROM accounts ORDER BY created_at ASC")
        .fetch_all(pool)
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
pub async fn get_accounts(app_handle: tauri::AppHandle) -> Result<Vec<AccountInfo>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_accounts_inner(pool.inner()).await
}

pub(crate) async fn switch_account_inner(pool: &sqlx::SqlitePool, account_id: &str) -> Result<(), String> {
    sqlx::query("UPDATE accounts SET is_active = 0")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE accounts SET is_active = 1 WHERE id = ?")
        .bind(account_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn switch_account(
    app_handle: tauri::AppHandle,
    account_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    switch_account_inner(pool.inner(), &account_id).await
}

pub(crate) async fn remove_account_inner(pool: &sqlx::SqlitePool, account_id: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM messages WHERE account_id = ?")
        .bind(account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM threads WHERE account_id = ?")
        .bind(account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM labels WHERE account_id = ?")
        .bind(account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(account_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    let _ = sqlx::query("UPDATE accounts SET is_active = 1 WHERE rowid = (SELECT MIN(rowid) FROM accounts WHERE is_active = 0)")
        .execute(pool)
        .await;

    Ok(())
}

#[tauri::command]
pub async fn remove_account(
    app_handle: tauri::AppHandle,
    account_id: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();

    let _ = crate::credentials::delete_tokens(&account_id);

    remove_account_inner(pool.inner(), &account_id).await
}

#[derive(serde::Serialize)]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
}

pub(crate) async fn get_settings_inner(pool: &sqlx::SqlitePool) -> Result<Vec<SettingEntry>, String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        key: String,
        value: String,
    }

    let rows: Vec<Row> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
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
pub async fn get_settings(app_handle: tauri::AppHandle) -> Result<Vec<SettingEntry>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_settings_inner(pool.inner()).await
}

pub(crate) async fn get_setting_inner(pool: &sqlx::SqlitePool, key: &str) -> Result<String, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.0).unwrap_or_default())
}

#[tauri::command]
pub async fn get_setting(app_handle: tauri::AppHandle, key: String) -> Result<String, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_setting_inner(pool.inner(), &key).await
}

pub(crate) async fn update_setting_inner(pool: &sqlx::SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn update_setting(
    app_handle: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    update_setting_inner(pool.inner(), &key, &value).await
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

pub(crate) async fn get_labels_inner(pool: &sqlx::SqlitePool, account_id: &str) -> Result<Vec<LocalLabel>, String> {
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
    .bind(account_id)
    .fetch_all(pool)
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

#[tauri::command]
pub async fn get_labels(app_handle: tauri::AppHandle) -> Result<Vec<LocalLabel>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    get_labels_inner(pool.inner(), &account.id).await
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

pub(crate) async fn get_threads_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    label_id: Option<&str>,
    offset: i32,
    limit: i32,
) -> Result<Vec<LocalThread>, String> {
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

    let rows: Vec<TR> = if let Some(lid) = label_id {
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
        ).bind(lid).bind(account_id).bind(limit).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?
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
        ).bind(account_id).bind(limit).bind(offset)
        .fetch_all(pool).await.map_err(|e| e.to_string())?
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
    get_threads_inner(pool.inner(), &account.id, label_id.as_deref(), off, lim).await
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

pub(crate) async fn get_messages_inner(
    pool: &sqlx::SqlitePool,
    thread_id: &str,
) -> Result<Vec<LocalMessage>, String> {
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
    ).bind(thread_id).fetch_all(pool).await.map_err(|e| e.to_string())?;

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
pub async fn get_messages(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<Vec<LocalMessage>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    get_messages_inner(pool.inner(), &thread_id).await
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

/// Performs local-only search: parse operators, FTS5 match, LIKE fallback.
/// Returns a deduplicated list of thread IDs found locally.
pub(crate) async fn search_messages_local(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    query: &str,
) -> Result<Vec<String>, String> {
    let mut all_thread_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let parsed = parse_query_operators(query);

    let has_operators = parsed.from.is_some() || parsed.to.is_some() || parsed.subject.is_some() || parsed.has_attachment || parsed.is_unread.is_some();

    if has_operators {
        let mut conditions = vec!["m.account_id = ?".to_string()];
        let mut binds: Vec<String> = vec![account_id.to_string()];

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
        let rows: Vec<TidRow> = q.fetch_all(pool).await.unwrap_or_default();
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
        .bind(account_id)
        .fetch_all(pool)
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
        ).bind(account_id).bind(&pattern).bind(&pattern)
        .fetch_all(pool).await.unwrap_or_default();
        for r in like_results {
            if let Some(tid) = r.thread_id {
                if seen.insert(tid.clone()) {
                    all_thread_ids.push(tid);
                }
            }
        }
    }

    Ok(all_thread_ids)
}

/// Toggle the STARRED label on a thread locally (insert or delete from thread_labels).
#[allow(dead_code)] // used in tests
pub(crate) async fn toggle_star_local(pool: &sqlx::SqlitePool, thread_id: &str, starred: bool) -> Result<(), String> {
    if starred {
        sqlx::query("INSERT OR IGNORE INTO thread_labels (thread_id, label_id) VALUES (?, 'STARRED')")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        sqlx::query("DELETE FROM thread_labels WHERE thread_id = ? AND label_id = 'STARRED'")
            .bind(thread_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Mark a thread as read or unread locally (update threads.unread column).
#[allow(dead_code)] // used in tests
pub(crate) async fn mark_read_status_local(pool: &sqlx::SqlitePool, thread_id: &str, unread: bool) -> Result<(), String> {
    let val = if unread { 1 } else { 0 };
    sqlx::query("UPDATE threads SET unread = ? WHERE id = ?")
        .bind(val)
        .bind(thread_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn search_messages(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    // Local search (operators + FTS5 + LIKE fallback)
    let mut all_thread_ids = search_messages_local(pool.inner(), &account.id, &query).await?;
    let mut seen: std::collections::HashSet<String> = all_thread_ids.iter().cloned().collect();

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

fn auth_gmail_api_url(path: &str) -> String {
    #[cfg(test)]
    {
        let base = std::env::var("TEST_AUTH_GMAIL_API_BASE")
            .unwrap_or_else(|_| "https://gmail.googleapis.com".to_string());
        format!("{}{}", base, path)
    }
    #[cfg(not(test))]
    {
        format!("https://gmail.googleapis.com{}", path)
    }
}

pub(crate) async fn search_gmail_api(access_token: &str, query: &str) -> Vec<String> {
    let client = reqwest::Client::new();
    let res = match client
        .get(auth_gmail_api_url("/gmail/v1/users/me/messages"))
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

pub(crate) async fn fetch_threads_by_ids(
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

pub(crate) async fn get_hydration_progress_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<HydrationProgress, String> {
    #[derive(sqlx::FromRow)]
    struct Count {
        cnt: i32,
    }

    let total =
        sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM threads WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(pool)
            .await
            .map(|r| r.cnt)
            .unwrap_or(0) as usize;

    let hydrated = sqlx::query_as::<_, Count>(
        "SELECT COUNT(DISTINCT t.id) as cnt FROM threads t INNER JOIN messages m ON t.id = m.thread_id WHERE t.account_id = ?"
    ).bind(account_id).fetch_one(pool).await.map(|r| r.cnt).unwrap_or(0) as usize;

    Ok(HydrationProgress { total, hydrated })
}

#[tauri::command]
pub async fn get_hydration_progress(
    app_handle: tauri::AppHandle,
) -> Result<HydrationProgress, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    get_hydration_progress_inner(pool.inner(), &account.id).await
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

pub(crate) async fn get_search_suggestions_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    operator: Option<&str>,
    value: &str,
    full_query: &str,
) -> Result<Vec<SearchSuggestion>, String> {
    let mut suggestions = Vec::new();

    match operator {
        Some("from") => {
            if !value.is_empty() {
                #[derive(sqlx::FromRow)]
                struct SenderRow { sender: String }
                let pattern = format!("%{}%", value);
                let contacts: Vec<SenderRow> = sqlx::query_as(
                    "SELECT DISTINCT sender FROM messages WHERE account_id = ? AND sender LIKE ? LIMIT 8",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
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
            if !value.is_empty() {
                #[derive(sqlx::FromRow)]
                struct RecipRow { recipients: String }
                let pattern = format!("%{}%", value);
                let rows: Vec<RecipRow> = sqlx::query_as(
                    "SELECT DISTINCT recipients FROM messages WHERE account_id = ? AND recipients LIKE ? LIMIT 20",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
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
            if !value.is_empty() {
                #[derive(sqlx::FromRow)]
                struct SubjectRow { subject: String }
                let pattern = format!("%{}%", value);
                let subjects: Vec<SubjectRow> = sqlx::query_as(
                    "SELECT DISTINCT subject FROM messages WHERE account_id = ? AND subject LIKE ? LIMIT 8",
                )
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
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
            #[derive(sqlx::FromRow)]
            struct SettingRow { value: String }
            if let Ok(Some(row)) =
                sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
                    .fetch_optional(pool)
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
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
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
                .bind(account_id)
                .bind(&pattern)
                .fetch_all(pool)
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
pub async fn get_search_suggestions(
    app_handle: tauri::AppHandle,
    operator: Option<String>,
    value: String,
    full_query: String,
) -> Result<Vec<SearchSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    get_search_suggestions_inner(pool.inner(), &account.id, operator.as_deref(), &value, &full_query).await
}

pub(crate) async fn save_recent_search_inner(pool: &sqlx::SqlitePool, query: &str) -> Result<(), String> {
    #[derive(sqlx::FromRow)]
    struct SettingRow {
        value: String,
    }
    let mut recents: Vec<String> =
        sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .and_then(|r| serde_json::from_str(&r.value).ok())
            .unwrap_or_default();

    recents.retain(|r| r != query);
    recents.insert(0, query.to_string());
    recents.truncate(10);

    let json = serde_json::to_string(&recents).unwrap_or_default();
    sqlx::query("INSERT INTO settings (key, value) VALUES ('recent_searches', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(&json).execute(pool).await.map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn save_recent_search(app_handle: tauri::AppHandle, query: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    save_recent_search_inner(pool.inner(), &query).await
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

pub(crate) async fn search_contacts_inner(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    query: &str,
) -> Result<Vec<ContactSuggestion>, String> {
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
    .bind(account_id).bind(&pattern)
    .bind(account_id).bind(&pattern)
    .fetch_all(pool).await.unwrap_or_default();

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

#[tauri::command]
pub async fn search_contacts(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<Vec<ContactSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    search_contacts_inner(pool.inner(), &account.id, &query).await
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

pub(crate) fn validate_external_url(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only http and https URLs are allowed".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    validate_external_url(&url)?;
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::SqlitePool;
    use std::env;
    use std::str::FromStr;

    async fn setup_test_db() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        pool
    }

    // Helper to insert an account
    async fn insert_account(pool: &SqlitePool, id: &str, email: &str, display_name: &str, is_active: i32, created_at: i64) {
        sqlx::query("INSERT INTO accounts (id, email, display_name, avatar_url, token_expiry, is_active, created_at) VALUES (?, ?, ?, '', 9999999999, ?, ?)")
            .bind(id).bind(email).bind(display_name).bind(is_active).bind(created_at)
            .execute(pool).await.unwrap();
    }

    // Helper to insert a message
    async fn insert_message(pool: &SqlitePool, id: &str, thread_id: &str, account_id: &str, sender: &str, recipients: &str, subject: &str, internal_date: i64) {
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, '', ?, '', '', 0)")
            .bind(id).bind(thread_id).bind(account_id).bind(sender).bind(recipients).bind(subject).bind(internal_date)
            .execute(pool).await.unwrap();
    }

    // Helper to insert a thread
    async fn insert_thread(pool: &SqlitePool, id: &str, account_id: &str) {
        sqlx::query("INSERT INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)")
            .bind(id).bind(account_id)
            .execute(pool).await.unwrap();
    }

    // ===== Existing tests (kept) =====

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
        env::set_var("RUSTYMAIL_CLIENT_ID", "dummy_id");
        env::set_var("RUSTYMAIL_CLIENT_SECRET", "dummy_secret");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search_messages_fts5() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO accounts (id) VALUES (?)")
            .bind("acc1").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("msg1").bind("thread1").bind("acc1").bind("sender@example.com").bind("recipient@example.com")
            .bind("Test Subject").bind("snippet").bind(0i64).bind("This is a searchterm inside the body").bind("").bind(0)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?")
            .bind("msg1").execute(&pool).await.unwrap();
        let fts_query = "searchterm*".to_string();
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT m.thread_id FROM messages m INNER JOIN messages_fts ON messages_fts.rowid = m.rowid WHERE messages_fts MATCH ? AND m.account_id = ?"
        ).bind(&fts_query).bind("acc1").fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "thread1");
    }

    #[tokio::test]
    async fn test_accounts_schema_has_no_token_columns() {
        let pool = setup_test_db().await;
        #[derive(sqlx::FromRow)]
        struct ColInfo { name: String }
        let cols: Vec<ColInfo> = sqlx::query_as("PRAGMA table_info(accounts)")
            .fetch_all(&pool).await.unwrap();
        let col_names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert!(!col_names.contains(&"access_token"), "access_token should not be in accounts table");
        assert!(!col_names.contains(&"refresh_token"), "refresh_token should not be in accounts table");
        assert!(col_names.contains(&"id"));
        assert!(col_names.contains(&"token_expiry"));
    }

    // ===== parse_query_operators tests =====

    #[test]
    fn test_parse_query_simple_free_text() {
        let p = parse_query_operators("hello world");
        assert_eq!(p.free_text, "hello world");
        assert!(p.from.is_none());
        assert!(p.to.is_none());
        assert!(p.subject.is_none());
        assert!(!p.has_attachment);
        assert!(p.is_unread.is_none());
    }

    #[test]
    fn test_parse_query_from_operator() {
        let p = parse_query_operators("from:alice@example.com");
        assert_eq!(p.from, Some("alice@example.com".to_string()));
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_to_operator() {
        let p = parse_query_operators("to:bob@example.com");
        assert_eq!(p.to, Some("bob@example.com".to_string()));
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_subject_operator() {
        let p = parse_query_operators("subject:meeting");
        assert_eq!(p.subject, Some("meeting".to_string()));
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_quoted_values() {
        let p = parse_query_operators("from:\"Alice Doe\"");
        assert_eq!(p.from, Some("Alice Doe".to_string()));
    }

    #[test]
    fn test_parse_query_has_attachment() {
        let p = parse_query_operators("has:attachment");
        assert!(p.has_attachment);
        assert!(p.free_text.is_empty());
    }

    #[test]
    fn test_parse_query_is_unread() {
        let p = parse_query_operators("is:unread");
        assert_eq!(p.is_unread, Some(true));
    }

    #[test]
    fn test_parse_query_is_read() {
        let p = parse_query_operators("is:read");
        assert_eq!(p.is_unread, Some(false));
    }

    #[test]
    fn test_parse_query_mixed_operators() {
        let p = parse_query_operators("from:alice subject:meeting hello");
        assert_eq!(p.from, Some("alice".to_string()));
        assert_eq!(p.subject, Some("meeting".to_string()));
        assert_eq!(p.free_text, "hello");
    }

    #[test]
    fn test_parse_query_passthrough_operators() {
        let p = parse_query_operators("before:2024/01/01");
        assert!(p.free_text.contains("before:2024/01/01"));
        assert!(p.from.is_none());
    }

    #[test]
    fn test_parse_query_has_link_passthrough() {
        let p = parse_query_operators("has:link");
        assert!(p.free_text.contains("has:link"));
    }

    #[test]
    fn test_parse_query_empty_operator_value() {
        let p = parse_query_operators("from: something");
        // "from:" with empty value is not set
        assert!(p.from.is_none());
        assert_eq!(p.free_text, "something");
    }

    // ===== open_external_url validation tests =====

    #[test]
    fn test_validate_url_javascript_rejected() {
        let result = validate_external_url("javascript:alert(1)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only http and https URLs are allowed"));
    }

    #[test]
    fn test_validate_url_ftp_rejected() {
        let result = validate_external_url("ftp://example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only http and https URLs are allowed"));
    }

    #[test]
    fn test_validate_url_empty_rejected() {
        let result = validate_external_url("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only http and https URLs are allowed"));
    }

    #[test]
    fn test_validate_url_data_rejected() {
        assert!(validate_external_url("data:text/html,<h1>hi</h1>").is_err());
    }

    #[test]
    fn test_validate_url_http_accepted() {
        assert!(validate_external_url("http://example.com").is_ok());
    }

    #[test]
    fn test_validate_url_https_accepted() {
        assert!(validate_external_url("https://example.com").is_ok());
    }

    // ===== get_accounts_inner tests =====

    #[tokio::test]
    async fn test_get_accounts_inner_empty() {
        let pool = setup_test_db().await;
        let accounts = get_accounts_inner(&pool).await.unwrap();
        assert!(accounts.is_empty());
    }

    #[tokio::test]
    async fn test_get_accounts_inner_returns_ordered() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc2", "b@test.com", "B User", 0, 200).await;
        insert_account(&pool, "acc1", "a@test.com", "A User", 1, 100).await;

        let accounts = get_accounts_inner(&pool).await.unwrap();
        assert_eq!(accounts.len(), 2);
        // Ordered by created_at ASC
        assert_eq!(accounts[0].id, "acc1");
        assert_eq!(accounts[0].email, "a@test.com");
        assert!(accounts[0].is_active);
        assert_eq!(accounts[1].id, "acc2");
        assert!(!accounts[1].is_active);
    }

    // ===== get_setting_inner / update_setting_inner tests =====

    #[tokio::test]
    async fn test_get_setting_inner_default_values() {
        let pool = setup_test_db().await;
        let theme = get_setting_inner(&pool, "theme").await.unwrap();
        assert_eq!(theme, "system");
        let density = get_setting_inner(&pool, "density").await.unwrap();
        assert_eq!(density, "default");
    }

    #[tokio::test]
    async fn test_get_setting_inner_nonexistent() {
        let pool = setup_test_db().await;
        let val = get_setting_inner(&pool, "nonexistent_key").await.unwrap();
        assert_eq!(val, "");
    }

    #[tokio::test]
    async fn test_update_setting_inner() {
        let pool = setup_test_db().await;
        let original = get_setting_inner(&pool, "theme").await.unwrap();
        assert_eq!(original, "system");

        update_setting_inner(&pool, "theme", "dark").await.unwrap();
        let updated = get_setting_inner(&pool, "theme").await.unwrap();
        assert_eq!(updated, "dark");
    }

    #[tokio::test]
    async fn test_update_setting_inner_creates_new_key() {
        let pool = setup_test_db().await;
        update_setting_inner(&pool, "custom_key", "custom_value").await.unwrap();
        let val = get_setting_inner(&pool, "custom_key").await.unwrap();
        assert_eq!(val, "custom_value");
    }

    // ===== get_labels_inner tests =====

    #[tokio::test]
    async fn test_get_labels_inner_empty() {
        let pool = setup_test_db().await;
        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert!(labels.is_empty());
    }

    #[tokio::test]
    async fn test_get_labels_inner_filters_hidden_labels() {
        let pool = setup_test_db().await;
        // Insert labels including ones that should be filtered
        for (id, name, ltype) in &[
            ("INBOX", "INBOX", "system"),
            ("SENT", "SENT", "system"),
            ("CHAT", "CHAT", "system"),
            ("VOICEMAIL", "VOICEMAIL", "system"),
            ("YELLOW_STAR", "YELLOW_STAR", "system"),
            ("Label_1", "Work", "user"),
        ] {
            sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES (?, 'acc1', ?, ?, 0)")
                .bind(id).bind(name).bind(ltype)
                .execute(&pool).await.unwrap();
        }

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        let label_ids: Vec<&str> = labels.iter().map(|l| l.id.as_str()).collect();
        assert!(label_ids.contains(&"INBOX"));
        assert!(label_ids.contains(&"SENT"));
        assert!(label_ids.contains(&"Label_1"));
        assert!(!label_ids.contains(&"CHAT"));
        assert!(!label_ids.contains(&"VOICEMAIL"));
        assert!(!label_ids.contains(&"YELLOW_STAR"));
    }

    #[tokio::test]
    async fn test_get_labels_inner_ordering() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('Label_Z', 'acc1', 'Zebra', 'user', 0)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX', 'acc1', 'INBOX', 'system', 5)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('Label_A', 'acc1', 'Alpha', 'user', 0)")
            .execute(&pool).await.unwrap();

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        // System labels first, then user labels alphabetically
        assert_eq!(labels[0].id, "INBOX");
        assert_eq!(labels[1].id, "Label_A");
        assert_eq!(labels[2].id, "Label_Z");
    }

    #[tokio::test]
    async fn test_get_labels_inner_account_isolation() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX', 'acc1', 'INBOX', 'system', 3)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX2', 'acc2', 'INBOX', 'system', 1)")
            .execute(&pool).await.unwrap();

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].unread_count, 3);
    }

    // ===== get_threads_inner tests =====

    #[tokio::test]
    async fn test_get_threads_inner_empty() {
        let pool = setup_test_db().await;
        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(threads.is_empty());
    }

    #[tokio::test]
    async fn test_get_threads_inner_with_messages() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@test.com>", "bob@test.com", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@test.com>", "alice@test.com", "World", 2000).await;

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 2);
        // Ordered by most recent message first
        assert_eq!(threads[0].id, "t2");
        assert_eq!(threads[0].sender, "Bob");
        assert_eq!(threads[0].subject, "World");
        assert_eq!(threads[1].id, "t1");
        assert_eq!(threads[1].sender, "Alice");
    }

    #[tokio::test]
    async fn test_get_threads_inner_label_filtering() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "sender@test.com", "", "Sub1", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "sender@test.com", "", "Sub2", 2000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'INBOX')")
            .execute(&pool).await.unwrap();

        let inbox_threads = get_threads_inner(&pool, "acc1", Some("INBOX"), 0, 50).await.unwrap();
        assert_eq!(inbox_threads.len(), 1);
        assert_eq!(inbox_threads[0].id, "t1");

        let all_threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(all_threads.len(), 2);
    }

    #[tokio::test]
    async fn test_get_threads_inner_starred_flag() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert!(threads[0].starred);
    }

    #[tokio::test]
    async fn test_get_threads_inner_pagination() {
        let pool = setup_test_db().await;
        for i in 0..5 {
            let tid = format!("t{}", i);
            let mid = format!("m{}", i);
            insert_thread(&pool, &tid, "acc1").await;
            insert_message(&pool, &mid, &tid, "acc1", "s@t.com", "", "Sub", (i * 1000) as i64).await;
        }

        let page1 = get_threads_inner(&pool, "acc1", None, 0, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = get_threads_inner(&pool, "acc1", None, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
        // Pages should not overlap
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn test_get_threads_inner_no_subject_fallback() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        // Thread with no messages -> subject should be "No Subject"
        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].subject, "No Subject");
    }

    // ===== get_messages_inner tests =====

    #[tokio::test]
    async fn test_get_messages_inner_empty() {
        let pool = setup_test_db().await;
        let messages = get_messages_inner(&pool, "nonexistent").await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_get_messages_inner_ordered_by_date() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m2", "t1", "acc1", "bob@test.com", "alice@test.com", "Reply", 2000).await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "bob@test.com", "Original", 1000).await;

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 2);
        // ASC order by internal_date
        assert_eq!(messages[0].id, "m1");
        assert_eq!(messages[0].subject, "Original");
        assert_eq!(messages[1].id, "m2");
        assert_eq!(messages[1].subject, "Reply");
    }

    #[tokio::test]
    async fn test_get_messages_inner_draft_flag() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Draft msg", 1000).await;
        sqlx::query("INSERT INTO message_labels (message_id, label_id) VALUES ('m1', 'DRAFT')")
            .execute(&pool).await.unwrap();

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_draft);
    }

    #[tokio::test]
    async fn test_get_messages_inner_non_draft() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Regular", 1000).await;

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(!messages[0].is_draft);
    }

    // ===== get_hydration_progress_inner tests =====

    #[tokio::test]
    async fn test_get_hydration_progress_inner_empty() {
        let pool = setup_test_db().await;
        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 0);
        assert_eq!(progress.hydrated, 0);
    }

    #[tokio::test]
    async fn test_get_hydration_progress_inner_partial() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_thread(&pool, "t3", "acc1").await;
        // Only t1 has messages (hydrated)
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 3);
        assert_eq!(progress.hydrated, 1);
    }

    #[tokio::test]
    async fn test_get_hydration_progress_inner_all_hydrated() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Sub", 2000).await;

        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 2);
        assert_eq!(progress.hydrated, 2);
    }

    #[tokio::test]
    async fn test_get_hydration_progress_inner_account_isolation() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc2").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        let progress = get_hydration_progress_inner(&pool, "acc1").await.unwrap();
        assert_eq!(progress.total, 1);
        assert_eq!(progress.hydrated, 1);
    }

    // ===== search_contacts_inner tests =====

    #[tokio::test]
    async fn test_search_contacts_inner_empty() {
        let pool = setup_test_db().await;
        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert!(contacts.is_empty());
    }

    #[tokio::test]
    async fn test_search_contacts_inner_finds_senders() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice Doe <alice@example.com>", "bob@example.com", "Hi", 1000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert!(!contacts.is_empty());
        assert_eq!(contacts[0].email, "alice@example.com");
        assert_eq!(contacts[0].name, "Alice Doe");
    }

    #[tokio::test]
    async fn test_search_contacts_inner_finds_recipients() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "someone@test.com", "Bob Smith <bob@example.com>", "Hi", 1000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "bob").await.unwrap();
        assert!(!contacts.is_empty());
        assert_eq!(contacts[0].email, "bob@example.com");
    }

    #[tokio::test]
    async fn test_search_contacts_inner_deduplication() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@example.com", "", "Hi", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "alice@example.com", "", "Hi again", 2000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(contacts.len(), 1);
    }

    #[tokio::test]
    async fn test_search_contacts_inner_sorted_by_email_length() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice_longname@verylongdomain.com", "", "Hi", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "alice@short.com", "", "Hi", 2000).await;

        let contacts = search_contacts_inner(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(contacts.len(), 2);
        // Shorter email first
        assert!(contacts[0].email.len() <= contacts[1].email.len());
    }

    // ===== save_recent_search_inner tests =====

    #[tokio::test]
    async fn test_save_recent_search_inner_stores_search() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "test query").await.unwrap();

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0], "test query");
    }

    #[tokio::test]
    async fn test_save_recent_search_inner_ordering() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "first").await.unwrap();
        save_recent_search_inner(&pool, "second").await.unwrap();

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents[0], "second");
        assert_eq!(recents[1], "first");
    }

    #[tokio::test]
    async fn test_save_recent_search_inner_deduplication() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "query").await.unwrap();
        save_recent_search_inner(&pool, "other").await.unwrap();
        save_recent_search_inner(&pool, "query").await.unwrap();

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0], "query"); // Most recent first
        assert_eq!(recents[1], "other");
    }

    #[tokio::test]
    async fn test_save_recent_search_inner_truncation() {
        let pool = setup_test_db().await;
        for i in 0..15 {
            save_recent_search_inner(&pool, &format!("query_{}", i)).await.unwrap();
        }

        let val = get_setting_inner(&pool, "recent_searches").await.unwrap();
        let recents: Vec<String> = serde_json::from_str(&val).unwrap();
        assert_eq!(recents.len(), 10); // Truncated to 10
        assert_eq!(recents[0], "query_14"); // Most recent
    }

    // ===== get_search_suggestions_inner tests =====

    #[tokio::test]
    async fn test_get_search_suggestions_inner_from_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "", "Subject", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("from"), "alice", "").await.unwrap();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, "contact");
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_subject_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Weekly Meeting Notes", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("subject"), "meeting", "").await.unwrap();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, "subject");
        assert!(suggestions[0].text.contains("Weekly Meeting Notes"));
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_free_text_with_recents() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "project alpha").await.unwrap();
        save_recent_search_inner(&pool, "budget report").await.unwrap();

        let suggestions = get_search_suggestions_inner(&pool, "acc1", None, "", "").await.unwrap();
        let recent_suggestions: Vec<&SearchSuggestion> = suggestions.iter().filter(|s| s.kind == "recent").collect();
        assert_eq!(recent_suggestions.len(), 2);
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_free_text_filters_recents() {
        let pool = setup_test_db().await;
        save_recent_search_inner(&pool, "project alpha").await.unwrap();
        save_recent_search_inner(&pool, "budget report").await.unwrap();

        let suggestions = get_search_suggestions_inner(&pool, "acc1", None, "", "project").await.unwrap();
        let recent_suggestions: Vec<&SearchSuggestion> = suggestions.iter().filter(|s| s.kind == "recent").collect();
        assert_eq!(recent_suggestions.len(), 1);
        assert!(recent_suggestions[0].text.contains("project"));
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_free_text_contacts_and_subjects() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "", "Budget Report Q4", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", None, "", "budget").await.unwrap();
        let contact_count = suggestions.iter().filter(|s| s.kind == "contact").count();
        let subject_count = suggestions.iter().filter(|s| s.kind == "subject").count();
        // "budget" is >= 2 chars, so should search contacts and subjects
        assert!(contact_count > 0 || subject_count > 0);
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_to_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "Bob <bob@example.com>", "Hi", 1000).await;

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("to"), "bob", "").await.unwrap();
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, "contact");
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_empty_value() {
        let pool = setup_test_db().await;
        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("from"), "", "").await.unwrap();
        assert!(suggestions.is_empty());
    }

    // ===== switch_account_inner tests =====

    #[tokio::test]
    async fn test_switch_account_inner() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;
        insert_account(&pool, "acc2", "b@test.com", "B", 0, 200).await;

        switch_account_inner(&pool, "acc2").await.unwrap();

        let accounts = get_accounts_inner(&pool).await.unwrap();
        let acc1 = accounts.iter().find(|a| a.id == "acc1").unwrap();
        let acc2 = accounts.iter().find(|a| a.id == "acc2").unwrap();
        assert!(!acc1.is_active);
        assert!(acc2.is_active);
    }

    #[tokio::test]
    async fn test_switch_account_inner_idempotent() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;

        switch_account_inner(&pool, "acc1").await.unwrap();

        let accounts = get_accounts_inner(&pool).await.unwrap();
        assert!(accounts[0].is_active);
    }

    // ===== remove_account_inner tests =====

    #[tokio::test]
    async fn test_remove_account_inner_cleans_all_data() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO labels (id, account_id, name, type, unread_count) VALUES ('INBOX', 'acc1', 'INBOX', 'system', 0)")
            .execute(&pool).await.unwrap();

        remove_account_inner(&pool, "acc1").await.unwrap();

        let accounts = get_accounts_inner(&pool).await.unwrap();
        assert!(accounts.is_empty());

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(threads.is_empty());

        let messages = get_messages_inner(&pool, "t1").await.unwrap();
        assert!(messages.is_empty());

        let labels = get_labels_inner(&pool, "acc1").await.unwrap();
        assert!(labels.is_empty());
    }

    #[tokio::test]
    async fn test_remove_account_inner_activates_remaining() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;
        insert_account(&pool, "acc2", "b@test.com", "B", 0, 200).await;

        remove_account_inner(&pool, "acc1").await.unwrap();

        let accounts = get_accounts_inner(&pool).await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "acc2");
        assert!(accounts[0].is_active);
    }

    #[tokio::test]
    async fn test_remove_account_inner_preserves_other_account_data() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;
        insert_account(&pool, "acc2", "b@test.com", "B", 0, 200).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc2").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc2", "s@t.com", "", "Sub", 2000).await;

        remove_account_inner(&pool, "acc1").await.unwrap();

        let threads = get_threads_inner(&pool, "acc2", None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "t2");

        let messages = get_messages_inner(&pool, "t2").await.unwrap();
        assert_eq!(messages.len(), 1);
    }

    // ===== get_active_account_row tests =====

    #[tokio::test]
    async fn test_get_active_account_row_returns_active() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;
        insert_account(&pool, "acc2", "b@test.com", "B", 0, 200).await;

        let (id, expiry) = get_active_account_row(&pool).await.unwrap();
        assert_eq!(id, "acc1");
        assert!(expiry.is_some());
    }

    #[tokio::test]
    async fn test_get_active_account_row_no_active() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 0, 100).await;

        let result = get_active_account_row(&pool).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No active account found"));
    }

    #[tokio::test]
    async fn test_get_active_account_row_empty_db() {
        let pool = setup_test_db().await;
        let result = get_active_account_row(&pool).await;
        assert!(result.is_err());
    }

    // ===== check_auth_status_db tests =====

    #[tokio::test]
    async fn test_check_auth_status_db_with_active() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 1, 100).await;

        let result = check_auth_status_db(&pool).await.unwrap();
        assert_eq!(result, Some("acc1".to_string()));
    }

    #[tokio::test]
    async fn test_check_auth_status_db_no_active() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 0, 100).await;

        let result = check_auth_status_db(&pool).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_check_auth_status_db_empty() {
        let pool = setup_test_db().await;
        let result = check_auth_status_db(&pool).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_check_auth_status_db_multiple_accounts_one_active() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "a@test.com", "A", 0, 100).await;
        insert_account(&pool, "acc2", "b@test.com", "B", 1, 200).await;
        insert_account(&pool, "acc3", "c@test.com", "C", 0, 300).await;

        let result = check_auth_status_db(&pool).await.unwrap();
        assert_eq!(result, Some("acc2".to_string()));
    }

    // ===== get_settings_inner tests =====

    #[tokio::test]
    async fn test_get_settings_inner_returns_defaults() {
        let pool = setup_test_db().await;
        let settings = get_settings_inner(&pool).await.unwrap();
        // apply_schema inserts schema_version + 12 defaults = at least 13
        assert!(settings.len() >= 13, "Expected at least 13 settings, got {}", settings.len());

        let theme = settings.iter().find(|s| s.key == "theme");
        assert!(theme.is_some());
        assert_eq!(theme.unwrap().value, "system");

        let density = settings.iter().find(|s| s.key == "density");
        assert!(density.is_some());
        assert_eq!(density.unwrap().value, "default");
    }

    #[tokio::test]
    async fn test_get_settings_inner_reflects_updates() {
        let pool = setup_test_db().await;
        update_setting_inner(&pool, "theme", "dark").await.unwrap();

        let settings = get_settings_inner(&pool).await.unwrap();
        let theme = settings.iter().find(|s| s.key == "theme").unwrap();
        assert_eq!(theme.value, "dark");
    }

    #[tokio::test]
    async fn test_get_settings_inner_includes_custom_keys() {
        let pool = setup_test_db().await;
        update_setting_inner(&pool, "my_custom", "val123").await.unwrap();

        let settings = get_settings_inner(&pool).await.unwrap();
        let custom = settings.iter().find(|s| s.key == "my_custom");
        assert!(custom.is_some());
        assert_eq!(custom.unwrap().value, "val123");
    }

    // ===== fetch_threads_by_ids tests =====

    #[tokio::test]
    async fn test_fetch_threads_by_ids_empty_list() {
        let pool = setup_test_db().await;
        let result = fetch_threads_by_ids(&pool, &[], "acc1").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_returns_matching() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_thread(&pool, "t3", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@test.com>", "", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@test.com>", "", "World", 2000).await;
        insert_message(&pool, "m3", "t3", "acc1", "Carol <carol@test.com>", "", "Test", 3000).await;

        let ids = vec!["t1".to_string(), "t3".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(result.len(), 2);
        // Ordered by internal_date DESC
        assert_eq!(result[0].id, "t3");
        assert_eq!(result[1].id, "t1");
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_account_isolation() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc2").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc2", "s@t.com", "", "Sub", 2000).await;

        let ids = vec!["t1".to_string(), "t2".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "t1");
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_with_starred() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        let ids = vec!["t1".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].starred);
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_nonexistent() {
        let pool = setup_test_db().await;
        let ids = vec!["nonexistent".to_string()];
        let result = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert!(result.is_empty());
    }

    // ===== search_messages_local tests =====

    #[tokio::test]
    async fn test_search_messages_local_empty_db() {
        let pool = setup_test_db().await;
        let result = search_messages_local(&pool, "acc1", "hello").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_search_messages_local_like_fallback_sender() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "bob@test.com", "Hello", 1000).await;

        let result = search_messages_local(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_like_fallback_subject() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Important Meeting Notes", 1000).await;

        let result = search_messages_local(&pool, "acc1", "meeting").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_fts5() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, recipients, subject, snippet, internal_date, body_plain, body_html, has_attachments) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1").bind("s@t.com").bind("")
            .bind("Subject").bind("").bind(1000i64).bind("unique_searchterm_xyz in body").bind("").bind(0)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT OR REPLACE INTO messages_fts(rowid, sender, subject, body_plain) SELECT rowid, sender, subject, body_plain FROM messages WHERE id = ?")
            .bind("m1").execute(&pool).await.unwrap();

        let result = search_messages_local(&pool, "acc1", "unique_searchterm_xyz").await.unwrap();
        assert!(result.contains(&"t1".to_string()));
    }

    #[tokio::test]
    async fn test_search_messages_local_from_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <alice@example.com>", "", "Sub", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <bob@example.com>", "", "Sub", 2000).await;

        let result = search_messages_local(&pool, "acc1", "from:alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_subject_operator() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Budget Review", 1000).await;
        insert_message(&pool, "m2", "t2", "acc1", "s@t.com", "", "Project Update", 2000).await;

        let result = search_messages_local(&pool, "acc1", "subject:budget").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_deduplicates() {
        let pool = setup_test_db().await;
        // Two messages in the same thread, both matching
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Hello Alice", 1000).await;
        insert_message(&pool, "m2", "t1", "acc1", "alice@test.com", "", "Reply Alice", 2000).await;

        let result = search_messages_local(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    #[tokio::test]
    async fn test_search_messages_local_account_isolation() {
        let pool = setup_test_db().await;
        insert_message(&pool, "m1", "t1", "acc1", "alice@test.com", "", "Hello", 1000).await;
        insert_message(&pool, "m2", "t2", "acc2", "alice@test.com", "", "Hello", 2000).await;

        let result = search_messages_local(&pool, "acc1", "alice").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");
    }

    // ===== toggle_star_local tests =====

    #[tokio::test]
    async fn test_toggle_star_local_add_star() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        toggle_star_local(&pool, "t1", true).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct LabelRow { label_id: String }
        let labels: Vec<LabelRow> = sqlx::query_as("SELECT label_id FROM thread_labels WHERE thread_id = 't1'")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].label_id, "STARRED");
    }

    #[tokio::test]
    async fn test_toggle_star_local_remove_star() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        sqlx::query("INSERT INTO thread_labels (thread_id, label_id) VALUES ('t1', 'STARRED')")
            .execute(&pool).await.unwrap();

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_toggle_star_local_idempotent_add() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        toggle_star_local(&pool, "t1", true).await.unwrap();
        toggle_star_local(&pool, "t1", true).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_toggle_star_local_idempotent_remove() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM thread_labels WHERE thread_id = 't1' AND label_id = 'STARRED'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn test_toggle_star_local_verified_via_threads() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        // Initially not starred
        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(!threads[0].starred);

        toggle_star_local(&pool, "t1", true).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(threads[0].starred);

        toggle_star_local(&pool, "t1", false).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(!threads[0].starred);
    }

    // ===== mark_read_status_local tests =====

    #[tokio::test]
    async fn test_mark_read_status_local_mark_unread() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;

        mark_read_status_local(&pool, "t1", true).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct UnreadRow { unread: Option<i32> }
        let row: UnreadRow = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(row.unread, Some(1));
    }

    #[tokio::test]
    async fn test_mark_read_status_local_mark_read() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        // First mark as unread
        mark_read_status_local(&pool, "t1", true).await.unwrap();

        // Now mark as read
        mark_read_status_local(&pool, "t1", false).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct UnreadRow { unread: Option<i32> }
        let row: UnreadRow = sqlx::query_as("SELECT unread FROM threads WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(row.unread, Some(0));
    }

    #[tokio::test]
    async fn test_mark_read_status_local_verified_via_threads() {
        let pool = setup_test_db().await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "s@t.com", "", "Sub", 1000).await;

        // Default is 0 (read)
        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 0);

        mark_read_status_local(&pool, "t1", true).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 1);

        mark_read_status_local(&pool, "t1", false).await.unwrap();

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads[0].unread, 0);
    }

    #[tokio::test]
    async fn test_mark_read_status_local_nonexistent_thread() {
        let pool = setup_test_db().await;
        // Should not error, just affect 0 rows
        let result = mark_read_status_local(&pool, "nonexistent", true).await;
        assert!(result.is_ok());
    }

    // ===== search_gmail_api tests =====

    use std::sync::Mutex;
    static AUTH_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn test_search_gmail_api_success() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "messages": [
                        {"id": "m1", "threadId": "t1"},
                        {"id": "m2", "threadId": "t2"},
                        {"id": "m3", "threadId": "t1"}
                    ]
                }));
        });

        let result = search_gmail_api("fake_token", "test query").await;
        mock.assert();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"t1".to_string()));
        assert!(result.contains(&"t2".to_string()));

        env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    #[tokio::test]
    async fn test_search_gmail_api_empty_response() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({}));
        });

        let result = search_gmail_api("fake_token", "no results").await;
        mock.assert();
        assert!(result.is_empty());

        env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    #[tokio::test]
    async fn test_search_gmail_api_http_error() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(401);
        });

        let result = search_gmail_api("bad_token", "query").await;
        mock.assert();
        assert!(result.is_empty());

        env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    #[tokio::test]
    async fn test_search_gmail_api_deduplicates_threads() {
        let _lock = AUTH_ENV_LOCK.lock().unwrap();
        let server = httpmock::MockServer::start();
        env::set_var("TEST_AUTH_GMAIL_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/gmail/v1/users/me/messages");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "messages": [
                        {"id": "m1", "threadId": "t1"},
                        {"id": "m2", "threadId": "t1"},
                        {"id": "m3", "threadId": "t1"}
                    ]
                }));
        });

        let result = search_gmail_api("fake_token", "query").await;
        mock.assert();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "t1");

        env::remove_var("TEST_AUTH_GMAIL_API_BASE");
    }

    // ===== open_external_url full tests =====


    // ===== Additional edge case tests =====

    #[test]
    fn test_clean_sender_name_empty_string() {
        assert_eq!(clean_sender_name(Some("".to_string())), "");
    }

    #[test]
    fn test_clean_sender_name_whitespace_only() {
        assert_eq!(clean_sender_name(Some("   ".to_string())), "   ");
    }

    #[test]
    fn test_clean_sender_name_multiple_brackets() {
        assert_eq!(
            clean_sender_name(Some("Name <email> <extra>".to_string())),
            "Name"
        );
    }

    #[test]
    fn test_clean_sender_name_no_brackets() {
        assert_eq!(
            clean_sender_name(Some("just-a-name".to_string())),
            "just-a-name"
        );
    }

    #[tokio::test]
    async fn test_get_threads_inner_clean_sender_name_integration() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "\"John Doe\" <john@example.com>", "", "Test Subject", 1000).await;

        let threads = get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].sender, "John Doe");
    }

    #[tokio::test]
    async fn test_get_messages_inner_with_html_body() {
        let pool = setup_test_db().await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, sender, subject, internal_date, body_html, body_plain) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1").bind("alice@test.com").bind("HTML Test").bind(1000i64)
            .bind("<p>Hello <b>World</b></p>").bind("Hello World")
            .execute(&pool).await.unwrap();

        let msgs = get_messages_inner(&pool, "t1").await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body_html, "<p>Hello <b>World</b></p>");
        assert_eq!(msgs[0].body_plain, "Hello World");
    }

    #[tokio::test]
    async fn test_fetch_threads_by_ids_with_messages_and_sender() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        insert_thread(&pool, "t1", "acc1").await;
        insert_thread(&pool, "t2", "acc1").await;
        insert_message(&pool, "m1", "t1", "acc1", "Alice <a@test.com>", "", "Subject 1", 2000).await;
        insert_message(&pool, "m2", "t2", "acc1", "Bob <b@test.com>", "", "Subject 2", 1000).await;

        let ids = vec!["t1".to_string(), "t2".to_string()];
        let threads = fetch_threads_by_ids(&pool, &ids, "acc1").await.unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].id, "t1");
        assert_eq!(threads[1].id, "t2");
    }

    #[tokio::test]
    async fn test_get_search_suggestions_inner_to_operator_dedup() {
        let pool = setup_test_db().await;
        insert_account(&pool, "acc1", "test@test.com", "Test", 1, 1000).await;
        sqlx::query("INSERT INTO messages (id, thread_id, account_id, recipients, sender, subject, internal_date) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("m1").bind("t1").bind("acc1")
            .bind("alice@test.com, bob@test.com, alice@test.com")
            .bind("sender@test.com").bind("Test").bind(1000i64)
            .execute(&pool).await.unwrap();

        let suggestions = get_search_suggestions_inner(&pool, "acc1", Some("to"), "alice", "").await.unwrap();
        let alice_count = suggestions.iter().filter(|s| s.detail.contains("alice")).count();
        assert_eq!(alice_count, 1);
    }
}
