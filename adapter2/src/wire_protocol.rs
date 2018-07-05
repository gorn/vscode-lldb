use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::fmt::Write;
use std::str;
use tokio::io;
use tokio_io::codec;

use debug_protocol::ProtocolMessage;
use serde_json::{self, Value};

enum State {
    ReadingHeaders,
    ReadingBody,
}

pub struct Codec {
    state: State,
    content_len: usize,
}

impl Codec {
    pub fn new() -> Codec {
        Codec {
            state: State::ReadingHeaders,
            content_len: 0,
        }
    }
}

impl codec::Decoder for Codec {
    type Item = ProtocolMessage;
    type Error = io::Error;

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<ProtocolMessage>, io::Error> {
        match self.state {
            State::ReadingHeaders => {
                if let Some(pos) = buffer.windows(2).position(|b| b == &[b'\r', b'\n']) {
                    let line = buffer.split_to(pos + 2);
                    if line.len() == 2 {
                        self.state = State::ReadingBody;
                    } else if let Ok(line) = str::from_utf8(&line) {
                        if line.len() > 15 && line[..15].eq_ignore_ascii_case("content-length:") {
                            if let Ok(content_len) = line[15..].trim().parse::<usize>() {
                                self.content_len = content_len;
                            }
                        }
                    }
                }
            }
            State::ReadingBody => {
                if (buffer.len() >= self.content_len) {
                    let message_bytes = buffer.split_to(self.content_len);
                    self.state = State::ReadingHeaders;
                    self.content_len = 0;

                    debug!("rx: {}", str::from_utf8(&message_bytes).unwrap());
                    match serde_json::from_slice(&message_bytes) {
                        Ok(message) => return Ok(Some(message)),
                        Err(err) => {
                            if (err.is_data()) {
                                // Try reading as generic JSON value.
                                if let Ok(message) = serde_json::from_slice::<Value>(&message_bytes) {
                                    return Ok(Some(ProtocolMessage::Unknown(message)));
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

impl codec::Encoder for Codec {
    type Item = ProtocolMessage;
    type Error = io::Error;

    fn encode(&mut self, message: ProtocolMessage, buffer: &mut BytesMut) -> Result<(), io::Error> {
        let message_bytes = serde_json::to_vec(&message).unwrap();
        debug!("tx: {}", str::from_utf8(&message_bytes).unwrap());
        write!(buffer, "Content-Length: {}\r\n\r\n", message_bytes.len());
        buffer.extend_from_slice(&message_bytes);
        Ok(())
    }
}
