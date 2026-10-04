//! Requests from other programs, such as editors, on a TCP port of the local
//! machine: the mode of the field with the focus, read, set and watched.
//!
//! Each connection is read and written on threads of its own, so a program
//! that stops reading never holds the IME up; the requests are answered in
//! the order they came, on the thread the platform serves fields on.
//!
//! Anyone on the machine can connect, so everything a connection can make
//! the IME hold is bounded: the connections, the requests waiting, the
//! answers not yet read and the length of a line. A connection that goes
//! past a bound is ended.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use kanaemi_core::Mode;
use serde_json::{Map, Value, json};

/// Asks the thread fields are served on to take the requests that came in.
type Wake = Arc<dyn Fn() + Send + Sync>;

/// The longest line a request may take; no request comes near it.
const MAX_LINE: u64 = 4 * 1024;
/// How many connections are served at once; more are ended at once.
const MAX_CONNECTIONS: usize = 16;
/// How many requests wait to be served, from every connection together; a
/// connection sending more waits for room.
const MAX_WAITING: usize = 64;
/// How many lines wait for a connection to read them.
const MAX_UNREAD: usize = 64;
/// How often a connection's writer looks whether its reader has ended.
const WRITER_POLL: Duration = Duration::from_millis(500);

/// A request from another program, to answer with [`crate::Profile::answer`]
/// or [`ControlPort::answer`].
pub struct ControlRequest {
    id: Option<Value>,
    kind: Kind,
    from: Arc<Link>,
}

// Nobody connecting is checked: any program on the machine, another OS
// user's included, may send these. A request added here must be harmless in
// anyone's hands; reading or changing what the user typed, their
// dictionaries or their records needs the connection checked first.
enum Kind {
    GetMode,
    SetMode(Mode),
    WatchMode,
    /// A JSON object that is no request this IME knows.
    Bad(&'static str),
}

impl ControlRequest {
    /// The mode to put the field with the focus in, before answering.
    pub fn mode_to_set(&self) -> Option<Mode> {
        match self.kind {
            Kind::SetMode(mode) => Some(mode),
            _ => None,
        }
    }

    /// Answers with the mode of the field with the focus, or why not; a
    /// watch is answered even without one, and gives the connection to tell
    /// changes to.
    fn answer(self, focused: Option<Mode>) -> Option<Arc<Link>> {
        let mut body = match (&self.kind, focused) {
            (Kind::Bad(message), _) => error("bad-request", message),
            (Kind::WatchMode, mode) => json!({ "mode": mode.map(mode_name) }),
            (Kind::GetMode | Kind::SetMode(_), Some(mode)) => json!({ "mode": mode_name(mode) }),
            (Kind::GetMode | Kind::SetMode(_), None) => {
                error("no-field", "no field that uses Kanaemi has the focus")
            }
        };
        if let (Some(id), Value::Object(fields)) = (self.id, &mut body) {
            fields.insert("id".to_owned(), id);
        }
        self.from.send(body.to_string());
        matches!(self.kind, Kind::WatchMode).then_some(self.from)
    }
}

/// One connection, as its answers are sent.
struct Link {
    id: u64,
    lines: SyncSender<String>,
    stream: TcpStream,
    /// Set once the connection's reader has ended.
    ended: Arc<AtomicBool>,
}

impl Link {
    /// Hands `line` to the connection's writer; one that has not read the
    /// lines before it is ended. Returns whether the connection goes on.
    fn send(&self, line: String) -> bool {
        match self.lines.try_send(line) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                let _ = self.stream.shutdown(Shutdown::Both);
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    fn ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }
}

/// The open connections, by id, to count and to end.
type Connections = Arc<Mutex<HashMap<u64, TcpStream>>>;

/// The port the settings name, taking requests while it is held.
struct Server {
    port: u16,
    incoming: Receiver<ControlRequest>,
    /// Whether a wake is on its way, so a burst of requests wakes once.
    waking: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    connections: Connections,
    /// Connections that asked to hear of changes, and the mode last told.
    watchers: Vec<Arc<Link>>,
    told: Option<Mode>,
}

impl Server {
    /// Listens on `port` of the local machine, waking the serving thread
    /// with `wake` when requests come in.
    fn listen(port: u16, wake: Wake) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let (requests, incoming) = mpsc::sync_channel(MAX_WAITING);
        let waking = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let connections = Connections::default();
        let shared = Shared {
            requests,
            wake,
            waking: waking.clone(),
            connections: connections.clone(),
        };
        let listener = std::thread::Builder::new()
            .name("control-listener".to_owned())
            .spawn({
                let stopped = stopped.clone();
                move || accept(listener, &stopped, &shared)
            })?;
        Ok(Self {
            port,
            incoming,
            waking,
            stopped,
            listener: Some(listener),
            connections,
            watchers: Vec::new(),
            told: None,
        })
    }

    fn port(&self) -> u16 {
        self.port
    }

    /// The requests that came in since the last call, oldest first. Ones
    /// that come in meanwhile wake the serving thread again.
    fn take(&mut self) -> Vec<ControlRequest> {
        self.waking.store(false, Ordering::SeqCst);
        self.forget_ended();
        self.incoming.try_iter().take(MAX_WAITING).collect()
    }

    /// Tells changes to `link` from now on, once however often it asks.
    fn watch(&mut self, link: Arc<Link>) {
        self.forget_ended();
        if !self.watchers.iter().any(|watcher| watcher.id == link.id) {
            self.watchers.push(link);
        }
    }

    /// Lets go of watching connections that ended, so they hold nothing
    /// until the mode next changes.
    fn forget_ended(&mut self) {
        self.watchers.retain(|watcher| !watcher.ended());
    }

    /// Tells every watching connection the mode of the field with the
    /// focus, when it is not the one last told.
    fn tell(&mut self, mode: Option<Mode>) {
        if mode == self.told {
            return;
        }
        self.told = mode;
        let event = json!({ "event": "mode", "mode": mode.map(mode_name) }).to_string();
        self.watchers.retain(|watcher| watcher.send(event.clone()));
    }
}

/// The port the settings name, for an input method that follows the field
/// with the focus itself rather than through a [`crate::Profile`]: one whose
/// fields live in other processes, which tell it their focus and mode.
pub struct ControlPort {
    wake: Wake,
    server: Option<Server>,
    /// The mode last told, so a port listened on anew starts from it.
    told: Option<Mode>,
}

impl ControlPort {
    /// Listens on no port yet. Requests that come in wake the thread that
    /// serves them with `wake`, which may be called from any thread.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            wake: Arc::new(wake),
            server: None,
            told: None,
        }
    }

    /// Listens on `port`, as the settings name it, letting go of any other;
    /// on none for `None`. A port that cannot be listened on is logged and
    /// tried again at the next call.
    pub fn listen_on(&mut self, port: Option<u16>) {
        if self.server.as_ref().map(Server::port) == port {
            return;
        }
        self.server = None;
        let Some(port) = port else {
            return;
        };
        match Server::listen(port, self.wake.clone()) {
            Ok(mut server) => {
                tracing::info!(port, "listening for other programs");
                server.told = self.told;
                self.server = Some(server);
            }
            Err(error) => tracing::warn!(port, %error, "control port not listened on"),
        }
    }

    /// The requests that came in since the last call, oldest first. Answer
    /// each with [`ControlPort::answer`], in order, after putting the field
    /// with the focus in the mode it asks for.
    pub fn take_requests(&mut self) -> Vec<ControlRequest> {
        self.server.as_mut().map(Server::take).unwrap_or_default()
    }

    /// Answers `request` with `focused`, the mode of the field with the
    /// focus now, or `None` when no field has it.
    pub fn answer(&mut self, request: ControlRequest, focused: Option<Mode>) {
        let watcher = request.answer(focused);
        if let (Some(watcher), Some(server)) = (watcher, self.server.as_mut()) {
            server.watch(watcher);
        }
    }

    /// Tells watching programs `focused`, the mode of the field with the
    /// focus, when it is not the one last told.
    pub fn tell(&mut self, focused: Option<Mode>) {
        self.told = focused;
        if let Some(server) = &mut self.server {
            server.tell(focused);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        // Wakes the listener out of accepting and waits for it to let the
        // port go, so the port can be listened on again at once. Without
        // the wake it would never return, so it is not waited for then.
        if TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).is_ok()
            && let Some(listener) = self.listener.take()
        {
            let _ = listener.join();
        }
        let connections = self
            .connections
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for stream in connections.values() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

/// What every connection's reader shares.
#[derive(Clone)]
struct Shared {
    requests: SyncSender<ControlRequest>,
    wake: Wake,
    waking: Arc<AtomicBool>,
    connections: Connections,
}

fn accept(listener: TcpListener, stopped: &AtomicBool, shared: &Shared) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for stream in listener.incoming() {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                tracing::debug!(%error, "control connection not accepted");
                continue;
            }
        };
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        {
            let mut connections = shared
                .connections
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let tracked = stream.try_clone();
            match tracked {
                Ok(tracked) if connections.len() < MAX_CONNECTIONS => {
                    connections.insert(id, tracked);
                }
                _ => {
                    tracing::debug!("control connection refused");
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
            }
        }
        let spawned = std::thread::Builder::new()
            .name("control-connection".to_owned())
            .spawn({
                let shared = shared.clone();
                move || serve_connection(id, stream, &shared)
            });
        if let Err(error) = spawned {
            tracing::debug!(%error, "control connection not served");
            let tracked = shared
                .connections
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&id);
            if let Some(stream) = tracked {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }
}

/// Reads requests until the connection ends or a line is not a JSON object,
/// which ends it unanswered: a program speaking HTTP or anything else is
/// never answered.
fn serve_connection(id: u64, stream: TcpStream, shared: &Shared) {
    let ended = Arc::new(AtomicBool::new(false));
    if let Some(link) = start_writer(id, &stream, &ended) {
        read_requests(&stream, &link, shared);
    }
    ended.store(true, Ordering::SeqCst);
    let _ = stream.shutdown(Shutdown::Both);
    shared
        .connections
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&id);
}

fn start_writer(id: u64, stream: &TcpStream, ended: &Arc<AtomicBool>) -> Option<Arc<Link>> {
    let (writer, link_stream) = (stream.try_clone().ok()?, stream.try_clone().ok()?);
    let (lines, unread) = mpsc::sync_channel::<String>(MAX_UNREAD);
    std::thread::Builder::new()
        .name("control-writer".to_owned())
        .spawn({
            let ended = ended.clone();
            move || write_lines(writer, &unread, &ended)
        })
        .ok()?;
    Some(Arc::new(Link {
        id,
        lines,
        stream: link_stream,
        ended: ended.clone(),
    }))
}

fn read_requests(stream: &TcpStream, link: &Arc<Link>, shared: &Shared) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match (&mut reader).take(MAX_LINE + 1).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) if !line.ends_with('\n') && line.len() as u64 > MAX_LINE => return,
            Ok(_) => {}
        }
        let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(line.trim_end()) else {
            return;
        };
        let request = ControlRequest {
            id: fields.get("id").cloned(),
            kind: kind(&fields),
            from: link.clone(),
        };
        if shared.requests.send(request).is_err() {
            // The server stopped listening.
            return;
        }
        if !shared.waking.swap(true, Ordering::SeqCst) {
            (shared.wake)();
        }
    }
}

/// Writes lines until the connection fails or its reader has ended; a line
/// it cannot write ends it, as the other side stopped reading.
fn write_lines(mut stream: TcpStream, lines: &Receiver<String>, ended: &AtomicBool) {
    loop {
        match lines.recv_timeout(WRITER_POLL) {
            Ok(line) => {
                if stream.write_all(format!("{line}\n").as_bytes()).is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) if !ended.load(Ordering::SeqCst) => {}
            Err(_) => return,
        }
    }
}

fn kind(fields: &Map<String, Value>) -> Kind {
    match fields.get("op").and_then(Value::as_str) {
        Some("get-mode") => Kind::GetMode,
        Some("watch-mode") => Kind::WatchMode,
        Some("set-mode") => match fields
            .get("mode")
            .and_then(Value::as_str)
            .and_then(named_mode)
        {
            Some(mode) => Kind::SetMode(mode),
            None => Kind::Bad("mode is kana or abc"),
        },
        _ => Kind::Bad("op is get-mode, set-mode or watch-mode"),
    }
}

fn error(code: &str, message: &str) -> Value {
    json!({ "error": code, "message": message })
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Kana => "kana",
        Mode::Abc => "abc",
    }
}

fn named_mode(name: &str) -> Option<Mode> {
    match name {
        "kana" => Some(Mode::Kana),
        "abc" => Some(Mode::Abc),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connections_past_the_limit_are_ended_at_once() {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let _server = Server::listen(port, Arc::new(|| {})).unwrap();
        let held: Vec<TcpStream> = (0..MAX_CONNECTIONS)
            .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap())
            .collect();
        let mut over = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        over.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut rest = Vec::new();
        assert!(over.read_to_end(&mut rest).is_ok(), "ended");
        assert!(rest.is_empty());
        drop(held);
    }
}
