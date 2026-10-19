use bytes::Bytes;
use serde_json::Value;

/// One server-sent event. `raw` holds the original bytes including the blank-line terminator,
/// so unmodified events are forwarded byte-for-byte.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub raw: Bytes,
    pub event: Option<String>,
    pub data: String,
}

impl SseEvent {
    pub fn json(&self) -> Option<Value> {
        let data = self.data.trim();
        if data.is_empty() || data == "[DONE]" {
            return None;
        }
        serde_json::from_str(data).ok()
    }

    /// Event name from the `event:` line, falling back to the JSON `type` field.
    pub fn kind<'a>(&'a self, json: Option<&'a Value>) -> Option<&'a str> {
        self.event
            .as_deref()
            .or_else(|| json.and_then(|j| j.get("type")).and_then(Value::as_str))
    }
}

#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(end) = find_event_end(&self.buf) {
            let raw: Vec<u8> = self.buf.drain(..end).collect();
            if let Some(ev) = parse_event(raw) {
                out.push(ev);
            }
        }
        out
    }

    /// Whatever is left once the upstream closes; an unterminated final event still counts.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if self.buf.iter().all(u8::is_ascii_whitespace) {
            self.buf.clear();
            return None;
        }
        parse_event(std::mem::take(&mut self.buf))
    }
}

fn find_event_end(buf: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some(i + 2);
        }
        if i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
        i += 1;
    }
    None
}

fn parse_event(raw: Vec<u8>) -> Option<SseEvent> {
    let text = String::from_utf8_lossy(&raw);
    let mut event = None;
    let mut data: Vec<&str> = Vec::new();
    let mut has_field = false;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        has_field = true;
        match field {
            "event" => event = Some(value.to_string()),
            "data" => data.push(value),
            _ => {}
        }
    }
    if !has_field && text.trim().is_empty() {
        return None;
    }
    let data = data.join("\n");
    Some(SseEvent {
        raw: Bytes::from(raw),
        event,
        data,
    })
}

pub fn encode(event: Option<&str>, data: &Value) -> Bytes {
    let mut s = String::new();
    if let Some(e) = event {
        s.push_str("event: ");
        s.push_str(e);
        s.push('\n');
    }
    s.push_str("data: ");
    s.push_str(&data.to_string());
    s.push_str("\n\n");
    Bytes::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_across_chunks_and_keeps_raw() {
        let mut p = SseParser::default();
        assert!(p.push(b"event: message_start\ndata: {\"a\"").is_empty());
        let evs = p.push(b":1}\n\ndata: [DONE]\r\n\r\n: keepalive\n\n");
        assert_eq!(evs.len(), 3);
        assert_eq!(evs[0].event.as_deref(), Some("message_start"));
        assert_eq!(evs[0].json().unwrap()["a"], 1);
        assert_eq!(
            &evs[0].raw[..],
            b"event: message_start\ndata: {\"a\":1}\n\n"
        );
        assert_eq!(evs[1].data, "[DONE]");
        assert!(evs[2].data.is_empty());
        assert!(p.finish().is_none());
    }

    #[test]
    fn unterminated_tail_is_flushed() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: {\"x\":2}").is_empty());
        let ev = p.finish().unwrap();
        assert_eq!(ev.json().unwrap()["x"], 2);
    }
}
