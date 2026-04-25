use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredConfig {
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub use_tls: bool,
    pub source: String,
}

pub async fn discover_settings(email: &str) -> Result<DiscoveredConfig, String> {
    let domain = email
        .split('@')
        .nth(1)
        .ok_or_else(|| "Invalid email address".to_string())?
        .to_lowercase();

    if let Ok(config) = query_mozilla_ispdb(&domain).await {
        return Ok(config);
    }

    if let Ok(config) = query_autoconfig(&domain).await {
        return Ok(config);
    }

    Err(format!("Could not auto-discover settings for {}", domain))
}

async fn query_mozilla_ispdb(domain: &str) -> Result<DiscoveredConfig, String> {
    let url = format!(
        "https://autoconfig.thunderbird.net/v1.1/{}",
        domain
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("ISPDB request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("ISPDB returned {}", resp.status()));
    }

    let xml = resp.text().await.map_err(|e| e.to_string())?;
    parse_autoconfig_xml(&xml, "mozilla-ispdb")
}

async fn query_autoconfig(domain: &str) -> Result<DiscoveredConfig, String> {
    let url = format!(
        "https://autoconfig.{}/mail/config-v1.1.xml",
        domain
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Autoconfig request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Autoconfig returned {}", resp.status()));
    }

    let xml = resp.text().await.map_err(|e| e.to_string())?;
    parse_autoconfig_xml(&xml, "autoconfig")
}

fn parse_autoconfig_xml(xml: &str, source: &str) -> Result<DiscoveredConfig, String> {
    let mut imap_host = String::new();
    let mut imap_port: u16 = 993;
    let mut smtp_host = String::new();
    let mut smtp_port: u16 = 587;
    let mut use_tls = true;

    let mut in_imap = false;
    let mut in_smtp = false;

    for line in xml.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<incomingServer") && trimmed.contains("\"imap\"") {
            in_imap = true;
            in_smtp = false;
        } else if trimmed.contains("<outgoingServer") {
            in_smtp = true;
            in_imap = false;
        } else if trimmed.contains("</incomingServer") {
            in_imap = false;
        } else if trimmed.contains("</outgoingServer") {
            in_smtp = false;
        }

        if let Some(value) = extract_xml_value(trimmed, "hostname") {
            if in_imap {
                imap_host = value;
            } else if in_smtp {
                smtp_host = value;
            }
        }

        if let Some(value) = extract_xml_value(trimmed, "port") {
            if let Ok(port) = value.parse::<u16>() {
                if in_imap {
                    imap_port = port;
                } else if in_smtp {
                    smtp_port = port;
                }
            }
        }

        if let Some(value) = extract_xml_value(trimmed, "socketType") {
            if in_imap {
                use_tls = value != "plain";
            }
        }
    }

    if imap_host.is_empty() || smtp_host.is_empty() {
        return Err("Incomplete autoconfig: missing IMAP or SMTP host".to_string());
    }

    Ok(DiscoveredConfig {
        imap_host,
        imap_port,
        smtp_host,
        smtp_port,
        use_tls,
        source: source.to_string(),
    })
}

fn extract_xml_value(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = line.find(&open) {
        if let Some(end) = line.find(&close) {
            let value_start = start + open.len();
            if value_start < end {
                return Some(line[value_start..end].trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_AUTOCONFIG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig version="1.1">
  <emailProvider id="fastmail.com">
    <domain>fastmail.com</domain>
    <incomingServer type="imap">
      <hostname>imap.fastmail.com</hostname>
      <port>993</port>
      <socketType>SSL</socketType>
      <authentication>password-cleartext</authentication>
      <username>%EMAILADDRESS%</username>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>smtp.fastmail.com</hostname>
      <port>465</port>
      <socketType>SSL</socketType>
      <authentication>password-cleartext</authentication>
      <username>%EMAILADDRESS%</username>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;

    #[test]
    fn test_parse_autoconfig_xml() {
        let config = parse_autoconfig_xml(SAMPLE_AUTOCONFIG, "test").unwrap();
        assert_eq!(config.imap_host, "imap.fastmail.com");
        assert_eq!(config.imap_port, 993);
        assert_eq!(config.smtp_host, "smtp.fastmail.com");
        assert_eq!(config.smtp_port, 465);
        assert!(config.use_tls);
        assert_eq!(config.source, "test");
    }

    #[test]
    fn test_parse_autoconfig_incomplete() {
        let xml = r#"<clientConfig><emailProvider></emailProvider></clientConfig>"#;
        let result = parse_autoconfig_xml(xml, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Incomplete"));
    }

    #[test]
    fn test_extract_xml_value() {
        assert_eq!(extract_xml_value("<hostname>imap.test.com</hostname>", "hostname"), Some("imap.test.com".to_string()));
        assert_eq!(extract_xml_value("<port>993</port>", "port"), Some("993".to_string()));
        assert_eq!(extract_xml_value("<other>stuff</other>", "hostname"), None);
        assert_eq!(extract_xml_value("no tags here", "hostname"), None);
    }

    const OUTLOOK_AUTOCONFIG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig version="1.1">
  <emailProvider id="outlook.com">
    <domain>outlook.com</domain>
    <incomingServer type="imap">
      <hostname>outlook.office365.com</hostname>
      <port>993</port>
      <socketType>SSL</socketType>
      <authentication>OAuth2</authentication>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>smtp.office365.com</hostname>
      <port>587</port>
      <socketType>STARTTLS</socketType>
      <authentication>OAuth2</authentication>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;

    #[test]
    fn test_parse_outlook_autoconfig() {
        let config = parse_autoconfig_xml(OUTLOOK_AUTOCONFIG, "test").unwrap();
        assert_eq!(config.imap_host, "outlook.office365.com");
        assert_eq!(config.imap_port, 993);
        assert_eq!(config.smtp_host, "smtp.office365.com");
        assert_eq!(config.smtp_port, 587);
    }

    #[test]
    fn test_parse_plain_socket_type() {
        let xml = r#"<clientConfig>
  <emailProvider>
    <incomingServer type="imap">
      <hostname>mail.test.com</hostname>
      <port>143</port>
      <socketType>plain</socketType>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>mail.test.com</hostname>
      <port>25</port>
      <socketType>plain</socketType>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;
        let config = parse_autoconfig_xml(xml, "test").unwrap();
        assert_eq!(config.imap_port, 143);
        assert_eq!(config.smtp_port, 25);
        assert!(!config.use_tls);
    }
}
