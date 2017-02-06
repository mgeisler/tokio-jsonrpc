// Copyright (c) 2017 Michal 'vorner' Vaner <vorner@vorner.cz>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! The codecs to encode and decode messages from a stream of bytes.
//!
//! You can choose to use either line separated one ([Line](struct.Line.html)) or
//! boundary separated one ([Boundary](struct.Boundary.html)). The first one needs the
//! messages to be separated by newlines and not to contain newlines in their representation. On
//! the other hand, it can recover from syntax error in a message and respond with an error instead
//! of terminating the connection.

// TODO: Have both line-separated and object separated codecs. The first can detect syntax errors,
// while the other can decode multiline messages or messages on single line.

use std::io::{Result as IoResult, Error, ErrorKind};
use std::error::Error as ErrorTrait;

use tokio_core::io::{Codec, EasyBuf};
use serde_json::de::from_slice;
use serde_json::ser::to_vec;
use serde_json::error::Error as SerdeError;

use message::Message;

/// A helper to wrap the error
fn err_map(e: SerdeError) -> Error {
    Error::new(ErrorKind::Other, e)
}

/// A codec working with JSONRPC 2.0 messages.
///
/// This produces or encodes [Message](../message/enum.Message.hmtl). It separates the records by
/// newlines, so it can recover from syntax error.s
pub struct Line;

impl Codec for Line {
    type In = Message;
    type Out = Message;
    fn decode(&mut self, buf: &mut EasyBuf) -> IoResult<Option<Message>> {
        if let Some(i) = buf.as_slice().iter().position(|&b| b == b'\n') {
            let line = buf.drain_to(i);
            buf.drain_to(1);
            match from_slice(line.as_slice()) {
                Ok(message) => Ok(Some(message)),
                // A hack to recognize syntax errors, before https://github.com/serde-rs/json/issues/245
                // is done.
                Err(ref e) if e.cause().is_none() => Ok(Some(Message::SyntaxError)),
                Err(e) => Err(err_map(e)),
            }
        } else {
            Ok(None)
        }
    }
    fn encode(&mut self, msg: Message, buf: &mut Vec<u8>) -> IoResult<()> {
        *buf = to_vec(&msg).map_err(err_map)?;
        buf.push(b'\n');
        Ok(())
    }
}

/// A codec working with JSONRPC 2.0 messages.
///
/// This produces or encodes [Message](../message/enum.Message.hmtl). It takes the JSON object boundaries,
/// so it works with both newline-separated and object-separated encoding. It produces
/// newline-separated stream, which is more generic.
///
/// TODO: This is not implemented yet.
pub struct Boundary;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode() {
        let mut output = Vec::new();
        let mut codec = Line;
        codec.encode(Message::notification("notif".to_owned(), None), &mut output).unwrap();
        assert_eq!(Vec::from(&b"{\"jsonrpc\":\"2.0\",\"method\":\"notif\"}\n"[..]), output);
    }

    #[test]
    fn decode() {
        fn one(input: &[u8], rest: &[u8]) -> IoResult<Option<Message>> {
            let mut codec = Line;
            let mut buf = EasyBuf::new();
            buf.get_mut().extend_from_slice(input);
            let result = codec.decode(&mut buf);
            assert_eq!(rest, buf.as_slice());
            result
        }

        // TODO: We currently have to terminate the records by newline, but that's a temporary
        // problem. Once that is solved, have some tests without the newline as well. Also, test
        // some messages that don't have a newline, but have a syntax error, so we know we abort
        // soon enough.

        let notif = Message::notification("notif".to_owned(), None);
        let msgstring = Vec::from(&b"{\"jsonrpc\":\"2.0\",\"method\":\"notif\"}\n"[..]);
        // A single message, nothing is left
        assert_eq!(one(&msgstring, b"").unwrap(), Some(notif.clone()));
        // The first message is decoded, the second stays in the buffer
        let mut twomsgs = msgstring.clone();
        twomsgs.extend_from_slice(&msgstring);
        assert_eq!(one(&twomsgs, &msgstring).unwrap(), Some(notif.clone()));
        // The second message is incomplete, but stays there
        let incomplete = Vec::from(&br#"{"jsonrpc": "2.0", "method":""#[..]);
        let mut oneandhalf = msgstring.clone();
        oneandhalf.extend_from_slice(&incomplete);
        assert_eq!(one(&oneandhalf, &incomplete).unwrap(), Some(notif.clone()));
        // An incomplete message ‒ nothing gets out and everything stays
        assert_eq!(one(&incomplete, &incomplete).unwrap(), None);
        // A syntax error is reported as an error (and eaten, but that's no longer interesting)
        assert_eq!(one(b"{]\n", b"").unwrap(), Some(Message::SyntaxError));
    }
}
