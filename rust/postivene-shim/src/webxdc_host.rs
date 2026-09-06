//! The loopback host one webxdc app is served from while it is open.
//!
//! A webxdc app is a zip file, and the `WebView` needs its files as a
//! website: an index.html that can pull in its own scripts, styles and
//! pictures. Nothing here opens the zip. Every file comes from the core,
//! which reads the archive itself (`get_webxdc_blob`) -- so the app is
//! served rather than unpacked, and no archive format is parsed in this
//! repository.
//!
//! Serving it over HTTP is also what gives the app a way back: the API in
//! `webxdc.js` sends an update with a POST and collects the others by
//! polling, so the bridge needs nothing of the `WebView` beyond the URL it
//! is pointed at. The alternative -- a Gecko frame script -- would put
//! the bridge on chrome-privileged ground this app cannot test.
//!
//! What keeps that from being a hole in the phone:
//!
//! - the listener is bound to 127.0.0.1 on a port the kernel picks, so
//!   nothing off the device can reach it at all;
//! - every path starts with an unguessable token, and a request whose
//!   `Host:` is not the address the app was given is refused, so a page
//!   in the browser cannot walk the ports and find it;
//! - it exists only while the app is open, and answers nothing once the
//!   page is closed;
//! - the app itself is confined by a content-security-policy that permits
//!   this origin and nothing else, so a webxdc cannot reach the network
//!   even though it is on one.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// The API the app sees, as JavaScript. Compiled in rather than installed
/// beside the QML: it is served, not loaded by the engine, and a file the
/// package could lose is a bridge that fails on a phone and nowhere else.
const BRIDGE: &str = include_str!("webxdc.js");

/// What an app may do from inside the `WebView`. `'self'` is this host,
/// which is where its own files and the two API paths are; nothing else
/// is reachable, so an app cannot phone home whatever it asks for.
/// `unsafe-inline` and `unsafe-eval` are for the app's own code -- webxdc
/// apps are single files full of inline script, and the isolation here
/// comes from the origin having nowhere to go rather than from what the
/// app is allowed to run.
const POLICY: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; \
     style-src 'self' 'unsafe-inline' blob:; \
     img-src 'self' data: blob:; \
     media-src 'self' data: blob:; \
     font-src 'self' data: blob:; \
     connect-src 'self' data: blob:; \
     form-action 'none'; \
     frame-ancestors 'none'";

/// Milliseconds the bridge waits between asking for updates.
const POLL_INTERVAL: u32 = 300;

/// The most a request head may be. A webxdc's own requests are short;
/// anything longer is not one of them.
const MAX_HEAD: usize = 16 * 1024;

/// The most a `POST`ed update may be. The core's own soft limit is 100 KiB
/// and it rejects what is over its own; this is the point at which the
/// body is not read at all.
const MAX_BODY: usize = 512 * 1024;

/// Which app is being served, and what it is told about itself.
pub(crate) struct Instance {
    /// The account the message belongs to.
    pub(crate) account_id: u32,
    /// The message the webxdc arrived as, which is also the instance the
    /// core keeps status updates for.
    pub(crate) message_id: u32,
    /// `window.webxdc.selfAddr`: what other members of the chat see this
    /// app's updates as coming from. The core's own answer, not the
    /// account address.
    pub(crate) self_addr: String,
    /// `window.webxdc.selfName`: the profile's display name.
    pub(crate) self_name: String,
    /// `window.webxdc.sendUpdateInterval`, in milliseconds.
    pub(crate) send_update_interval: u64,
    /// `window.webxdc.sendUpdateMaxSize`, in bytes.
    pub(crate) send_update_max_size: u64,
}

/// Everything a connection needs, shared by every one of them.
struct Shared {
    rpc: Arc<RpcClient>,
    instance: Instance,
    /// `/<token>`, which every path this host answers begins with.
    prefix: String,
    /// The `Host:` a request must carry: the address the app was given.
    authority: String,
    /// Cleared when the app is closed. A connection accepted just before
    /// that stops rather than reaching the core.
    running: Arc<AtomicBool>,
    /// How many requests have been answered. Counted for the page, which
    /// says so while an app is coming up: a view that stays empty is a
    /// different fault depending on whether the engine ever asked for
    /// anything, and on a phone that is the one thing nothing else can
    /// tell us.
    served: Arc<AtomicU32>,
}

/// A served app: the URL to point a `WebView` at, and the task serving it.
///
/// Dropping it stops the host, which is what closing the page does.
pub(crate) struct Host {
    /// Where the app is. What `WebxdcApp.url` hands to QML.
    url: String,
    running: Arc<AtomicBool>,
    served: Arc<AtomicU32>,
    task: JoinHandle<()>,
}

impl Host {
    /// Where the app is being served.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// How many requests this host has answered, refusals included.
    pub(crate) fn served(&self) -> u32 {
        self.served.load(Ordering::SeqCst)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Both: the flag stops a connection already accepted, and the
        // abort closes the listener so no more arrive.
        self.running.store(false, Ordering::SeqCst);
        self.task.abort();
    }
}

/// Serve `instance` on the loopback interface until the [`Host`] is
/// dropped.
///
/// # Errors
///
/// Fails when the loopback port cannot be bound.
pub(crate) async fn start(rpc: Arc<RpcClient>, instance: Instance) -> Result<Host, String> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .map_err(|err| format!("cannot serve the app: {err}"))?;
    let address = listener
        .local_addr()
        .map_err(|err| format!("cannot serve the app: {err}"))?;
    let token = token();
    let authority = format!("127.0.0.1:{}", address.port());
    let url = format!("http://{authority}/{token}/index.html");
    let running = Arc::new(AtomicBool::new(true));
    let served = Arc::new(AtomicU32::new(0));
    let shared = Arc::new(Shared {
        rpc,
        instance,
        prefix: format!("/{token}"),
        authority,
        running: Arc::clone(&running),
        served: Arc::clone(&served),
    });
    let task = tokio::spawn(accept(listener, shared));
    Ok(Host {
        url,
        running,
        served,
        task,
    })
}

/// One connection at a time is not enough: a page loads its script, its
/// style and its pictures at once, and a blob the core is still reading
/// would hold up the rest.
async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while shared.running.load(Ordering::SeqCst) {
        // A listener that cannot accept is not going to start: this is
        // the socket being gone, not one bad connection.
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let _ = answer(stream, shared).await;
        });
    }
}

/// Read one request, answer it, and close. No keep-alive: every answer
/// says so, and a page load is a handful of short connections.
async fn answer(mut stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let response = match read_request(&mut stream).await? {
        Some(request) => route(&shared, &request).await,
        None => Response::empty("400 Bad Request"),
    };
    // Counted before the write rather than after it: what the page is
    // being told is that the engine got this far, and a write that fails
    // on a connection the engine dropped is still an answer that was
    // asked for.
    shared.served.fetch_add(1, Ordering::SeqCst);
    write_response(&mut stream, &response).await
}

/// One request, as much of it as anything here reads.
struct Request {
    method: String,
    target: String,
    host: String,
    body: Vec<u8>,
}

/// What goes back.
struct Response {
    status: &'static str,
    content_type: String,
    body: Vec<u8>,
}

impl Response {
    fn new(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: "200 OK",
            content_type: content_type.to_string(),
            body,
        }
    }

    /// A refusal, carrying its own reason. An answer with nothing in it
    /// draws as a blank page, and a blank page in a `WebView` cannot be
    /// told apart from an app that came up and did nothing.
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: status.as_bytes().to_vec(),
        }
    }

    /// The app itself could not be read. This fills the view in its
    /// place, so a reader is told what happened rather than shown grey.
    fn failed(status: &'static str, reason: &str) -> Self {
        let body = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width\">\
             <title>{status}</title></head>\
             <body style=\"font-family:sans-serif;padding:1.5em;color:#808080\">\
             <p>This app could not be opened.</p><p>{}</p></body></html>",
            escape(reason)
        );
        Self {
            status,
            content_type: "text/html; charset=utf-8".to_string(),
            body: body.into_bytes(),
        }
    }
}

/// The core's own words, put in a page as text rather than as markup.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A request whose head fits, or `None` for one that does not parse.
async fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 2048];
    let body_at = loop {
        if let Some(at) = find(&head, b"\r\n\r\n") {
            break at + 4;
        }
        if head.len() > MAX_HEAD {
            return Ok(None);
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        head.extend_from_slice(&chunk[..read]);
    };

    let mut body = head.split_off(body_at);
    let Ok(text) = std::str::from_utf8(&head) else {
        return Ok(None);
    };
    let mut lines = text.lines();
    let Some(start) = lines.next() else {
        return Ok(None);
    };
    let mut parts = start.split(' ');
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };

    let mut host = String::new();
    let mut length = 0_usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host = value.to_string();
        } else if name.eq_ignore_ascii_case("content-length") {
            length = value.parse().unwrap_or(0);
        }
    }
    if length > MAX_BODY {
        return Ok(None);
    }
    while body.len() < length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);

    Ok(Some(Request {
        method: method.to_string(),
        target: target.to_string(),
        host,
        body,
    }))
}

/// Where the needle starts in the haystack.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Pick the answer. Everything this host does not itself answer is a file
/// inside the archive, which the core reads.
async fn route(shared: &Shared, request: &Request) -> Response {
    // A page in the browser can reach 127.0.0.1 too, and a name that
    // resolves there would arrive with its own Host. Only the address the
    // app was handed is served.
    if !shared.running.load(Ordering::SeqCst) || request.host != shared.authority {
        return Response::empty("404 Not Found");
    }
    let (path, query) = match request.target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (request.target.as_str(), ""),
    };
    let Some(rest) = path.strip_prefix(&shared.prefix) else {
        return Response::empty("404 Not Found");
    };
    // The two API paths and the bridge are answered before anything is
    // looked for in the archive, so an app that ships a `webxdc.js` of
    // its own -- some do, to run outside a messenger -- gets this one.
    match (request.method.as_str(), rest) {
        ("GET", "" | "/" | "/index.html") => index(shared).await,
        ("GET", "/webxdc.js") => Response::new(
            "text/javascript; charset=utf-8",
            bridge(shared).into_bytes(),
        ),
        ("GET", "/webxdc-api/updates") => updates(shared, query).await,
        ("POST", "/webxdc-api/send") => send(shared, &request.body).await,
        ("GET", inner) => blob(shared, inner).await,
        _ => Response::empty("404 Not Found"),
    }
}

/// The app's own front page, with the API put in front of it.
async fn index(shared: &Shared) -> Response {
    match file(shared, "index.html").await {
        Ok(bytes) => {
            let html = String::from_utf8_lossy(&bytes);
            Response::new("text/html; charset=utf-8", inject(&html).into_bytes())
        }
        // The one refusal worth spelling out: without index.html there
        // is no app, and the core's reason is the only thing anybody can
        // act on. A blank page here reads as a broken app.
        Err(reason) => Response::failed("502 Bad Gateway", &reason),
    }
}

/// One file out of the archive, named as the app asked for it.
async fn blob(shared: &Shared, path: &str) -> Response {
    let Some(name) = archive_path(path) else {
        return Response::empty("404 Not Found");
    };
    match file(shared, &name).await {
        Ok(bytes) => Response::new(content_type(&name), bytes),
        Err(_) => Response::empty("404 Not Found"),
    }
}

/// Everything the chat has said about this app since `serial`, as the
/// core hands it over: a JSON array, passed through unread.
async fn updates(shared: &Shared, query: &str) -> Response {
    let serial = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("serial="))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let answer: Result<String, _> = shared
        .rpc
        .call(
            "get_webxdc_status_updates",
            (
                shared.instance.account_id,
                shared.instance.message_id,
                serial,
            ),
        )
        .await;
    match answer {
        Ok(json) => Response::new("application/json; charset=utf-8", json.into_bytes()),
        Err(_) => Response::empty("502 Bad Gateway"),
    }
}

/// One update from the app, on its way to everyone else in the chat.
async fn send(shared: &Shared, body: &[u8]) -> Response {
    let Ok(update) = std::str::from_utf8(body) else {
        return Response::empty("400 Bad Request");
    };
    // Read here rather than passed on as text: an update the core cannot
    // parse is the app's mistake, and it should hear about it from the
    // host it is talking to rather than through a core error nothing in
    // the page can see.
    if serde_json::from_str::<serde_json::Value>(update).is_err() {
        return Response::empty("400 Bad Request");
    }
    let answer: Result<serde_json::Value, _> = shared
        .rpc
        .call(
            "send_webxdc_status_update",
            (
                shared.instance.account_id,
                shared.instance.message_id,
                update,
                Option::<String>::None,
            ),
        )
        .await;
    match answer {
        Ok(_) => Response::empty("204 No Content"),
        Err(_) => Response::empty("502 Bad Gateway"),
    }
}

/// One file from the archive, through the core. The error is the core's
/// own words: the message is gone, the archive holds no such name, or the
/// attachment was never downloaded.
async fn file(shared: &Shared, name: &str) -> Result<Vec<u8>, String> {
    let answer: Result<String, _> = shared
        .rpc
        .call(
            "get_webxdc_blob",
            (shared.instance.account_id, shared.instance.message_id, name),
        )
        .await;
    let encoded = answer.map_err(|err| err.to_string())?;
    decode_base64(&encoded).ok_or_else(|| format!("{name} came back as something unreadable"))
}

/// The bridge with this instance's own values in it.
fn bridge(shared: &Shared) -> String {
    fill(BRIDGE, shared)
}

/// The placeholders in `webxdc.js`, as JSON literals: a name with a quote
/// in it is a string either way rather than the end of one.
fn fill(script: &str, shared: &Shared) -> String {
    let text = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    script
        .replace("__BASE__", &text(&shared.prefix))
        .replace("__SELF_ADDR__", &text(&shared.instance.self_addr))
        .replace("__SELF_NAME__", &text(&shared.instance.self_name))
        .replace(
            "__INTERVAL__",
            &shared.instance.send_update_interval.to_string(),
        )
        .replace(
            "__MAX_SIZE__",
            &shared.instance.send_update_max_size.to_string(),
        )
        .replace("__POLL__", &POLL_INTERVAL.to_string())
}

/// The app's index.html with the API loaded before any of its own code.
///
/// Inside `<head>` where there is one, since that is where a document's
/// scripts are expected and where anything the app runs on load comes
/// after it. A document without a head gets it at the front, which is
/// still before every script in the body.
fn inject(html: &str) -> String {
    const TAG: &str = "<script src=\"webxdc.js\"></script>";
    let lower = html.to_lowercase();
    let at = lower
        .find("<head")
        .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
        .unwrap_or(0);
    let mut out = String::with_capacity(html.len() + TAG.len());
    out.push_str(&html[..at]);
    out.push_str(TAG);
    out.push_str(&html[at..]);
    out
}

/// The name of a file inside the archive, from the path a request asked
/// for: percent-decoded, without its leading slash, and without a way
/// out of the archive.
fn archive_path(path: &str) -> Option<String> {
    let path = percent_decode(path.strip_prefix('/').unwrap_or(path));
    if path.is_empty() || path.starts_with('/') {
        return None;
    }
    // A zip may name an entry anything at all, and the core looks up what
    // it is given: "../" is refused here rather than trusted to mean
    // nothing there.
    if path
        .split('/')
        .any(|segment| segment == ".." || segment == ".")
    {
        return None;
    }
    Some(path)
}

/// `%20` and the rest. A byte that is not valid UTF-8 once decoded
/// leaves the path as it was written, which then simply does not name
/// anything in the archive.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let decoded = if byte == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok());
            match hex {
                Some(value) => {
                    index += 3;
                    Some(value)
                }
                None => None,
            }
        } else {
            None
        };
        if let Some(value) = decoded {
            out.push(value);
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// What to call a file, by its extension. Only what a page is built from;
/// anything else is handed over as bytes, which is what a download is.
fn content_type(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "text/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "oga" | "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Standard base64, with or without its padding: what the core encodes a
/// blob as (`STANDARD_NO_PAD`). Whitespace is skipped; anything else that
/// is not in the alphabet is a blob this cannot read.
pub(crate) fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            // The byte that just completed, dropping the bits that are
            // still only part of the next one.
            let shifted = buffer >> bits;
            out.push(u8::try_from(shifted & 0xff).ok()?);
        }
    }
    Some(out)
}

/// The digits a token is written in.
const HEX: &[u8; 16] = b"0123456789abcdef";

/// The unguessable part of every path.
///
/// From `/dev/urandom`, which is there on every phone this runs on. The
/// fallback is not a secret worth relying on, and does not have to be:
/// what it guards is one app's files on a socket only this device can
/// reach, and the app is open for as long as somebody is looking at it.
fn token() -> String {
    let mut bytes = [0_u8; 16];
    let read = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok();
    if !read {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let mixed = u64::from(nanos) ^ (u64::from(std::process::id()) << 32);
        bytes[..8].copy_from_slice(&mixed.to_le_bytes());
    }
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Two hex digits, written rather than formatted: `format!` per
        // byte builds sixteen little strings to throw away.
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Write the answer out. Every one of them carries the policy: the app is
/// on an origin of its own, and this is what it may do from there.
async fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Content-Security-Policy: {POLICY}\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n",
        response.status,
        response.content_type,
        response.body.len(),
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    stream.flush().await
}

#[cfg(test)]
mod tests {
    use super::{
        archive_path, content_type, decode_base64, inject, percent_decode, token, Response,
    };

    #[test]
    fn base64_reads_with_or_without_padding_and_refuses_anything_else() {
        assert_eq!(decode_base64("aGk").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode_base64("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode_base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(decode_base64("").as_deref(), Some(&b""[..]));
        // The core wraps nothing, but a blob that arrived wrapped is
        // still the blob.
        assert_eq!(decode_base64("aGVs\r\nbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(decode_base64("not base64!"), None);
    }

    #[test]
    fn base64_round_trips_every_byte() {
        // Encoded by hand, so the decoder is checked against the
        // alphabet rather than against itself.
        let bytes: Vec<u8> = (0..=255).collect();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in bytes.chunks(3) {
            let mut block = [0_u8; 3];
            block[..chunk.len()].copy_from_slice(chunk);
            let packed =
                (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);
            for index in 0..=chunk.len() {
                let shift = 18 - 6 * index;
                encoded.push(alphabet[((packed >> shift) & 0x3f) as usize] as char);
            }
        }
        assert_eq!(decode_base64(&encoded).as_deref(), Some(&bytes[..]));
    }

    #[test]
    fn the_api_goes_inside_the_head_and_before_every_script() {
        let injected = inject("<html><head><title>x</title></head><body></body></html>");
        assert_eq!(
            injected,
            "<html><head><script src=\"webxdc.js\"></script><title>x</title></head>\
             <body></body></html>"
        );
        // A head that carries attributes is still a head.
        assert!(inject("<HEAD lang=\"en\">a")
            .starts_with("<HEAD lang=\"en\"><script src=\"webxdc.js\"></script>"));
        // And a document without one gets the API first of all.
        assert!(inject("<p>hi</p>").starts_with("<script src=\"webxdc.js\"></script>"));
    }

    #[test]
    fn a_path_cannot_leave_the_archive() {
        assert_eq!(archive_path("/index.html").as_deref(), Some("index.html"));
        assert_eq!(
            archive_path("/assets/app%20one.js").as_deref(),
            Some("assets/app one.js")
        );
        assert_eq!(archive_path("/../../etc/passwd"), None);
        assert_eq!(archive_path("/assets/../../secret"), None);
        assert_eq!(archive_path("/./index.html"), None);
        assert_eq!(archive_path("/"), None);
    }

    #[test]
    fn percent_decoding_leaves_what_is_not_an_escape_alone() {
        assert_eq!(percent_decode("a%2Fb"), "a/b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
    }

    #[test]
    fn a_page_is_named_by_its_extension_and_anything_else_is_bytes() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("app.JS"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("icon.png"), "image/png");
        assert_eq!(content_type("data"), "application/octet-stream");
        assert_eq!(content_type("archive.tar.gz"), "application/octet-stream");
    }

    #[test]
    fn a_refusal_says_what_happened_rather_than_drawing_blank() {
        // Every refusal carries its reason: a body with nothing in it is
        // a blank page, and that is what a broken app looks like too.
        let refused = Response::empty("404 Not Found");
        assert_eq!(refused.body, b"404 Not Found");

        // The app's own page is the one worth a sentence, and the core's
        // words go in it as text.
        let failed = Response::failed("502 Bad Gateway", "no <b>file</b> & no message");
        let page = String::from_utf8_lossy(&failed.body);
        assert!(
            page.contains("This app could not be opened."),
            "the failure page does not say what went wrong: {page}"
        );
        assert!(
            page.contains("no &lt;b&gt;file&lt;/b&gt; &amp; no message"),
            "the core's words were not escaped into the page: {page}"
        );
        assert_eq!(failed.content_type, "text/html; charset=utf-8");
    }

    #[test]
    fn two_tokens_are_not_the_same_token() {
        let (one, two) = (token(), token());
        assert_eq!(one.len(), 32);
        assert_ne!(one, two);
    }
}
