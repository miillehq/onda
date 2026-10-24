//! Incremental UTF-8 SSE decoding with limits independent of network chunking.

use crate::{CloudError, Result};

const MAX_EVENT_BYTES: usize = 256 * 1024;

pub(crate) struct ServerEvent {
    pub event_type: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub(crate) struct SseDecoder {
    line: Vec<u8>,
    data: Vec<u8>,
    event_type: Option<String>,
    pending_carriage_return: bool,
    started: bool,
    frame_bytes: usize,
}

impl SseDecoder {
    /// Time O(n), space O(maximum event size), regardless of chunk boundaries.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<ServerEvent>> {
        let mut events = Vec::new();
        for &byte in bytes {
            if self.pending_carriage_return {
                self.pending_carriage_return = false;
                if byte == b'\n' {
                    continue;
                }
            }
            self.frame_bytes += 1;
            if self.frame_bytes > MAX_EVENT_BYTES {
                return Err(CloudError::SizeLimit);
            }
            match byte {
                b'\r' => {
                    self.complete_line(&mut events)?;
                    self.pending_carriage_return = true;
                }
                b'\n' => self.complete_line(&mut events)?,
                _ => self.line.push(byte),
            }
        }
        Ok(events)
    }

    #[cfg(test)]
    pub fn has_partial_event(&self) -> bool {
        !self.line.is_empty() || !self.data.is_empty()
    }

    fn complete_line(&mut self, events: &mut Vec<ServerEvent>) -> Result<()> {
        let bytes = std::mem::take(&mut self.line);
        let line = std::str::from_utf8(&bytes).map_err(|_| CloudError::MalformedResponse)?;
        let line = if !self.started {
            self.started = true;
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        if line.is_empty() {
            if !self.data.is_empty() {
                self.data.pop();
                let data = String::from_utf8(std::mem::take(&mut self.data))
                    .map_err(|_| CloudError::MalformedResponse)?;
                events.push(ServerEvent {
                    event_type: self.event_type.take(),
                    data,
                });
            }
            self.event_type = None;
            self.frame_bytes = 0;
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(());
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                self.data.extend_from_slice(value.as_bytes());
                self.data.push(b'\n');
            }
            "event" => self.event_type = Some(value.to_owned()),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_every_chunk_boundary_utf8_multiline_comments_and_crlf() {
        let stream =
            "\u{feff}: keepalive\r\nevent: message\r\ndata: café\r\ndata: 東京\r\n\r\n".as_bytes();
        for boundary in 0..=stream.len() {
            let mut decoder = SseDecoder::default();
            let mut events = decoder.push(&stream[..boundary]).unwrap();
            events.extend(decoder.push(&stream[boundary..]).unwrap());
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].data, "café\n東京");
            assert_eq!(events[0].event_type.as_deref(), Some("message"));
            assert!(!decoder.has_partial_event());
        }
    }

    #[test]
    fn rejects_oversized_event_and_malformed_utf8() {
        assert!(matches!(
            SseDecoder::default().push(&vec![b'x'; MAX_EVENT_BYTES + 1]),
            Err(CloudError::SizeLimit)
        ));
        assert!(matches!(
            SseDecoder::default().push(b"data: \xff\n\n"),
            Err(CloudError::MalformedResponse)
        ));
    }

    #[test]
    fn incomplete_frame_is_never_emitted() {
        let mut decoder = SseDecoder::default();
        assert!(decoder
            .push(b"data: {\"type\":\"response.completed\"}\n")
            .unwrap()
            .is_empty());
        assert!(decoder.has_partial_event());
    }
}
