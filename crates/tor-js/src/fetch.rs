//! HTTP fetch implementation over arti-client DataStream
//!
//! This module implements HTTP/1.1 requests over Tor streams,
//! with TLS support via subtle-tls for HTTPS.
//!
//! The fetch resolves as soon as response headers are received.
//! Body reading is deferred to async methods on the response object.

use crate::error::JsTorError;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use http::Method;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use url::Url;

/// Maximum response header size (64KB)
const MAX_HEADER_SIZE: usize = 64 * 1024;

/// Maximum response body size (1MB)
const MAX_BODY_SIZE: usize = 64 * 1024 * 1024;

/// Type-erased async reader for the response body stream.
pub type BoxedReader = Box<dyn futures::io::AsyncRead + Unpin>;

/// How the response body is framed.
pub enum BodyFraming {
    /// Content-Length header present: read exactly N bytes.
    ContentLength(usize),
    /// Transfer-Encoding: chunked.
    Chunked,
    /// No framing info: read until EOF (Connection: close).
    UntilEof,
    /// No body expected (HEAD response, 204, 304, 1xx).
    None,
}

/// Result of the header phase of a fetch request.
pub struct FetchHeadersResult {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub url: Url,
    pub body_reader: BodyReader,
}

/// Reads the HTTP response body from a stream.
///
/// Created after headers are parsed, holds the stream and any overflow
/// bytes that were read past the header separator.
pub struct BodyReader {
    stream: BoxedReader,
    framing: BodyFraming,
    /// Bytes already read past \r\n\r\n during header parsing.
    buffer: Vec<u8>,
    done: bool,
}

impl BodyReader {
    pub fn new(stream: BoxedReader, framing: BodyFraming, overflow: Vec<u8>) -> Self {
        Self {
            stream,
            framing,
            buffer: overflow,
            done: false,
        }
    }

    /// Read the entire remaining body. Enforces the 1MB size limit.
    pub async fn read_all(&mut self) -> Result<Vec<u8>, JsTorError> {
        if self.done {
            return Ok(Vec::new());
        }
        self.done = true;

        match &self.framing {
            BodyFraming::None => Ok(Vec::new()),
            BodyFraming::ContentLength(len) => self.read_content_length(*len).await,
            BodyFraming::Chunked => self.read_chunked().await,
            BodyFraming::UntilEof => self.read_until_eof().await,
        }
    }

    /// Read exactly `len` bytes of body (Content-Length framing).
    async fn read_content_length(&mut self, len: usize) -> Result<Vec<u8>, JsTorError> {
        if len > MAX_BODY_SIZE {
            return Err(JsTorError::http_request(format!(
                "Content-Length {} exceeds {}MB limit",
                len,
                MAX_BODY_SIZE / (1024 * 1024)
            )));
        }

        let mut body = std::mem::take(&mut self.buffer);

        // Already have enough from overflow?
        if body.len() >= len {
            body.truncate(len);
            return Ok(body);
        }

        body.reserve(len - body.len());
        let mut buf = [0u8; 8192];

        while body.len() < len {
            match self.stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let take = std::cmp::min(n, len - body.len());
                    body.extend_from_slice(&buf[..take]);
                }
                Err(e) => {
                    if body.is_empty() {
                        return Err(JsTorError::http_request(format!(
                            "Failed to read body: {}",
                            e
                        )));
                    }
                    debug!("Read ended with error (may be normal close): {}", e);
                    break;
                }
            }
        }

        Ok(body)
    }

    /// Read body until EOF (no Content-Length or chunked framing).
    async fn read_until_eof(&mut self) -> Result<Vec<u8>, JsTorError> {
        let mut body = std::mem::take(&mut self.buffer);
        let mut buf = [0u8; 8192];

        loop {
            match self.stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    body.extend_from_slice(&buf[..n]);
                    if body.len() > MAX_BODY_SIZE {
                        warn!("Response exceeds {}MB limit, truncating", MAX_BODY_SIZE / (1024 * 1024));
                        body.truncate(MAX_BODY_SIZE);
                        break;
                    }
                }
                Err(e) => {
                    if body.is_empty() {
                        return Err(JsTorError::http_request(format!(
                            "Failed to read body: {}",
                            e
                        )));
                    }
                    debug!("Read ended with error (may be normal close): {}", e);
                    break;
                }
            }
        }

        Ok(body)
    }

    /// Read chunked transfer-encoded body incrementally from the stream.
    async fn read_chunked(&mut self) -> Result<Vec<u8>, JsTorError> {
        let mut input = std::mem::take(&mut self.buffer);
        let mut output = Vec::new();
        let mut buf = [0u8; 8192];

        // State machine for chunked decoding
        enum State {
            ReadingSize,
            ReadingData { remaining: usize },
            ReadingTrailingCrlf,
        }

        let mut state = State::ReadingSize;

        loop {
            match state {
                State::ReadingSize => {
                    // Look for \r\n to get chunk size line
                    if let Some(pos) = find_crlf(&input) {
                        let size_str = std::str::from_utf8(&input[..pos])
                            .map_err(|_| {
                                JsTorError::http_request("Chunk size line is not valid UTF-8")
                            })?;
                        let size_str = size_str.split(';').next().unwrap_or("").trim();

                        if size_str.is_empty() {
                            // Skip empty lines
                            input.drain(..pos + 2);
                            continue;
                        }

                        let size = usize::from_str_radix(size_str, 16).map_err(|e| {
                            JsTorError::http_request(format!(
                                "Invalid chunk size '{}': {}",
                                size_str, e
                            ))
                        })?;

                        input.drain(..pos + 2);

                        if size == 0 {
                            break; // Terminal chunk
                        }

                        state = State::ReadingData { remaining: size };
                    } else {
                        // Need more data
                        let n = self.read_more(&mut buf).await?;
                        if n == 0 {
                            break; // EOF
                        }
                        input.extend_from_slice(&buf[..n]);
                    }
                }
                State::ReadingData { remaining } => {
                    if !input.is_empty() {
                        let take = std::cmp::min(input.len(), remaining);
                        output.extend_from_slice(&input[..take]);
                        input.drain(..take);
                        let new_remaining = remaining - take;
                        if new_remaining == 0 {
                            state = State::ReadingTrailingCrlf;
                        } else {
                            state = State::ReadingData {
                                remaining: new_remaining,
                            };
                        }
                    } else {
                        let n = self.read_more(&mut buf).await?;
                        if n == 0 {
                            break; // Unexpected EOF
                        }
                        input.extend_from_slice(&buf[..n]);
                    }

                    if output.len() > MAX_BODY_SIZE {
                        warn!("Chunked response exceeds {}MB limit", MAX_BODY_SIZE / (1024 * 1024));
                        output.truncate(MAX_BODY_SIZE);
                        break;
                    }
                }
                State::ReadingTrailingCrlf => {
                    if input.len() >= 2 {
                        input.drain(..2);
                        state = State::ReadingSize;
                    } else {
                        let n = self.read_more(&mut buf).await?;
                        if n == 0 {
                            break; // EOF
                        }
                        input.extend_from_slice(&buf[..n]);
                    }
                }
            }
        }

        Ok(output)
    }

    /// Read more bytes from the stream into the buffer.
    async fn read_more(&mut self, buf: &mut [u8]) -> Result<usize, JsTorError> {
        self.stream.read(buf).await.map_err(|e| {
            JsTorError::http_request(format!("Failed to read response: {}", e))
        })
    }
}

/// Build an HTTP/1.1 request as raw bytes
pub fn build_http_request(
    url: &Url,
    method: &Method,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> Vec<u8> {
    let host = url.host_str().unwrap_or("localhost");
    let path = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };

    let query = url.query().map(|q| format!("?{}", q)).unwrap_or_default();

    let mut request = format!(
        "{} {}{} HTTP/1.1\r\nHost: {}\r\n",
        method.as_str(),
        path,
        query,
        host
    );

    // Add default headers if not present
    if !headers.contains_key("User-Agent") && !headers.contains_key("user-agent") {
        request.push_str("User-Agent: tor-js/0.1.0\r\n");
    }
    if !headers.contains_key("Accept") && !headers.contains_key("accept") {
        request.push_str("Accept: */*\r\n");
    }
    if !headers.contains_key("Connection") && !headers.contains_key("connection") {
        request.push_str("Connection: close\r\n");
    }

    // Add custom headers
    for (key, value) in headers {
        request.push_str(&format!("{}: {}\r\n", key, value));
    }

    // Add content-length for requests with body
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }

    // End headers
    request.push_str("\r\n");

    let mut bytes = request.into_bytes();

    // Add body if present
    if let Some(body) = body {
        bytes.extend_from_slice(body);
    }

    bytes
}

/// Write the HTTP request and read response headers.
///
/// Returns the parsed status/headers, the body framing mode, and any overflow
/// bytes read past the `\r\n\r\n` header separator. The stream is borrowed
/// mutably so the caller retains ownership for body reading.
async fn send_request_and_read_headers<S>(
    stream: &mut S,
    request_bytes: &[u8],
    method: &Method,
) -> Result<(u16, HashMap<String, String>, BodyFraming, Vec<u8>), JsTorError>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
{
    // Write the request
    stream
        .write_all(request_bytes)
        .await
        .map_err(|e| JsTorError::http_request(format!("Failed to write request: {}", e)))?;
    stream
        .flush()
        .await
        .map_err(|e| JsTorError::http_request(format!("Failed to flush request: {}", e)))?;

    // Read until we find \r\n\r\n (header/body separator)
    let mut header_buf = Vec::new();
    let mut buf = [0u8; 8192];
    let header_end;

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => {
                return Err(JsTorError::http_request(
                    "Connection closed before headers received",
                ));
            }
            Ok(n) => {
                header_buf.extend_from_slice(&buf[..n]);

                if let Some(pos) = find_subsequence(&header_buf, b"\r\n\r\n") {
                    header_end = pos;
                    break;
                }

                if header_buf.len() > MAX_HEADER_SIZE {
                    return Err(JsTorError::http_request(format!(
                        "Response headers exceed {}KB limit",
                        MAX_HEADER_SIZE / 1024
                    )));
                }
            }
            Err(e) => {
                return Err(JsTorError::http_request(format!(
                    "Failed to read response headers: {}",
                    e
                )));
            }
        }
    }

    // Split headers from overflow body bytes
    let header_bytes = &header_buf[..header_end];
    let overflow = header_buf[header_end + 4..].to_vec();

    // Parse headers
    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|e| JsTorError::http_request(format!("Invalid HTTP headers: {}", e)))?;

    let mut lines = header_str.lines();

    // Parse status line: "HTTP/1.1 200 OK"
    let status_line = lines
        .next()
        .ok_or_else(|| JsTorError::http_request("Invalid HTTP response: no status line"))?;

    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(JsTorError::http_request("Invalid HTTP status line"));
    }

    let status: u16 = parts[1]
        .parse()
        .map_err(|e| JsTorError::http_request(format!("Invalid status code: {}", e)))?;

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    // Determine body framing
    let framing = if *method == Method::HEAD
        || status == 204
        || status == 304
        || (100..200).contains(&status)
    {
        BodyFraming::None
    } else if headers
        .get("transfer-encoding")
        .map(|te| te.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false)
    {
        debug!("Body framing: chunked");
        BodyFraming::Chunked
    } else if let Some(cl) = headers.get("content-length") {
        let len: usize = cl
            .parse()
            .map_err(|e| JsTorError::http_request(format!("Invalid content-length: {}", e)))?;
        debug!("Body framing: content-length {}", len);
        BodyFraming::ContentLength(len)
    } else {
        debug!("Body framing: read until EOF");
        BodyFraming::UntilEof
    };

    debug!(
        "Parsed response headers: status={}, headers={}, overflow_bytes={}",
        status,
        headers.len(),
        overflow.len()
    );

    Ok((status, headers, framing, overflow))
}

/// Perform an HTTP fetch over a Tor stream, resolving as soon as headers arrive.
///
/// The returned `FetchHeadersResult` contains parsed headers and a `BodyReader`
/// that can be used to read the body asynchronously.
pub async fn fetch_headers<S>(
    stream: S,
    url: &Url,
    method: Method,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
    is_https: bool,
    host: &str,
    ca_bundle_wait: Option<std::rc::Rc<subtle_tls::ReadySignal>>,
) -> Result<FetchHeadersResult, JsTorError>
where
    S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + 'static,
{
    let request_bytes = build_http_request(url, &method, &headers, body.as_deref());
    debug!("Sending {} bytes of HTTP request", request_bytes.len());

    if is_https {
        use subtle_tls::{TlsConfig, TlsStream};

        let config = TlsConfig {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
            ..Default::default()
        };

        let mut tls_stream = TlsStream::connect(stream, host, config, ca_bundle_wait)
            .await
            .map_err(|e| {
                JsTorError::tls(format!("TLS handshake failed with {}: {}", host, e))
            })?;
        info!(
            "TLS 1.3 connection established with {} (WASM/SubtleCrypto)",
            host
        );

        let (status, resp_headers, framing, overflow) =
            send_request_and_read_headers(&mut tls_stream, &request_bytes, &method).await?;

        info!("Received response headers: status={}", status);

        let reader: BoxedReader = Box::new(tls_stream);
        Ok(FetchHeadersResult {
            status,
            headers: resp_headers,
            url: url.clone(),
            body_reader: BodyReader::new(reader, framing, overflow),
        })
    } else {
        let mut stream = stream;

        let (status, resp_headers, framing, overflow) =
            send_request_and_read_headers(&mut stream, &request_bytes, &method).await?;

        info!("Received response headers: status={}", status);

        let reader: BoxedReader = Box::new(stream);
        Ok(FetchHeadersResult {
            status,
            headers: resp_headers,
            url: url.clone(),
            body_reader: BodyReader::new(reader, framing, overflow),
        })
    }
}

/// Find the position of a subsequence in a byte slice
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Find the position of \r\n in a byte slice
fn find_crlf(data: &[u8]) -> Option<usize> {
    find_subsequence(data, b"\r\n")
}
