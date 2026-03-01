use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, TokenResponse, TokenUrl,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use keyring::Entry;
use tauri::Manager;
use std::env;





#[derive(sqlx::FromRow, Clone)]
struct ActiveAccount {
    id: String,
    access_token: String,
}

#[derive(sqlx::FromRow)]
struct ActiveAccountFull {
    id: String,
    access_token: String,
    refresh_token: Option<String>,
    token_expiry: Option<i64>,
}

async fn get_active_account(pool: &sqlx::SqlitePool) -> Result<ActiveAccount, String> {
    let account = sqlx::query_as::<_, ActiveAccountFull>(
        "SELECT id, access_token, refresh_token, token_expiry FROM accounts WHERE is_active = 1 LIMIT 1"
    )
        .fetch_one(pool)
        .await
        .map_err(|_| "No authenticated account found. Please sign in first.".to_string())?;

    let now = chrono::Utc::now().timestamp();
    let expiry = account.token_expiry.unwrap_or(0);
    if expiry > 0 && expiry - 300 < now {
        let refresh_tok = account.refresh_token.clone().unwrap_or_default();
        let token_to_use = if refresh_tok.is_empty() {
            keyring::Entry::new("rustymail", &account.id)
                .and_then(|e| e.get_password())
                .unwrap_or_default()
        } else {
            refresh_tok
        };
        if !token_to_use.is_empty() {
            if let Err(e) = refresh_and_update(pool, &account.id, &token_to_use).await {
            } else {
                
                return sqlx::query_as::<_, ActiveAccount>(
                    "SELECT id, access_token FROM accounts WHERE is_active = 1 LIMIT 1"
                )
                    .fetch_one(pool)
                    .await
                    .map_err(|_| "Failed to read account after refresh.".to_string());
            }
        }
    }

    Ok(ActiveAccount {
        id: account.id,
        access_token: account.access_token,
    })
}





pub async fn start_oauth_flow(app_handle: tauri::AppHandle) -> Result<(), String> {
    let client_id = env::var("RUSTYMAIL_CLIENT_ID")
        .map_err(|_| "RUSTYMAIL_CLIENT_ID not found in environment".to_string())?;
    let client_secret = env::var("RUSTYMAIL_CLIENT_SECRET")
        .map_err(|_| "RUSTYMAIL_CLIENT_SECRET not found in environment".to_string())?;

    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_url = format!("http://127.0.0.1:{}", port);


    let client = BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string()).unwrap(),
        Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string()).unwrap()),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_url.clone()).unwrap());

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(oauth2::Scope::new("openid".to_string()))
        .add_scope(oauth2::Scope::new("email".to_string()))
        .add_scope(oauth2::Scope::new("profile".to_string()))
        .add_scope(oauth2::Scope::new("https://www.googleapis.com/auth/gmail.readonly".to_string()))
        .add_scope(oauth2::Scope::new("https://www.googleapis.com/auth/gmail.modify".to_string()))
        .add_scope(oauth2::Scope::new("https://www.googleapis.com/auth/gmail.send".to_string()))
        .add_scope(oauth2::Scope::new("https://www.googleapis.com/auth/gmail.labels".to_string()))
        .add_scope(oauth2::Scope::new("https://www.googleapis.com/auth/calendar.readonly".to_string()))
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
    reader.read_line(&mut request_line).await.map_err(|e| e.to_string())?;

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
                        if k == "code" { code = v.to_string(); }
                        else if k == "state" { state = v.to_string(); }
                    }
                }
            }
        }
    }

    let response = "HTTP/1.1 200 OK\r\n\r\n<html><body style='font-family:-apple-system,system-ui,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;color:#333;'><div style='text-align:center'><h2>✓ Authentication successful!</h2><p>You can close this tab and return to Rustymail.</p></div></body></html>";
    let _ = stream.write_all(response.as_bytes()).await;

    if state != *csrf_token.secret() {
        return Err("CSRF token mismatch".to_string());
    }

    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| e.to_string())?;

    let access_token = token_result.access_token().secret().to_string();
    let refresh_token = token_result.refresh_token().map(|r| r.secret().clone()).unwrap_or_default();
    
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
                    body["email"].as_str().unwrap_or("unknown@gmail.com").to_string(),
                    body["name"].as_str().unwrap_or("").to_string(),
                    body["picture"].as_str().unwrap_or("").to_string(),
                )
            } else {
                ("unknown@gmail.com".to_string(), String::new(), String::new())
            }
        }
        Err(_) => ("unknown@gmail.com".to_string(), String::new(), String::new()),
    };

    
    let account_id = email.clone();

    if !refresh_token.is_empty() {
        if let Ok(entry) = Entry::new("rustymail", &account_id) {
            let _ = entry.set_password(&refresh_token);
        }
    }

    let pool = app_handle.state::<sqlx::SqlitePool>();

    
    let _ = sqlx::query("UPDATE accounts SET is_active = 0").execute(pool.inner()).await;

    let sql = "INSERT INTO accounts (id, email, display_name, avatar_url, access_token, refresh_token, token_expiry, is_active, created_at) 
               VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?)
               ON CONFLICT(id) DO UPDATE SET 
                 access_token = excluded.access_token, 
                 refresh_token = excluded.refresh_token, 
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
        .bind(&access_token)
        .bind(&refresh_token)
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
        refresh_token: Option<String>,
        token_expiry: Option<i64>,
        is_active: Option<i32>,
    }

    let all_accounts: Vec<AccountRow> = sqlx::query_as("SELECT id, email, display_name, avatar_url, refresh_token, token_expiry, is_active FROM accounts")
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    if all_accounts.is_empty() {
        return Ok(AuthStatus { authenticated: false, active_account: None, accounts: vec![] });
    }

    let accounts_info: Vec<AccountInfo> = all_accounts.iter().map(|a| AccountInfo {
        id: a.id.clone(),
        email: a.email.clone().unwrap_or_default(),
        display_name: a.display_name.clone().unwrap_or_default(),
        avatar_url: a.avatar_url.clone().unwrap_or_default(),
        is_active: a.is_active.unwrap_or(0) == 1,
    }).collect();

    
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
        let active_info = accounts_info.iter().find(|a| a.is_active).cloned()
            .unwrap_or_else(|| accounts_info[0].clone());
        return Ok(AuthStatus {
            authenticated: true,
            active_account: Some(active_info),
            accounts: accounts_info,
        });
    }

    
    let refresh_token = active.refresh_token.clone().unwrap_or_default();
    let token_to_use = if refresh_token.is_empty() {
        Entry::new("rustymail", &active.id)
            .and_then(|e| e.get_password())
            .unwrap_or_default()
    } else {
        refresh_token
    };

    if token_to_use.is_empty() {
        return Ok(AuthStatus { authenticated: false, active_account: None, accounts: accounts_info });
    }

    match refresh_and_update(pool.inner(), &active.id, &token_to_use).await {
        Ok(_) => {
            let active_info = accounts_info.iter().find(|a| a.is_active).cloned()
                .unwrap_or_else(|| accounts_info[0].clone());
            Ok(AuthStatus {
                authenticated: true,
                active_account: Some(active_info),
                accounts: accounts_info,
            })
        }
        Err(_) => Ok(AuthStatus { authenticated: false, active_account: None, accounts: accounts_info }),
    }
}

async fn refresh_and_update(pool: &sqlx::SqlitePool, account_id: &str, refresh_token: &str) -> Result<(), String> {
    let client_id = std::env::var("RUSTYMAIL_CLIENT_ID")
        .map_err(|_| "RUSTYMAIL_CLIENT_ID not found".to_string())?;
    let client_secret = std::env::var("RUSTYMAIL_CLIENT_SECRET")
        .map_err(|_| "RUSTYMAIL_CLIENT_SECRET not found".to_string())?;

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
        return Err("Token refresh failed".to_string());
    }

    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let new_access_token = body["access_token"].as_str().unwrap_or_default();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3500);
    let new_expiry = chrono::Utc::now().timestamp() + expires_in;

    
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
            sqlx::query("UPDATE accounts SET access_token = ?, token_expiry = ?, display_name = COALESCE(NULLIF(display_name, ''), ?), avatar_url = COALESCE(NULLIF(avatar_url, ''), ?), email = COALESCE(NULLIF(email, 'unknown@gmail.com'), ?) WHERE id = ?")
                .bind(new_access_token)
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

    sqlx::query("UPDATE accounts SET access_token = ?, token_expiry = ? WHERE id = ?")
        .bind(new_access_token)
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

    Ok(rows.into_iter().map(|r| AccountInfo {
        id: r.id,
        email: r.email.unwrap_or_default(),
        display_name: r.display_name.unwrap_or_default(),
        avatar_url: r.avatar_url.unwrap_or_default(),
        is_active: r.is_active.unwrap_or(0) == 1,
    }).collect())
}

#[tauri::command]
pub async fn switch_account(app_handle: tauri::AppHandle, account_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    sqlx::query("UPDATE accounts SET is_active = 0").execute(pool.inner()).await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE accounts SET is_active = 1 WHERE id = ?")
        .bind(&account_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_account(app_handle: tauri::AppHandle, account_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    
    
    if let Ok(entry) = Entry::new("rustymail", &account_id) {
        let _ = entry.delete_password();
    }

    
    let mut tx = pool.inner().begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM messages WHERE account_id = ?").bind(&account_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM threads WHERE account_id = ?").bind(&account_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM labels WHERE account_id = ?").bind(&account_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM accounts WHERE id = ?").bind(&account_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
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
    struct Row { key: String, value: String }

    let rows: Vec<Row> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(|r| SettingEntry { key: r.key, value: r.value }).collect())
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
pub async fn update_setting(app_handle: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
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
pub async fn sync_gmail_data(app_handle: tauri::AppHandle) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    println!("[Sync] Starting fast sync for: {}", account.id);

    
    crate::gmail_api::fetch_and_store_labels(pool.inner(), &account.id, &account.access_token).await?;

    
    crate::gmail_api::fetch_and_store_threads(
        pool.inner(), &account.id, &account.access_token,
        Some(&["INBOX"]), 100,
    ).await?;

    let inbox_unhydrated = crate::gmail_api::get_unhydrated_thread_ids(pool.inner(), &account.id).await;
    if !inbox_unhydrated.is_empty() {
        let batch: Vec<String> = inbox_unhydrated.into_iter().take(100).collect();
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(), &account.id, &account.access_token, batch
        ).await;
    }


    
    let bg_pool = pool.inner().clone();
    let bg_account_id = account.id.clone();
    let bg_token = account.access_token.clone();
    let bg_app = app_handle.clone();
    tokio::spawn(async move {
        let all_unhydrated = crate::gmail_api::get_unhydrated_thread_ids(&bg_pool, &bg_account_id).await;
        if !all_unhydrated.is_empty() {
            crate::gmail_api::batch_hydrate_threads(
                &bg_pool, &bg_account_id, &bg_token, all_unhydrated
            ).await;
        }


        
        let app_dir = bg_app.path().app_data_dir().unwrap_or_default();
        let db_path = app_dir.join("rustymail.db");
        let db_size_mb = std::fs::metadata(&db_path).map(|m| m.len() / (1024 * 1024)).unwrap_or(0);
        #[derive(sqlx::FromRow)]
        struct S { value: String }
        let max_mb: u64 = sqlx::query_as::<_, S>("SELECT value FROM settings WHERE key = 'max_cache_mb'")
            .fetch_optional(&bg_pool).await.unwrap_or(None)
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
    struct LabelRow { id: String, name: Option<String>, r#type: Option<String>, unread_count: Option<i32> }

    let rows: Vec<LabelRow> = sqlx::query_as("SELECT id, name, type, unread_count FROM labels WHERE account_id = ? ORDER BY name ASC")
        .bind(&account.id)
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(|r| LocalLabel {
        id: r.id,
        name: r.name.unwrap_or_default(),
        r#type: r.r#type.unwrap_or_default(),
        unread_count: r.unread_count.unwrap_or(0),
    }).collect())
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
}

fn clean_sender_name(raw: Option<String>) -> String {
    let mut s = raw.unwrap_or_else(|| "Unknown Sender".to_string());
    if let Some(idx) = s.find('<') {
        let name = s[..idx].trim();
        if !name.is_empty() { s = name.to_string(); }
        else { s = s.replace("<", "").replace(">", "").trim().to_string(); }
    }
    s.replace("\"", "")
}

#[tauri::command]
pub async fn get_threads(app_handle: tauri::AppHandle, label_id: Option<String>, offset: Option<i32>, limit: Option<i32>) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let lim = limit.unwrap_or(50);
    let off = offset.unwrap_or(0);
    
    #[derive(sqlx::FromRow)]
    struct ThreadRow {
        id: String, snippet: Option<String>, history_id: Option<String>,
        unread: Option<i32>, sender: Option<String>, subject: Option<String>, msg_date: Option<i64>,
    }

    let rows: Vec<ThreadRow> = if let Some(ref lid) = label_id {
        sqlx::query_as(
            "SELECT t.id, t.snippet, t.history_id, t.unread,
                    (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                    (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                    (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date
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
                    (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date
             FROM threads t
             WHERE t.account_id = ?
             ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC, t.rowid DESC
             LIMIT ? OFFSET ?"
        ).bind(&account.id).bind(lim).bind(off)
        .fetch_all(pool.inner()).await.map_err(|e| e.to_string())?
    };

    Ok(rows.into_iter().map(|r| LocalThread {
        id: r.id, snippet: r.snippet.unwrap_or_default(), history_id: r.history_id.unwrap_or_default(),
        unread: r.unread.unwrap_or(0), sender: clean_sender_name(r.sender),
        subject: r.subject.unwrap_or_else(|| "No Subject".to_string()), internal_date: r.msg_date.unwrap_or(0),
    }).collect())
}


#[tauri::command]
pub async fn fetch_label_threads(app_handle: tauri::AppHandle, label_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    println!("[OnDemand] Fetching threads for label: {}", label_id);
    crate::gmail_api::fetch_and_store_threads(
        pool.inner(), &account.id, &account.access_token,
        Some(&[label_id.as_str()]), 50,
    ).await?;
    
    let unhydrated = crate::gmail_api::get_unhydrated_thread_ids(pool.inner(), &account.id).await;
    if !unhydrated.is_empty() {
        let batch: Vec<String> = unhydrated.into_iter().take(50).collect();
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(), &account.id, &account.access_token, batch
        ).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn sync_thread_messages(app_handle: tauri::AppHandle, thread_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::fetch_messages_for_thread(pool.inner(), &account.id, &account.access_token, &thread_id).await
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalMessage {
    pub id: String, pub thread_id: String, pub sender: String, pub recipients: String,
    pub subject: String, pub snippet: String, pub internal_date: i64,
    pub body_html: String, pub body_plain: String,
}

#[tauri::command]
pub async fn get_messages(app_handle: tauri::AppHandle, thread_id: String) -> Result<Vec<LocalMessage>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String, thread_id: Option<String>, sender: Option<String>, recipients: Option<String>,
        subject: Option<String>, snippet: Option<String>, internal_date: Option<i64>,
        body_html: Option<String>, body_plain: Option<String>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, thread_id, sender, recipients, subject, snippet, internal_date, body_html, body_plain 
         FROM messages WHERE thread_id = ? ORDER BY internal_date ASC"
    ).bind(thread_id).fetch_all(pool.inner()).await.map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(|r| LocalMessage {
        id: r.id, thread_id: r.thread_id.unwrap_or_default(), sender: r.sender.unwrap_or_default(),
        recipients: r.recipients.unwrap_or_default(), subject: r.subject.unwrap_or_default(),
        snippet: r.snippet.unwrap_or_default(), internal_date: r.internal_date.unwrap_or(0),
        body_plain: r.body_plain.unwrap_or_default(), body_html: r.body_html.unwrap_or_default(),
    }).collect())
}





#[tauri::command]
pub async fn archive_thread(app_handle: tauri::AppHandle, thread_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::modify_thread(pool.inner(), &account.id, &account.access_token, &thread_id, vec![], vec!["INBOX".to_string()]).await
}

#[tauri::command]
pub async fn move_thread_to_trash(app_handle: tauri::AppHandle, thread_id: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::gmail_api::trash_thread(pool.inner(), &account.id, &account.access_token, &thread_id).await
}

#[tauri::command]
pub async fn mark_thread_read_status(app_handle: tauri::AppHandle, thread_id: String, is_read: bool) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let (add, remove) = if is_read { (vec![], vec!["UNREAD".to_string()]) } else { (vec!["UNREAD".to_string()], vec![]) };
    crate::gmail_api::modify_thread(pool.inner(), &account.id, &account.access_token, &thread_id, add, remove).await
}





#[tauri::command]
pub async fn search_messages(app_handle: tauri::AppHandle, query: String) -> Result<Vec<LocalThread>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let mut all_thread_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    
    #[derive(sqlx::FromRow)]
    struct FtsRow { thread_id: Option<String> }
    let fts_query = format!("{}*", query.replace('"', ""));
    let local: Vec<FtsRow> = sqlx::query_as(
        "SELECT DISTINCT m.thread_id FROM messages m 
         INNER JOIN messages_fts ON messages_fts.rowid = m.rowid 
         WHERE messages_fts MATCH ? AND m.account_id = ?
         LIMIT 50"
    ).bind(&fts_query).bind(&account.id)
    .fetch_all(pool.inner()).await.unwrap_or_default();

    for r in local {
        if let Some(tid) = r.thread_id {
            if seen.insert(tid.clone()) { all_thread_ids.push(tid); }
        }
    }

    #[derive(sqlx::FromRow)]
    struct LikeRow { thread_id: Option<String> }
    let pattern = format!("%{}%", query);
    let like_results: Vec<LikeRow> = sqlx::query_as(
        "SELECT DISTINCT thread_id FROM messages WHERE account_id = ? AND (sender LIKE ? OR subject LIKE ?) LIMIT 30"
    ).bind(&account.id).bind(&pattern).bind(&pattern)
    .fetch_all(pool.inner()).await.unwrap_or_default();
    for r in like_results {
        if let Some(tid) = r.thread_id {
            if seen.insert(tid.clone()) { all_thread_ids.push(tid); }
        }
    }

    let api_ids = search_gmail_api(&account.access_token, &query).await;
    for tid in api_ids {
        if seen.insert(tid.clone()) { all_thread_ids.push(tid); }
    }

    let mut need_hydrate: Vec<String> = Vec::new();
    for tid in &all_thread_ids {
        #[derive(sqlx::FromRow)]
        struct C { cnt: i32 }
        let cnt = sqlx::query_as::<_, C>("SELECT COUNT(*) as cnt FROM messages WHERE thread_id = ?")
            .bind(tid).fetch_one(pool.inner()).await.map(|r| r.cnt).unwrap_or(0);
        if cnt == 0 {
            let _ = sqlx::query("INSERT OR IGNORE INTO threads (id, account_id, snippet, history_id, unread) VALUES (?, ?, '', '', 0)")
                .bind(tid).bind(&account.id)
                .execute(pool.inner()).await;
            need_hydrate.push(tid.clone());
        }
    }
    if !need_hydrate.is_empty() {
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(), &account.id, &account.access_token, need_hydrate
        ).await;
    }

    fetch_threads_by_ids(pool.inner(), &all_thread_ids, &account.id).await
}

async fn search_gmail_api(access_token: &str, query: &str) -> Vec<String> {
    let client = reqwest::Client::new();
    let res = match client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
        .query(&[("q", query), ("maxResults", "30")])
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await {
        Ok(r) => r,
        Err(_) => return vec![], 
    };
    if !res.status().is_success() { return vec![]; }

    #[derive(serde::Deserialize)]
    struct MsgRef { #[allow(dead_code)] id: String, #[serde(rename = "threadId")] thread_id: String }
    #[derive(serde::Deserialize)]
    struct MsgsResponse { messages: Option<Vec<MsgRef>> }

    match res.json::<MsgsResponse>().await {
        Ok(api_res) => {
            if let Some(msgs) = api_res.messages {
                let mut seen = std::collections::HashSet::new();
                msgs.into_iter()
                    .filter(|m| seen.insert(m.thread_id.clone()))
                    .map(|m| m.thread_id)
                    .collect()
            } else { vec![] }
        }
        Err(_) => vec![],
    }
}

async fn fetch_threads_by_ids(pool: &sqlx::SqlitePool, ids: &[String], account_id: &str) -> Result<Vec<LocalThread>, String> {
    if ids.is_empty() { return Ok(vec![]); }
    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT t.id, t.snippet, t.history_id, t.unread,
                (SELECT m2.sender FROM messages m2 WHERE m2.thread_id = t.id ORDER BY m2.internal_date DESC LIMIT 1) as sender,
                (SELECT m3.subject FROM messages m3 WHERE m3.thread_id = t.id ORDER BY m3.internal_date DESC LIMIT 1) as subject,
                (SELECT MAX(m4.internal_date) FROM messages m4 WHERE m4.thread_id = t.id) as msg_date
         FROM threads t
         WHERE t.id IN ({}) AND t.account_id = ?
         ORDER BY COALESCE((SELECT MAX(m5.internal_date) FROM messages m5 WHERE m5.thread_id = t.id), 0) DESC",
        placeholders.join(",")
    );
    
    #[derive(sqlx::FromRow)]
    struct TR { id: String, snippet: Option<String>, history_id: Option<String>, unread: Option<i32>, sender: Option<String>, subject: Option<String>, msg_date: Option<i64> }

    let mut q = sqlx::query_as::<_, TR>(&sql);
    for tid in ids { q = q.bind(tid); }
    q = q.bind(account_id);
    
    let rows = q.fetch_all(pool).await.unwrap_or_default();
    Ok(rows.into_iter().map(|r| LocalThread {
        id: r.id, snippet: r.snippet.unwrap_or_default(), history_id: r.history_id.unwrap_or_default(),
        unread: r.unread.unwrap_or(0), sender: clean_sender_name(r.sender),
        subject: r.subject.unwrap_or_else(|| "No Subject".to_string()), internal_date: r.msg_date.unwrap_or(0),
    }).collect())
}





#[derive(serde::Serialize)]
pub struct HydrationProgress {
    pub total: usize,
    pub hydrated: usize,
}

#[tauri::command]
pub async fn get_hydration_progress(app_handle: tauri::AppHandle) -> Result<HydrationProgress, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    
    #[derive(sqlx::FromRow)]
    struct Count { cnt: i32 }
    
    let total = sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM threads WHERE account_id = ?")
        .bind(&account.id).fetch_one(pool.inner()).await.map(|r| r.cnt).unwrap_or(0) as usize;
    
    let hydrated = sqlx::query_as::<_, Count>(
        "SELECT COUNT(DISTINCT t.id) as cnt FROM threads t INNER JOIN messages m ON t.id = m.thread_id WHERE t.account_id = ?"
    ).bind(&account.id).fetch_one(pool.inner()).await.map(|r| r.cnt).unwrap_or(0) as usize;
    
    Ok(HydrationProgress { total, hydrated })
}

#[tauri::command]
pub async fn ensure_threads_hydrated(app_handle: tauri::AppHandle, thread_ids: Vec<String>) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    
    
    let mut need_hydration = Vec::new();
    for tid in &thread_ids {
        #[derive(sqlx::FromRow)]
        struct Count { cnt: i32 }
        let has_msgs = sqlx::query_as::<_, Count>("SELECT COUNT(*) as cnt FROM messages WHERE thread_id = ?")
            .bind(tid).fetch_one(pool.inner()).await.map(|r| r.cnt).unwrap_or(0);
        if has_msgs == 0 {
            need_hydration.push(tid.clone());
        }
    }
    if !need_hydration.is_empty() {
        crate::gmail_api::batch_hydrate_threads(
            pool.inner(), &account.id, &account.access_token, need_hydration
        ).await;
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
pub async fn get_search_suggestions(app_handle: tauri::AppHandle, partial: String) -> Result<Vec<SearchSuggestion>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    let mut suggestions = Vec::new();

    #[derive(sqlx::FromRow)]
    struct SettingRow { value: String }
    if let Ok(Some(row)) = sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
        .fetch_optional(pool.inner()).await {
        if let Ok(recents) = serde_json::from_str::<Vec<String>>(&row.value) {
            for r in recents.iter().take(5) {
                if partial.is_empty() || r.to_lowercase().contains(&partial.to_lowercase()) {
                    suggestions.push(SearchSuggestion {
                        kind: "recent".to_string(),
                        text: r.clone(),
                        detail: "Recent search".to_string(),
                    });
                }
            }
        }
    }

    if partial.len() >= 2 {
        #[derive(sqlx::FromRow)]
        struct SenderRow { sender: String }
        let pattern = format!("%{}%", partial);
        let contacts: Vec<SenderRow> = sqlx::query_as(
            "SELECT DISTINCT sender FROM messages WHERE account_id = ? AND sender LIKE ? LIMIT 5"
        ).bind(&account.id).bind(&pattern)
        .fetch_all(pool.inner()).await.unwrap_or_default();
        
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
            "SELECT DISTINCT subject FROM messages WHERE account_id = ? AND subject LIKE ? LIMIT 3"
        ).bind(&account.id).bind(&pattern)
        .fetch_all(pool.inner()).await.unwrap_or_default();
        
        for s in subjects {
            suggestions.push(SearchSuggestion {
                kind: "subject".to_string(),
                text: format!("subject:{}", s.subject),
                detail: s.subject.clone(),
            });
        }
    }
    
    Ok(suggestions)
}

#[tauri::command]
pub async fn save_recent_search(app_handle: tauri::AppHandle, query: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    
    #[derive(sqlx::FromRow)]
    struct SettingRow { value: String }
    let mut recents: Vec<String> = sqlx::query_as::<_, SettingRow>("SELECT value FROM settings WHERE key = 'recent_searches'")
        .fetch_optional(pool.inner()).await.unwrap_or(None)
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
pub async fn send_message(app_handle: tauri::AppHandle, to: String, subject: String, body: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;

    #[derive(sqlx::FromRow)]
    struct EmailRow { email: String }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;

    crate::gmail_api::send_message(&account.id, &row.email, &account.access_token, &to, &subject, &body).await
}


#[tauri::command]
pub async fn save_draft(app_handle: tauri::AppHandle, to: String, subject: String, body: String) -> Result<(), String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    
    #[derive(sqlx::FromRow)]
    struct EmailRow { email: String }
    let row = sqlx::query_as::<_, EmailRow>("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.id).fetch_one(pool.inner()).await.map_err(|e| e.to_string())?;
        
    crate::gmail_api::save_draft(&account.id, &row.email, &account.access_token, &to, &subject, &body).await
}

#[tauri::command]
pub async fn get_upcoming_events(app_handle: tauri::AppHandle) -> Result<Vec<crate::calendar_api::CalendarEvent>, String> {
    let pool = app_handle.state::<sqlx::SqlitePool>();
    let account = get_active_account(pool.inner()).await?;
    crate::calendar_api::get_upcoming_events(&account.access_token).await
}
