//! Other programs asking the IME for the mode of the field with the focus,
//! over the port the settings name, as an input method serves them.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use kanaemi_config::{DICTIONARY_DIR, FILE_NAME};
use kanaemi_core::{Event, Key, KeyEvent, KeyKind, Modifiers};
use kanaemi_runtime::{Field, Profile};

const FOCUS_IN: Event = Event::FocusIn { password: false };
const WAIT: Duration = Duration::from_secs(5);

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kanaemi-runtime-control-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
    dir
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn press(key: Key) -> Event {
    Event::Key(KeyEvent {
        key,
        mods: Modifiers::default(),
        kind: KeyKind::Press,
        time_ms: 0,
    })
}

/// An input method with one field and the control port open, serving
/// requests on this thread as a platform serves them on its own.
struct Ime {
    profile: Profile,
    field: Field,
    woken: Receiver<()>,
    port: u16,
    dir: PathBuf,
}

impl Ime {
    fn open(name: &str) -> Self {
        let dir = temp_dir(name);
        let port = free_port();
        fs::write(dir.join(FILE_NAME), format!("[control]\nport = {port}\n")).unwrap();
        let mut profile = Profile::open(&dir);
        let (wake, woken) = mpsc::channel();
        profile.listen(move || {
            let _ = wake.send(());
        });
        let field = Field::new(&profile);
        Self {
            profile,
            field,
            woken,
            port,
            dir,
        }
    }

    fn profile_dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn handle(&mut self, event: Event) {
        self.field.handle(&mut self.profile, event);
    }

    /// Answers one request.
    fn serve(&mut self) {
        self.serve_requests(1);
    }

    /// Answers `count` requests, waking as many times as they take to come
    /// in; a mode to set goes to the field, as to the one with the focus.
    fn serve_requests(&mut self, count: usize) {
        let mut answered = 0;
        while answered < count {
            self.woken.recv_timeout(WAIT).expect("woken for a request");
            for request in self.profile.take_control_requests() {
                if let Some(mode) = request.mode_to_set() {
                    self.field.handle(&mut self.profile, Event::SetMode(mode));
                }
                self.profile.answer(request);
                answered += 1;
            }
        }
    }
}

struct Client {
    lines: BufReader<TcpStream>,
}

impl Client {
    fn connect(ime: &Ime) -> Self {
        let stream = TcpStream::connect(("127.0.0.1", ime.port)).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        Self {
            lines: BufReader::new(stream),
        }
    }

    fn send(&mut self, line: &str) {
        let stream = self.lines.get_mut();
        stream.write_all(line.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
    }

    fn receive(&mut self) -> serde_json::Value {
        let mut line = String::new();
        self.lines.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line:?}"))
    }

    /// Whether the IME closed the connection without a word.
    fn closed(&mut self) -> bool {
        let mut rest = Vec::new();
        self.lines.read_to_end(&mut rest).is_ok() && rest.is_empty()
    }
}

#[test]
fn the_mode_of_the_field_with_the_focus_is_read_with_the_id_given() {
    let mut ime = Ime::open("get");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "get-mode", "id": 7}"#);
    ime.serve();
    assert_eq!(
        client.receive(),
        serde_json::json!({"id": 7, "mode": "abc"})
    );
}

#[test]
fn without_a_field_with_the_focus_there_is_no_mode() {
    let mut ime = Ime::open("no-field");
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "get-mode"}"#);
    ime.serve();
    assert_eq!(client.receive()["error"], "no-field");
    client.send(r#"{"op": "set-mode", "mode": "kana"}"#);
    ime.serve();
    assert_eq!(client.receive()["error"], "no-field");
}

#[test]
fn the_mode_is_set_and_the_answer_is_the_mode_after() {
    let mut ime = Ime::open("set");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "set-mode", "mode": "kana", "id": "a"}"#);
    ime.serve();
    assert_eq!(
        client.receive(),
        serde_json::json!({"id": "a", "mode": "kana"})
    );
    client.send(r#"{"op": "set-mode", "mode": "kana", "id": "b"}"#);
    ime.serve();
    assert_eq!(
        client.receive(),
        serde_json::json!({"id": "b", "mode": "kana"})
    );
}

#[test]
fn requests_sent_together_are_answered_in_order() {
    let mut ime = Ime::open("order");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(
        "{\"op\": \"set-mode\", \"mode\": \"kana\", \"id\": 1}\n{\"op\": \"get-mode\", \"id\": 2}",
    );
    ime.serve_requests(2);
    assert_eq!(
        client.receive(),
        serde_json::json!({"id": 1, "mode": "kana"})
    );
    assert_eq!(
        client.receive(),
        serde_json::json!({"id": 2, "mode": "kana"})
    );
}

#[test]
fn a_watch_hears_every_change_of_the_mode_with_the_focus() {
    let mut ime = Ime::open("watch");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "watch-mode", "id": 1}"#);
    ime.serve();
    assert_eq!(
        client.receive(),
        serde_json::json!({"id": 1, "mode": "abc"})
    );
    ime.handle(press(Key::Kana));
    assert_eq!(
        client.receive(),
        serde_json::json!({"event": "mode", "mode": "kana"})
    );
    ime.handle(Event::FocusOut);
    assert_eq!(
        client.receive(),
        serde_json::json!({"event": "mode", "mode": null})
    );
    ime.handle(FOCUS_IN);
    assert_eq!(
        client.receive(),
        serde_json::json!({"event": "mode", "mode": "abc"})
    );
}

#[test]
fn a_watch_without_a_field_with_the_focus_starts_from_no_mode() {
    let mut ime = Ime::open("watch-no-field");
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "watch-mode"}"#);
    ime.serve();
    assert_eq!(client.receive(), serde_json::json!({"mode": null}));
}

#[test]
fn an_unknown_request_is_answered_with_an_error_and_the_connection_stays() {
    let mut ime = Ime::open("bad");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "get-text", "id": 1}"#);
    ime.serve();
    let answer = client.receive();
    assert_eq!(
        (&answer["id"], &answer["error"]),
        (&1.into(), &"bad-request".into())
    );
    client.send(r#"{"op": "set-mode", "mode": "hiragana", "id": 2}"#);
    ime.serve();
    assert_eq!(client.receive()["error"], "bad-request");
    client.send(r#"{"op": "get-mode", "id": 3}"#);
    ime.serve();
    assert_eq!(client.receive()["mode"], "abc");
}

#[test]
fn a_line_that_is_not_a_json_object_closes_the_connection_unanswered() {
    let ime = Ime::open("http");
    let mut client = Client::connect(&ime);
    client.send("POST / HTTP/1.1");
    client.send(r#"{"op": "set-mode", "mode": "kana"}"#);
    assert!(client.closed());
    let mut client = Client::connect(&ime);
    client.send("[1, 2]");
    assert!(client.closed());
}

#[test]
fn a_port_taken_by_another_program_leaves_the_ime_working_without_it() {
    let dir = temp_dir("taken");
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    fs::write(dir.join(FILE_NAME), format!("[control]\nport = {port}\n")).unwrap();
    let mut profile = Profile::open(&dir);
    profile.listen(|| {});
    let mut field = Field::new(&profile);
    field.handle(&mut profile, FOCUS_IN);
    assert!(profile.take_control_requests().is_empty());
}

#[test]
fn a_port_changed_in_the_settings_is_taken_when_they_are_read_again() {
    let mut ime = Ime::open("moved");
    let old = ime.port;
    let new = free_port();
    let settings = ime.profile_dir().join(FILE_NAME);
    fs::write(&settings, format!("[control]\nport = {new}\n")).unwrap();
    fs::File::options()
        .write(true)
        .open(&settings)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    ime.handle(FOCUS_IN);
    assert!(TcpStream::connect(("127.0.0.1", old)).is_err());
    ime.port = new;
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "get-mode"}"#);
    ime.serve();
    assert_eq!(client.receive()["mode"], "abc");
}

#[test]
fn watching_twice_on_one_connection_still_hears_each_change_once() {
    let mut ime = Ime::open("watch-twice");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "watch-mode", "id": 1}"#);
    client.send(r#"{"op": "watch-mode", "id": 2}"#);
    ime.serve_requests(2);
    assert_eq!(client.receive()["id"], 1);
    assert_eq!(client.receive()["id"], 2);
    ime.handle(press(Key::Kana));
    client.send(r#"{"op": "get-mode", "id": 3}"#);
    ime.serve();
    assert_eq!(
        client.receive(),
        serde_json::json!({"event": "mode", "mode": "kana"})
    );
    assert_eq!(client.receive()["id"], 3, "no second event in between");
}

#[test]
fn connections_end_when_the_port_is_taken_out_of_the_settings() {
    let mut ime = Ime::open("closed");
    ime.handle(FOCUS_IN);
    let mut client = Client::connect(&ime);
    client.send(r#"{"op": "watch-mode"}"#);
    ime.serve();
    client.receive();
    let settings = ime.profile_dir().join(FILE_NAME);
    fs::write(&settings, "").unwrap();
    fs::File::options()
        .write(true)
        .open(&settings)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    ime.handle(Event::FocusOut);
    ime.handle(FOCUS_IN);
    // What was told before the port went may still come first.
    let mut rest = String::new();
    let ended = client.lines.read_to_string(&mut rest);
    assert!(ended.is_ok(), "the connection ended: {ended:?}");
    assert!(
        rest.lines().all(|line| line.contains("\"event\"")),
        "{rest:?}"
    );
}
