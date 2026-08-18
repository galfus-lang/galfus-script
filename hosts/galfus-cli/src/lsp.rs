use anyhow::Result;

use std::io::{self, BufRead, Read, Write};

pub fn run_lsp() -> Result<()> {
    let mut workspace = crate::workspace::workspace_with_native_catalog();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();

    loop {
        let mut content_length = 0;

        // Read headers
        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;
            if bytes_read == 0 {
                // EOF
                return Ok(());
            }

            let line = line.trim();
            if line.is_empty() {
                // Empty line means end of headers
                break;
            }

            if line.to_lowercase().starts_with("content-length:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    content_length = parts[1].trim().parse().unwrap_or(0);
                }
            }
        }

        if content_length == 0 {
            continue;
        }

        // Read body
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body)?;

        if let Ok(json_str) = String::from_utf8(body) {
            let responses = workspace.handle_lsp_message(&json_str);
            for response in responses {
                let output = format!("Content-Length: {}\r\n\r\n{}", response.len(), response);
                stdout.write_all(output.as_bytes())?;
                stdout.flush()?;
            }
        }
    }
}
