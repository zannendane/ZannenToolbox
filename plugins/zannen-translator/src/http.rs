//! HTTP 辅助：手写 multipart/form-data 编码与 URL 百分号编码（纯函数）。

/// multipart 一个字段：文本或文件。
#[derive(Debug, Clone)]
pub struct MultipartField {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

impl MultipartField {
    pub fn text(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            filename: None,
            content_type: None,
            data: value.as_bytes().to_vec(),
        }
    }

    pub fn file(name: &str, filename: &str, content_type: &str, data: Vec<u8>) -> Self {
        Self {
            name: name.to_string(),
            filename: Some(filename.to_string()),
            content_type: Some(content_type.to_string()),
            data,
        }
    }
}

/// 生成一次请求内唯一的 boundary。
pub fn new_boundary() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("zannen-{nanos:032x}")
}

pub fn multipart_content_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// multipart/form-data 编码（CRLF 分隔，字段顺序保持）。
pub fn multipart_encode(boundary: &str, fields: &[MultipartField]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fields {
        out.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"",
                f.name
            )
            .as_bytes(),
        );
        if let Some(filename) = &f.filename {
            out.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if let Some(ct) = &f.content_type {
            out.extend_from_slice(format!("Content-Type: {ct}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&f.data);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}

/// application/x-www-form-urlencoded 百分号编码（unreserved 集保守保留）。
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 逐条解析 SSE（Server-Sent Events）流的 `data:` 载荷；`[DONE]` 终止。
/// 每个 data 载荷调用一次回调；非 data 行（注释/事件名/空行）忽略。
pub fn for_each_sse_data(
    mut reader: impl std::io::BufRead,
    mut on_data: impl FnMut(&str),
) -> Result<(), std::io::Error> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break; // EOF
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let Some(data) = trimmed.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim_start();
        if data == "[DONE]" {
            break;
        }
        on_data(data);
    }
    Ok(())
}

/// 从 OpenAI 兼容 SSE chunk JSON 提取增量文本（choices[0].delta.content）。
pub fn sse_delta_content(chunk_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(chunk_json).ok()?;
    v.pointer("/choices/0/delta/content")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_structure() {
        let fields = vec![
            MultipartField::text("model", "whisper-1"),
            MultipartField::file("file", "audio.wav", "audio/wav", b"RIFF".to_vec()),
        ];
        let body = multipart_encode("BOUND", &fields);
        let text = String::from_utf8(body).unwrap();
        for part in [
            "--BOUND\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n",
            "--BOUND\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFF\r\n",
            "--BOUND--\r\n",
        ] {
            assert!(text.contains(part), "missing part: {part:?}\nbody:\n{text}");
        }
        assert!(text.starts_with("--BOUND\r\n"));
        assert!(text.ends_with("--BOUND--\r\n"));
    }

    #[test]
    fn multipart_content_type_header() {
        assert_eq!(
            multipart_content_type("abc"),
            "multipart/form-data; boundary=abc"
        );
    }

    #[test]
    fn url_encode_rules() {
        assert_eq!(url_encode("abc-DEF_123.~"), "abc-DEF_123.~");
        assert_eq!(url_encode("a b+c&d=e"), "a%20b%2Bc%26d%3De");
        // UTF-8 多字节逐字节百分号编码
        assert_eq!(url_encode("中文"), "%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn sse_streaming_parsing() {
        let raw = concat!(
            ": comment\n",
            "\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n",
            "data: [DONE]\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ignored\"}}]}\n",
        );
        let mut deltas = Vec::new();
        for_each_sse_data(std::io::BufReader::new(raw.as_bytes()), |d| {
            if let Some(c) = sse_delta_content(d) {
                deltas.push(c);
            }
        })
        .unwrap();
        assert_eq!(deltas, vec!["Hello".to_string(), " world".to_string()]);
    }
}
