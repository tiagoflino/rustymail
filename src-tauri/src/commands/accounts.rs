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
pub(crate) struct ActiveAccountFull {
    pub(crate) id: String,
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) token_expiry: Option<i64>,
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

pub(crate) async fn get_active_account(pool: &sqlx::SqlitePool) -> Result<ActiveAccountFull, String> {
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

pub(crate) async fn refresh_and_update(
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;

    #[tokio::test]
    async fn test_refresh_and_update_missing_client_id() {
        std::env::remove_var("RUSTYMAIL_CLIENT_ID");
        std::env::set_var("RUSTYMAIL_CLIENT_SECRET", "dummy_secret");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_and_update_missing_client_secret() {
        std::env::set_var("RUSTYMAIL_CLIENT_ID", "dummy_id");
        std::env::remove_var("RUSTYMAIL_CLIENT_SECRET");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_and_update_http_error() {
        std::env::set_var("RUSTYMAIL_CLIENT_ID", "dummy_id");
        std::env::set_var("RUSTYMAIL_CLIENT_SECRET", "dummy_secret");
        let pool = setup_test_db().await;
        let result = refresh_and_update(&pool, "acc1", "refresh_token").await;
        assert!(result.is_err());
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
        assert_eq!(accounts[0].id, "acc1");
        assert_eq!(accounts[0].email, "a@test.com");
        assert!(accounts[0].is_active);
        assert_eq!(accounts[1].id, "acc2");
        assert!(!accounts[1].is_active);
    }

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

        let threads = super::super::threads::get_threads_inner(&pool, "acc1", None, 0, 50).await.unwrap();
        assert!(threads.is_empty());

        let messages = super::super::messages::get_messages_inner(&pool, "t1").await.unwrap();
        assert!(messages.is_empty());

        let labels = super::super::labels::get_labels_inner(&pool, "acc1").await.unwrap();
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

        let threads = super::super::threads::get_threads_inner(&pool, "acc2", None, 0, 50).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "t2");

        let messages = super::super::messages::get_messages_inner(&pool, "t2").await.unwrap();
        assert_eq!(messages.len(), 1);
    }

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
}
