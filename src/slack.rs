use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use std::time::Duration;

pub struct SlackClient {
    client: Client,
    webhook_url: String,
}

impl SlackClient {
    pub fn new(webhook_url: &str) -> Result<Self> {
        if webhook_url.trim().is_empty() {
            bail!("Slack webhook URL cannot be empty");
        }

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build Slack HTTP client")?;

        Ok(Self {
            client,
            webhook_url: webhook_url.to_string(),
        })
    }

    pub fn send_text(&self, text: &str) -> Result<()> {
        let response = self
            .client
            .post(&self.webhook_url)
            .header(CONTENT_TYPE, "application/json")
            .body(slack_text_payload(text))
            .send()
            .context("failed to send Slack webhook request")?;

        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "<unreadable response body>".to_string());

        if !status.is_success() {
            bail!("Slack webhook returned {}: {}", status, body.trim());
        }

        Ok(())
    }
}

fn slack_text_payload(text: &str) -> String {
    serde_json::json!({ "text": text }).to_string()
}

#[cfg(test)]
mod tests {
    use super::{slack_text_payload, SlackClient};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn slack_payload_escapes_text_for_json() {
        let payload = slack_text_payload("hello \"world\"");
        assert_eq!(payload, r#"{"text":"hello \"world\""}"#);
    }

    #[test]
    fn send_text_posts_to_webhook() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let addr = listener.local_addr().expect("listener addr should load");

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("request should read");
            let request = String::from_utf8_lossy(&buffer[..read]);

            assert!(request.starts_with("POST / HTTP/1.1"));
            assert!(request.contains(r#"{"text":"hello from rssify"}"#));

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .expect("response should write");
        });

        let client = SlackClient::new(&format!("http://{}", addr)).expect("client should build");
        client
            .send_text("hello from rssify")
            .expect("send should succeed");

        server.join().expect("server thread should join");
    }
}
