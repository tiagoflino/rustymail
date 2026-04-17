use tauri::Manager;

#[tauri::command]
pub async fn summarize_thread(
    app_handle: tauri::AppHandle,
    thread_id: String,
) -> Result<String, String> {
    let engine = app_handle.state::<crate::llm::engine::LlmEngine>();
    let status = engine.get_status().await;

    match status {
        crate::llm::engine::EngineStatus::Ready { .. } => {}
        _ => return Err("No model loaded. Download and load a model in Settings > AI.".into()),
    }

    let pool = app_handle.state::<sqlx::SqlitePool>();

    let messages: Vec<(String, String)> = sqlx::query_as(
        "SELECT sender, body_plain FROM messages WHERE thread_id = ? ORDER BY internal_date ASC"
    )
    .bind(&thread_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| format!("Failed to fetch messages: {}", e))?;

    if messages.is_empty() {
        return Err("No messages in thread".into());
    }

    let mut prompt = String::from(
        "<start_of_turn>user\nSummarize the following email thread in 2-3 concise sentences. Focus on key points, decisions, and action items.\n\n"
    );

    for (sender, body) in &messages {
        let truncated = if body.len() > 2000 { &body[..2000] } else { body.as_str() };
        prompt.push_str(&format!("From: {}\n---\n{}\n\n", sender, truncated));
    }

    prompt.push_str("<end_of_turn>\n<start_of_turn>model\n");

    let summary = engine.generate(&prompt, crate::llm::engine::GenerateParams {
        max_tokens: 256,
        temperature: 0.3,
        top_p: 0.9,
        stop_sequences: vec!["<end_of_turn>".into()],
    })
    .await
    .map_err(|e| format!("Inference failed: {}", e))?;

    Ok(summary.trim().to_string())
}
