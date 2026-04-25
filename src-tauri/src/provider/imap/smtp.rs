use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use lettre::transport::smtp::authentication::Credentials;

pub async fn send_via_smtp(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    message: &lettre::Message,
) -> Result<(), String> {
    let creds = Credentials::new(username.to_string(), password.to_string());

    let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
        .map_err(|e| format!("SMTP relay setup failed: {}", e))?
        .port(port)
        .credentials(creds)
        .build();

    transport
        .send(message.clone())
        .await
        .map_err(|e| format!("SMTP send failed: {}", e))?;

    tracing::info!("Message sent via SMTP through {}", host);
    Ok(())
}

pub async fn test_smtp_connection(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let creds = Credentials::new(username.to_string(), password.to_string());

    let transport: AsyncSmtpTransport<Tokio1Executor> = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
        .map_err(|e| format!("SMTP relay setup failed: {}", e))?
        .port(port)
        .credentials(creds)
        .build();

    transport
        .test_connection()
        .await
        .map_err(|e| format!("SMTP connection test failed: {}", e))?;

    Ok(format!("SMTP connection to {}:{} successful", host, port))
}
