use super::connection::ImapSession;
use futures::StreamExt;

pub async fn set_flags(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    flags: &str,
    add: bool,
) -> Result<(), String> {
    if uids.is_empty() {
        return Ok(());
    }

    session
        .select(folder)
        .await
        .map_err(|e| format!("SELECT {} failed: {}", folder, e))?;

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let query = if add {
        format!("+FLAGS ({})", flags)
    } else {
        format!("-FLAGS ({})", flags)
    };

    let store_stream = session
        .uid_store(&uid_set, &query)
        .await
        .map_err(|e| format!("STORE failed: {}", e))?;

    let _: Vec<_> = store_stream.filter_map(|r| async { r.ok() }).collect().await;

    Ok(())
}

pub async fn move_messages(
    session: &mut ImapSession,
    from_folder: &str,
    to_folder: &str,
    uids: &[u32],
) -> Result<(), String> {
    if uids.is_empty() {
        return Ok(());
    }

    session
        .select(from_folder)
        .await
        .map_err(|e| format!("SELECT {} failed: {}", from_folder, e))?;

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    session
        .uid_copy(&uid_set, to_folder)
        .await
        .map_err(|e| format!("COPY to {} failed: {}", to_folder, e))?;

    let store_stream = session
        .uid_store(&uid_set, "+FLAGS (\\Deleted)")
        .await
        .map_err(|e| format!("STORE \\Deleted failed: {}", e))?;

    let _: Vec<_> = store_stream.filter_map(|r| async { r.ok() }).collect().await;

    let expunge_stream = session
        .expunge()
        .await
        .map_err(|e| format!("EXPUNGE failed: {}", e))?;

    let _: Vec<_> = expunge_stream.filter_map(|r| async { r.ok() }).collect().await;

    Ok(())
}

pub async fn mark_read(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    read: bool,
) -> Result<(), String> {
    set_flags(session, folder, uids, "\\Seen", read).await
}

pub async fn set_starred(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    starred: bool,
) -> Result<(), String> {
    set_flags(session, folder, uids, "\\Flagged", starred).await
}

pub async fn trash_messages(
    session: &mut ImapSession,
    from_folder: &str,
    trash_folder: &str,
    uids: &[u32],
) -> Result<(), String> {
    move_messages(session, from_folder, trash_folder, uids).await
}

pub async fn archive_messages(
    session: &mut ImapSession,
    from_folder: &str,
    archive_folder: &str,
    uids: &[u32],
) -> Result<(), String> {
    move_messages(session, from_folder, archive_folder, uids).await
}
