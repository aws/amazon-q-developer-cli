use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub struct LspMessage {
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

pub async fn read_lsp_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<String> {
    let mut headers = HashMap::new();
    let mut line = String::new();

    // Read headers
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        // Handle EOF - connection closed
        if bytes_read == 0 {
            return Err(anyhow::anyhow!("Connection closed by language server"));
        }

        if line.trim().is_empty() {
            break;
        }

        if let Some((key, value)) = line.trim().split_once(": ") {
            headers.insert(key.to_lowercase(), value.to_string());
        }
    }

    // Get content length
    let content_length: usize = headers
        .get("content-length")
        .ok_or_else(|| anyhow::anyhow!("Missing Content-Length header"))?
        .parse()?;

    // Read content
    let mut buffer = vec![0; content_length];
    reader.read_exact(&mut buffer).await?;

    let content = String::from_utf8(buffer)?;

    Ok(content)
}

pub async fn write_lsp_message<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    content: &str,
) -> Result<()> {
    let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);
    writer.write_all(message.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

pub fn parse_lsp_message(content: &str) -> Result<LspMessage> {
    let json: Value = serde_json::from_str(content)?;

    Ok(LspMessage {
        id: json.get("id").cloned(),
        method: json
            .get("method")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default(), // Use unwrap_or_default() instead of unwrap_or("")
        params: json.get("params").cloned(),
        result: json.get("result").cloned(),
        error: json.get("error").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_lsp_request_message() {
        let content = r#"{"id":1,"method":"textDocument/definition","params":{"textDocument":{"uri":"file:///test.rs"},"position":{"line":0,"character":5}}}"#;
        
        let message = parse_lsp_message(content).unwrap();
        
        assert_eq!(message.id, Some(json!(1)));
        assert_eq!(message.method, "textDocument/definition");
        assert!(message.params.is_some());
        assert!(message.result.is_none());
        assert!(message.error.is_none());
    }

    #[test]
    fn test_parse_lsp_response_message() {
        let content = r#"{"id":1,"result":{"uri":"file:///test.rs","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":10}}}}"#;
        
        let message = parse_lsp_message(content).unwrap();
        
        assert_eq!(message.id, Some(json!(1)));
        assert_eq!(message.method, "");
        assert!(message.params.is_none());
        assert!(message.result.is_some());
        assert!(message.error.is_none());
    }

    #[test]
    fn test_parse_lsp_notification_message() {
        let content = r#"{"method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///test.rs","languageId":"rust","version":1,"text":"fn main() {}"}}}"#;
        
        let message = parse_lsp_message(content).unwrap();
        
        assert!(message.id.is_none());
        assert_eq!(message.method, "textDocument/didOpen");
        assert!(message.params.is_some());
        assert!(message.result.is_none());
        assert!(message.error.is_none());
    }

    #[test]
    fn test_parse_lsp_error_message() {
        let content = r#"{"id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        
        let message = parse_lsp_message(content).unwrap();
        
        assert_eq!(message.id, Some(json!(1)));
        assert_eq!(message.method, "");
        assert!(message.params.is_none());
        assert!(message.result.is_none());
        assert!(message.error.is_some());
    }

    #[test]
    fn test_parse_lsp_message_invalid_json() {
        let content = r#"{"invalid": json"#;
        
        let result = parse_lsp_message(content);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_lsp_message_empty_content() {
        let content = "{}";
        
        let message = parse_lsp_message(content).unwrap();
        
        assert!(message.id.is_none());
        assert_eq!(message.method, "");
        assert!(message.params.is_none());
        assert!(message.result.is_none());
        assert!(message.error.is_none());
    }
}
