//! The Claude Code hook listener.
//!
//! Claude Code's `http` hook type POSTs each event as JSON. Clawd accepts
//! those on loopback and hands the bytes to `clawd_core::hook::translate`,
//! which is where the entire understanding of Claude Code lives — this module
//! only moves bytes.
//!
//! An `http` hook needs no script, which is the whole reason for it: a
//! `command` hook would need one that works under both PowerShell and bash,
//! and PowerShell startup is 150-400ms paid twice per tool call.
//!
//! **Nothing in a worker thread may panic.** The release profile sets
//! `panic = "abort"`, so a panic here does not lose a request — it kills the
//! pet. There is no `unwrap` or `expect` below this line.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use clawd_core::source::{EventSink, EventSource};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Request, Response, Server};

/// The port the installed hook config points at.
pub const DEFAULT_PORT: u16 = 8787;

/// How many neighbouring ports to try when the default is taken.
const PORT_SCAN: u16 = 4;

/// Largest body accepted.
///
/// A `Write` tool call carries the whole file it is writing, so multi-megabyte
/// payloads are ordinary traffic here rather than an attack. The cap exists so
/// a runaway body cannot be read into memory without bound — it is a ceiling,
/// not a budget, and is set high enough that no real edit reaches it. A body
/// that does reach it is dropped as oversized, never counted as malformed:
/// truncating JSON at a byte boundary and then blaming the sender for bad
/// syntax would turn a size limit into a lie.
const MAX_BODY: u64 = 16 * 1024 * 1024;

/// Workers sharing the listener. More than one because a single handler can be
/// stalled by one slow client mid-body, and parked threads cost nothing.
const WORKERS: usize = 4;

/// Where the shell records the port it actually bound, so the installer can
/// point the hook config at the right one.
#[derive(Debug, Serialize, Deserialize)]
struct PortFile {
    port: u16,
}

pub struct HookSource {
    requested_port: u16,
    server: Option<Arc<Server>>,
    workers: Vec<JoinHandle<()>>,
    /// Cleared by `stop`, so a worker that is already mid-request when the
    /// listener is unblocked does not emit into a channel that is going away.
    running: Arc<AtomicBool>,
    /// Bodies that were not hook payloads. Only ever surfaced under devtools.
    malformed: Arc<AtomicU64>,
    /// Held even when the bind failed and there are no workers to hold it.
    ///
    /// The last `EventSink` closing the channel ends the pump thread, and the
    /// machine would then never advance again for the life of the process —
    /// so a Clawd that could not open its socket would also stop keeping time.
    _sink: Option<EventSink>,
}

impl HookSource {
    pub fn new(requested_port: u16) -> Self {
        Self {
            requested_port,
            server: None,
            workers: Vec::new(),
            running: Arc::new(AtomicBool::new(true)),
            malformed: Arc::new(AtomicU64::new(0)),
            _sink: None,
        }
    }

    /// The port actually bound, or `None` when no listener is running.
    pub fn port(&self) -> Option<u16> {
        self.server
            .as_ref()
            .and_then(|s| s.server_addr().to_ip())
            .map(|addr| addr.port())
    }
}

impl EventSource for HookSource {
    fn start(&mut self, sink: EventSink) {
        // Keep the sink whatever happens below: dropping the last one closes
        // the channel and ends the pump.
        self._sink = Some(sink.clone());

        let Some(server) = bind(self.requested_port) else {
            // Clawd runs perfectly well with no listener; it simply never
            // leaves Idle. A second instance must not crash the first, and a
            // taken port is not worth a dialog.
            log(&format!(
                "no hook listener: ports {}-{} are all in use",
                self.requested_port,
                self.requested_port.saturating_add(PORT_SCAN)
            ));
            return;
        };

        let server = Arc::new(server);
        let bound = server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .unwrap_or_default();
        log(&format!(
            "listening for Claude Code hooks on http://127.0.0.1:{bound}/hook"
        ));
        if bound != self.requested_port {
            // Worth saying loudly. Whatever already owns the requested port is
            // now receiving Claude Code's hook payloads — which carry prompts
            // and tool inputs — until the config is pointed here instead.
            log(&format!(
                "WARNING: port {} was taken, so hooks configured for it are                  going to whatever owns it. Re-run `npm run hooks:install`.",
                self.requested_port
            ));
        }

        for _ in 0..WORKERS {
            let server = server.clone();
            let sink = sink.clone();
            let running = self.running.clone();
            let malformed = self.malformed.clone();
            if let Ok(handle) = thread::Builder::new()
                .name("clawd-hook".into())
                .spawn(move || serve(&server, &sink, &running, &malformed))
            {
                self.workers.push(handle);
            }
        }

        self.server = Some(server);
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(server) = &self.server {
            // Wakes every worker parked in `recv`.
            server.unblock();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        self.server = None;
    }
}

/// Bind loopback, scanning a few ports if the first is taken.
///
/// Loopback only: never `0.0.0.0`. That is what keeps this off the LAN and
/// keeps it from raising a firewall prompt on either platform.
fn bind(port: u16) -> Option<Server> {
    (port..=port.saturating_add(PORT_SCAN)).find_map(|candidate| {
        Server::http(SocketAddrV4::new(Ipv4Addr::LOCALHOST, candidate)).ok()
    })
}

/// How long a worker parks before re-checking whether it should still be
/// running. `Server::unblock` wakes a waiter, but not dependably all four of
/// them, so shutdown is driven by the flag and `unblock` only hurries it.
const PARK: std::time::Duration = std::time::Duration::from_millis(250);

fn serve(server: &Server, sink: &EventSink, running: &AtomicBool, malformed: &AtomicU64) {
    while running.load(Ordering::Relaxed) {
        match server.recv_timeout(PARK) {
            Ok(Some(request)) => handle(request, sink, running, malformed),
            // Parked out; loop round and re-read the flag.
            Ok(None) => {}
            // Unblocked, or the listener hit an accept error it cannot
            // recover from (descriptor exhaustion, say). Say so: a worker
            // that retires quietly leaves Clawd looking healthy while it
            // observes less and less.
            Err(error) => {
                if running.load(Ordering::Relaxed) {
                    log(&format!("hook worker stopped: {error}"));
                }
                return;
            }
        }
    }
}

fn handle(mut request: Request, sink: &EventSink, running: &AtomicBool, malformed: &AtomicU64) {
    if !is_hook_post(&request) {
        let _ = request.respond(Response::empty(404));
        return;
    }

    // Oversized bodies are dropped unread rather than refused: a hook that
    // sees an error is a hook that might report one to the user.
    if request.body_length().is_some_and(|len| len as u64 > MAX_BODY) {
        let _ = request.respond(ok());
        return;
    }

    let mut body = Vec::new();
    let read = request
        .as_reader()
        .take(MAX_BODY)
        .read_to_end(&mut body)
        .is_ok();
    // A chunked body has no `Content-Length` to check up front, so the only
    // sign it was too big is that the reader stopped exactly at the cap.
    let truncated = body.len() as u64 >= MAX_BODY;

    // Answer before doing anything with the bytes. Clawd is an observer, and
    // an observer that can delay a turn is not free. Always 200, never a
    // status a hook runner could read as a blocking error.
    let _ = request.respond(ok());

    if !read || truncated || !running.load(Ordering::Relaxed) {
        if truncated {
            log(&format!("dropped an oversized hook body (>{MAX_BODY} bytes)"));
        }
        return;
    }

    match clawd_core::hook::translate(&body) {
        Ok(Some(event)) => sink.emit(event),
        // Understood, and deliberately not an event — most of a turn's
        // traffic lands here.
        Ok(None) => {}
        Err(error) => {
            let count = malformed.fetch_add(1, Ordering::Relaxed) + 1;
            log(&format!("malformed hook body (#{count}): {error}"));
        }
    }
}

fn is_hook_post(request: &Request) -> bool {
    if request.method().as_str() != "POST" {
        return false;
    }
    // Strip any query string before comparing.
    let path = request.url().split('?').next().unwrap_or_default();
    if path != "/hook" {
        return false;
    }
    // Requiring JSON is what closes the browser vector: a page can POST
    // text/plain or a form body cross-origin without a preflight, but not
    // application/json, and Clawd sends no CORS headers for a preflight to
    // succeed against. Prefix match, to allow `; charset=utf-8`.
    request.headers().iter().any(|header| {
        header.field.equiv("Content-Type")
            && header
                .value
                .as_str()
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("application/json")
    })
}

/// The only response this listener ever sends.
///
/// An empty object rather than an empty body: a hook runner may parse the
/// response as hook output, and `{}` is the unambiguous "no opinion".
fn ok() -> Response<std::io::Cursor<Vec<u8>>> {
    let json = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]);
    let response = Response::from_string("{}");
    match json {
        Ok(header) => response.with_header(header),
        Err(()) => response,
    }
}

/// The port to ask for, from the environment or the default.
pub fn requested_port() -> u16 {
    std::env::var("CLAWD_HOOK_PORT")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Record the bound port next to `positions.json`, so `npm run hooks:install`
/// can point the config at the port actually in use rather than assuming.
///
/// Follows `position.rs`: every failure is swallowed, because a pet that
/// cannot write a hint file is still a working pet.
pub fn save_port(app: &AppHandle, port: u16) {
    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(&PortFile { port }) {
        let _ = std::fs::write(dir.join("hook-port.json"), text);
    }
}

/// Remove the hint file, so a stale port cannot outlive the process that bound
/// it and send the installer somewhere nothing is listening.
pub fn clear_port(app: &AppHandle) {
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = std::fs::remove_file(dir.join("hook-port.json"));
    }
}

#[cfg(feature = "devtools")]
fn log(message: &str) {
    eprintln!("[clawd hook] {message}");
}

#[cfg(not(feature = "devtools"))]
fn log(_message: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::Duration;

    /// Start a listener on an unused port and return it with its receiver.
    fn listener() -> (HookSource, u16, std::sync::mpsc::Receiver<clawd_core::SessionEvent>) {
        let (tx, rx) = clawd_core::source::channel();
        // Port 0 asks the OS for any free port, so tests never collide.
        let mut source = HookSource::new(0);
        source.start(EventSink::new(tx));
        let port = source.port().expect("bound a port");
        (source, port, rx)
    }

    /// Send a raw request and return the status line.
    fn send(port: u16, request: &str) -> String {
        let mut stream =
            TcpStream::connect(("127.0.0.1", port)).expect("the listener is accepting");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("a read timeout");
        stream.write_all(request.as_bytes()).expect("wrote");
        stream.flush().expect("flushed");
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        String::from_utf8_lossy(&response)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    }

    fn post(port: u16, content_type: &str, body: &str) -> String {
        send(
            port,
            &format!(
                "POST /hook HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: {content_type}\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    #[test]
    fn a_posted_hook_payload_becomes_an_event() {
        let (mut source, port, rx) = listener();

        let status = post(
            port,
            "application/json",
            r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1"}"#,
        );
        assert!(status.contains("200"), "status was {status:?}");

        let event = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("an event arrived");
        assert_eq!(event.session_id, "s1");
        assert_eq!(event.kind, clawd_core::EventKind::PromptSubmitted);

        source.stop();
    }

    #[test]
    fn a_charset_suffix_on_the_content_type_is_accepted() {
        let (mut source, port, rx) = listener();
        post(
            port,
            "application/json; charset=utf-8",
            r#"{"hook_event_name":"Stop","session_id":"s1"}"#,
        );
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
        source.stop();
    }

    #[test]
    fn everything_that_is_not_a_hook_post_is_refused() {
        let (mut source, port, rx) = listener();

        let body = r#"{"hook_event_name":"Stop","session_id":"s1"}"#;
        let cases = [
            // Wrong method.
            format!("GET /hook HTTP/1.1\r\nHost: h\r\nConnection: close\r\n\r\n"),
            // Wrong path.
            format!("POST / HTTP/1.1\r\nHost: h\r\nConnection: close\r\n\r\n"),
        ];
        for request in cases {
            let status = send(port, &request);
            assert!(status.contains("404"), "status was {status:?}");
        }

        // A form content type is what a cross-origin page could send without
        // a preflight, so it must not be accepted.
        let status = post(port, "text/plain", body);
        assert!(status.contains("404"), "status was {status:?}");

        assert!(
            rx.recv_timeout(Duration::from_millis(250)).is_err(),
            "a refused request produced an event"
        );
        source.stop();
    }

    /// A malformed body must be answered, counted, and otherwise ignored —
    /// never allowed to panic a worker, which under `panic = "abort"` would
    /// take the whole app down.
    #[test]
    fn a_malformed_body_is_answered_and_dropped() {
        let (mut source, port, rx) = listener();

        for body in ["not json", "{", r#"{"hello":"world"}"#, ""] {
            let status = post(port, "application/json", body);
            assert!(status.contains("200"), "status was {status:?} for {body:?}");
        }

        assert!(rx.recv_timeout(Duration::from_millis(250)).is_err());
        assert_eq!(source.malformed.load(Ordering::Relaxed), 4);

        // Still serving.
        post(
            port,
            "application/json",
            r#"{"hook_event_name":"Stop","session_id":"s1"}"#,
        );
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());

        source.stop();
    }

    /// A subagent payload is a well-formed body that is deliberately not an
    /// event, so it must be answered without being counted as malformed.
    #[test]
    fn a_dropped_payload_is_not_counted_as_malformed() {
        let (mut source, port, rx) = listener();
        let status = post(
            port,
            "application/json",
            r#"{"hook_event_name":"Stop","session_id":"s1","agent_id":"a1"}"#,
        );
        assert!(status.contains("200"));
        assert!(rx.recv_timeout(Duration::from_millis(250)).is_err());
        assert_eq!(source.malformed.load(Ordering::Relaxed), 0);
        source.stop();
    }

    #[test]
    fn an_oversized_body_is_answered_without_being_read() {
        let (mut source, port, rx) = listener();
        let status = send(
            port,
            &format!(
                "POST /hook HTTP/1.1\r\nHost: h\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_BODY + 1
            ),
        );
        assert!(status.contains("200"), "status was {status:?}");
        assert!(rx.recv_timeout(Duration::from_millis(250)).is_err());
        source.stop();
    }

    /// A taken port must not be fatal: the second instance simply runs without
    /// a listener rather than crashing, or stealing the first one's socket.
    #[test]
    fn a_second_listener_on_a_taken_port_gives_up_quietly() {
        let (mut first, port, _rx) = listener();

        // Ask for exactly the taken port, with no room to scan.
        let (tx, _rx2) = clawd_core::source::channel();
        let mut second = HookSource::new(port);
        second.start(EventSink::new(tx));

        // It either found a neighbouring port or none at all; what matters is
        // that it did not take the first one's.
        assert_ne!(second.port(), Some(port));

        second.stop();
        first.stop();
    }
}
