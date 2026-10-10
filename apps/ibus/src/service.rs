//! The IBus engine: a process ibus-daemon starts, which answers on the IBus
//! bus as a factory of engines. IBus makes one and moves it from field to
//! field.
//!
//! The profile and the fields cannot cross threads, while D-Bus methods run
//! on the connection's executor, so they live on a thread of their own that
//! the methods ask through a channel.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use kanaemi_core::Event;
use kanaemi_runtime::{Field, Profile};
use zbus::names::BusName;
use zbus::object_server::ObjectServer;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
use zbus::{Connection, interface};

use crate::ibus::{self, Property};
use crate::keys::{Keys, RELEASE_MASK};
use crate::reply::{self, Reply, Signal};

/// The bus name ibus-daemon expects, as the component file gives it.
const BUS_NAME: &str = "org.freedesktop.IBus.Kanaemi";
const ENGINE_INTERFACE: &str = "org.freedesktop.IBus.Engine";
const SETTINGS_KEY: &str = "settings";
/// The settings app, installed beside the engine.
const SETTINGS_APP: &str = "kanaemi-settings";

/// One input context's field, with what IBus last said of it.
struct Context {
    field: Field,
    password: bool,
    private: bool,
}

/// Everything the engines share, on the thread that owns it.
struct Shell {
    profile: Profile,
    contexts: HashMap<u32, Context>,
    focused: Option<u32>,
    /// The keyboard's keys down, whichever context has the focus.
    keys: Keys,
    started: Instant,
    /// Whether the mode is shown by the caret.
    indicator: bool,
    /// The IBus bus, once connected.
    connection: Option<Connection>,
}

impl Shell {
    fn new(dir: PathBuf) -> Self {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        Self {
            profile: Profile::open(dir),
            contexts: HashMap::new(),
            focused: None,
            keys: Keys::default(),
            started: Instant::now(),
            indicator: reply::shows_indicator(&desktop),
            connection: None,
        }
    }

    fn create(&mut self, id: u32) {
        let context = Context {
            field: Field::new(&self.profile),
            password: false,
            private: false,
        };
        self.contexts.insert(id, context);
    }

    /// Feeds one event to the context's field and says what to tell IBus. A
    /// panic clears the preedit, starts the field over and hands the key to
    /// the application.
    fn handle(&mut self, id: u32, event: Event) -> Reply {
        let Some(context) = self.contexts.get_mut(&id) else {
            return Reply::NOTHING;
        };
        let profile = &mut self.profile;
        let handled = catch_unwind(AssertUnwindSafe(|| context.field.handle(profile, event)));
        match handled {
            Ok(output) => {
                tracing::debug!(?event, ?output, "handled");
                let mut reply = reply::reply(&output, self.indicator);
                // The text is erased by keys forwarded ahead of what follows,
                // which IBus takes without telling whether they arrived.
                if output.erase.is_some() {
                    let erased = self.handle(id, Event::Erased(true));
                    reply.signals.extend(erased.signals);
                }
                reply
            }
            Err(_) => {
                // The event is left out: it may be a key the user typed.
                tracing::warn!("handling an event panicked; the state was reset");
                context.field.restart(profile);
                Reply {
                    consumed: false,
                    signals: vec![Signal::Preedit(String::new(), 0), Signal::HideCandidates],
                }
            }
        }
    }

    fn key(&mut self, id: u32, keyval: u32, keycode: u32, state: u32) -> Reply {
        let time_ms = self.started.elapsed().as_millis() as u64;
        match self.keys.translate(keyval, keycode, state, time_ms) {
            Some(key) => self.handle(id, Event::Key(key)),
            None => Reply::NOTHING,
        }
    }

    fn focus_in(&mut self, id: u32) -> Reply {
        self.focused = Some(id);
        let password = self.contexts.get(&id).is_some_and(|c| c.password);
        self.handle(id, Event::FocusIn { password })
    }

    fn set_application(&mut self, id: u32, program: &str) {
        if let Some(context) = self.contexts.get_mut(&id) {
            context.field.set_application(program);
        }
    }

    /// Takes the field's purpose and hints. IBus tells them after the focus
    /// comes in, so a field that turns out to be a password one is focused
    /// again with that known.
    fn set_content_type(&mut self, id: u32, purpose: u32, hints: u32) -> Option<Reply> {
        let (password, private) = reply::content_type(purpose, hints);
        let context = self.contexts.get_mut(&id)?;
        context.private = private;
        context.field.set_private(private);
        let changed = std::mem::replace(&mut context.password, password) != password;
        (changed && self.focused == Some(id)).then(|| self.focus_in(id))
    }

    /// IBus moves its one engine to the next field before it tells the last
    /// one's focus went, so text committed here would go into that field.
    /// What is typed is dropped instead, and IBus clears the preedit.
    fn focus_out(&mut self, id: u32) {
        if self.focused == Some(id) {
            self.focused = None;
        }
        let Some(context) = self.contexts.get_mut(&id) else {
            return;
        };
        let profile = &mut self.profile;
        let dropped = catch_unwind(AssertUnwindSafe(|| context.field.drop_focus(profile)));
        if dropped.is_err() {
            tracing::warn!("dropping the focus panicked; the state was reset");
            context.field.restart(profile);
        }
    }

    /// Tells IBus from this thread, so what is shown follows the order the
    /// events were handled in, whoever sent them. Whether the key was used.
    fn tell(&self, id: u32, reply: Reply) -> bool {
        if let Some(connection) = &self.connection {
            zbus::block_on(emit(
                connection,
                &engine_path(id),
                reply.signals,
                self.indicator,
            ));
        }
        reply.consumed
    }

    /// Answers the requests other programs sent; a mode to set goes to the
    /// field with the focus first.
    fn serve_control(&mut self) {
        let served = catch_unwind(AssertUnwindSafe(|| {
            for request in self.profile.take_control_requests() {
                if let Some(mode) = request.mode_to_set()
                    && let Some(id) = self.focused
                {
                    let reply = self.handle(id, Event::SetMode(mode));
                    self.tell(id, reply);
                }
                self.profile.answer(request);
            }
        }));
        if served.is_err() {
            tracing::warn!("serving other programs panicked");
        }
    }

    /// Gives the panel the engine's menu, which only opens the settings: the
    /// mode is not a property, as the IME shows it itself.
    fn register_menu(&self, id: u32) {
        let Some(connection) = &self.connection else {
            return;
        };
        let menu = [Property {
            key: SETTINGS_KEY,
            label: "設定を開く…",
        }];
        let body = (ibus::property_list(&menu),);
        let path = engine_path(id);
        if let Err(error) = zbus::block_on(send(connection, &path, "RegisterProperties", &body)) {
            tracing::warn!(%error, "IBus not told");
        }
    }
}

type Job = Box<dyn FnOnce(&mut Shell) + Send>;

/// The way to the shell's thread.
#[derive(Clone)]
struct Handle(mpsc::Sender<Job>);

impl Handle {
    fn start(dir: PathBuf) -> Self {
        let (jobs, received) = mpsc::channel::<Job>();
        let waking = jobs.clone();
        std::thread::spawn(move || {
            let mut shell = Shell::new(dir);
            // Requests from other programs are served among the events, on
            // this thread, in the order they came.
            shell.profile.listen(move || {
                let _ = waking.send(Box::new(Shell::serve_control));
            });
            for job in received {
                job(&mut shell);
            }
        });
        Self(jobs)
    }

    /// Runs `act` on the shell, which tells IBus what came of it; whether
    /// the key was used.
    fn tell(
        &self,
        id: u32,
        act: impl FnOnce(&mut Shell) -> Option<Reply> + Send + 'static,
    ) -> bool {
        self.ask(move |shell| act(shell).is_some_and(|reply| shell.tell(id, reply)))
            .unwrap_or(false)
    }

    /// Runs `ask` on the shell and returns what it gives; `None` once the
    /// shell's thread has gone.
    fn ask<T: Send + 'static>(
        &self,
        ask: impl FnOnce(&mut Shell) -> T + Send + 'static,
    ) -> Option<T> {
        let (answer, answered) = mpsc::channel();
        let job: Job = Box::new(move |shell| {
            let _ = answer.send(ask(shell));
        });
        self.0.send(job).ok()?;
        answered.recv().ok()
    }
}

fn engine_path(id: u32) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("/org/freedesktop/IBus/Engine/{id}"))
        .expect("a number makes a valid path")
}

async fn emit(
    connection: &Connection,
    path: &ObjectPath<'_>,
    signals: Vec<Signal>,
    indicator: bool,
) {
    for signal in signals {
        let sent = match signal {
            Signal::Commit(text) => {
                send(connection, path, "CommitText", &(ibus::text(&text),)).await
            }
            Signal::Preedit(text, cursor) => {
                let visible = !text.is_empty();
                let body = (ibus::preedit(&text), cursor as u32, visible, 0u32);
                send(connection, path, "UpdatePreeditText", &body).await
            }
            Signal::Candidates(items, selected) => {
                if indicator {
                    // The candidates take the place by the caret.
                    INDICATOR.fetch_add(1, Ordering::Relaxed);
                    let _ = send(connection, path, "HideAuxiliaryText", &()).await;
                }
                let body = (ibus::lookup_table(&items, selected), true);
                send(connection, path, "UpdateLookupTable", &body).await
            }
            Signal::HideCandidates => send(connection, path, "HideLookupTable", &()).await,
            Signal::Forward(keyval, state) => {
                let press = (keyval, 0u32, state);
                let release = (keyval, 0u32, state | RELEASE_MASK);
                match send(connection, path, "ForwardKeyEvent", &press).await {
                    Ok(()) => send(connection, path, "ForwardKeyEvent", &release).await,
                    failed => failed,
                }
            }
            Signal::Indicator(mode) => {
                let label = ibus::text(reply::mode_label(mode));
                let shown = send(connection, path, "UpdateAuxiliaryText", &(label, true)).await;
                hide_indicator_later(connection.clone(), path.to_owned().into());
                shown
            }
        };
        if let Err(error) = sent {
            tracing::warn!(%error, "IBus not told");
        }
    }
}

async fn send<B>(
    connection: &Connection,
    path: &ObjectPath<'_>,
    name: &str,
    body: &B,
) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    connection
        .emit_signal(None::<BusName<'_>>, path, ENGINE_INTERFACE, name, body)
        .await
}

/// Counts the times the mode was shown, so only the latest hides it.
static INDICATOR: AtomicU64 = AtomicU64::new(0);
/// How long the mode shows by the caret.
const INDICATOR_VISIBLE: Duration = Duration::from_millis(800);

/// Hides the mode shown by the caret once a moment has passed, unless it
/// was shown again or the candidates took its place meanwhile.
fn hide_indicator_later(connection: Connection, path: OwnedObjectPath) {
    let shown = INDICATOR.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(INDICATOR_VISIBLE);
        if INDICATOR.load(Ordering::Relaxed) == shown {
            let hidden = zbus::block_on(send(&connection, &path, "HideAuxiliaryText", &()));
            if let Err(error) = hidden {
                tracing::warn!(%error, "IBus not told");
            }
        }
    });
}

/// Makes an engine for each input context IBus asks for.
struct Factory {
    shell: Handle,
    next: AtomicU32,
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl Factory {
    async fn create_engine(
        &self,
        _name: &str,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.shell.ask(move |shell| shell.create(id));
        let path = engine_path(id);
        let engine = Engine {
            id,
            shell: self.shell.clone(),
            content_type: (0, 0),
        };
        server.at(&path, engine).await?;
        server
            .at(
                &path,
                Service {
                    id,
                    shell: self.shell.clone(),
                },
            )
            .await?;
        Ok(path)
    }
}

/// An engine IBus made, with the field it feeds.
struct Engine {
    id: u32,
    shell: Handle,
    /// The purpose and hints last set, as IBus reads them back.
    content_type: (u32, u32),
}

impl Engine {
    fn handle(&self, event: Event) {
        let id = self.id;
        self.shell
            .tell(id, move |shell| Some(shell.handle(id, event)));
    }
}

// Each call is handled before the next is taken, in the order IBus sent them;
// zbus would otherwise run them as tasks of their own, in any order.
#[interface(name = "org.freedesktop.IBus.Engine", spawn = false)]
impl Engine {
    async fn process_key_event(&self, keyval: u32, keycode: u32, state: u32) -> bool {
        let id = self.id;
        self.shell
            .tell(id, move |shell| Some(shell.key(id, keyval, keycode, state)))
    }

    async fn focus_in(&self) {
        let id = self.id;
        self.shell.tell(id, move |shell| {
            let reply = shell.focus_in(id);
            shell.register_menu(id);
            Some(reply)
        });
    }

    async fn focus_in_id(&self, _object_path: &str, client: &str) {
        // The one engine moves between fields: one naming no program is in
        // none, not in the one before.
        let id = self.id;
        let program = reply::program(client).unwrap_or_default().to_owned();
        self.shell
            .ask(move |shell| shell.set_application(id, &program));
        self.focus_in().await;
    }

    async fn focus_out(&self) {
        let id = self.id;
        self.shell.ask(move |shell| shell.focus_out(id));
    }

    async fn focus_out_id(&self, _object_path: &str) {
        self.focus_out().await;
    }

    /// The application asks for the preedit to be committed, as on a click.
    async fn reset(&self) {
        self.handle(Event::Flush);
    }

    async fn enable(&self) {}

    async fn disable(&self) {
        self.handle(Event::Flush);
    }

    async fn candidate_clicked(&self, index: u32, _button: u32, _state: u32) {
        self.handle(Event::Select(index as usize));
    }

    async fn property_activate(&self, name: &str, _state: u32) {
        if name == SETTINGS_KEY {
            open_settings();
        }
    }

    async fn set_cursor_location(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}
    async fn set_capabilities(&self, _caps: u32) {}
    async fn property_show(&self, _name: &str) {}
    async fn property_hide(&self, _name: &str) {}
    async fn page_up(&self) {}
    async fn page_down(&self) {}
    async fn cursor_up(&self) {}
    async fn cursor_down(&self) {}
    async fn process_hand_writing_event(&self, _coordinates: Vec<f64>) {}
    async fn cancel_hand_writing(&self, _strokes: u32) {}
    async fn set_surrounding_text(&self, _text: Value<'_>, _cursor: u32, _anchor: u32) {}
    async fn panel_extension_received(&self, _event: Value<'_>) {}
    async fn panel_extension_register_keys(&self, _data: Value<'_>) {}

    /// IBus asks for the focus by `FocusInId`, which names the client.
    #[zbus(property)]
    fn focus_id(&self) -> bool {
        true
    }

    /// The text around the cursor is not used.
    #[zbus(property)]
    fn active_surrounding_text(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn content_type(&self) -> (u32, u32) {
        self.content_type
    }

    /// The field's purpose and hints: a password field is typed in without
    /// kana, and a private one is not recorded.
    #[zbus(property)]
    async fn set_content_type(&mut self, value: (u32, u32)) {
        self.content_type = value;
        let (purpose, hints) = value;
        let id = self.id;
        self.shell
            .tell(id, move |shell| shell.set_content_type(id, purpose, hints));
    }
}

/// The interface IBus ends an engine through.
struct Service {
    id: u32,
    shell: Handle,
}

#[interface(name = "org.freedesktop.IBus.Service")]
impl Service {
    async fn destroy(&self, #[zbus(object_server)] server: &ObjectServer) {
        let id = self.id;
        self.shell.ask(move |shell| {
            // A context may go with the focus still in it; other programs
            // are told no field has it.
            if shell.focused == Some(id) {
                shell.focus_out(id);
            }
            shell.contexts.remove(&id);
        });
        let path = engine_path(id);
        let _ = server.remove::<Engine, _>(&path).await;
        let _ = server.remove::<Service, _>(&path).await;
    }
}

/// Opens the settings app installed beside the engine.
fn open_settings() {
    let Ok(exe) = std::env::current_exe() else {
        tracing::warn!("the engine's own path is unknown; settings app not opened");
        return;
    };
    let app = exe.with_file_name(SETTINGS_APP);
    match std::process::Command::new(&app).spawn() {
        // Waited for on a thread of its own, so no finished app lingers.
        Ok(mut child) => {
            let waiting = std::thread::Builder::new()
                .name("settings-launcher".to_owned())
                .spawn(move || child.wait());
            if let Err(error) = waiting {
                tracing::warn!(%error, "settings app not waited for");
            }
        }
        Err(error) => tracing::warn!(app = %app.display(), %error, "settings app not opened"),
    }
}

/// The IBus bus: the address ibus-daemon gives its components, or else the
/// one the `ibus` command reports.
fn bus_address() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(address) = std::env::var("IBUS_ADDRESS") {
        return Ok(address);
    }
    let output = std::process::Command::new("ibus").arg("address").output()?;
    let address = String::from_utf8(output.stdout)?.trim().to_owned();
    if address.is_empty() || address == "(null)" {
        return Err("ibus-daemon is not running".into());
    }
    Ok(address)
}

/// Serves engines until ibus-daemon goes away.
pub fn run(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let shell = Handle::start(dir);
    let factory = Factory {
        shell: shell.clone(),
        next: AtomicU32::new(1),
    };
    let address = bus_address()?;
    let connection = zbus::block_on(async {
        zbus::connection::Builder::address(address.as_str())?
            .serve_at("/org/freedesktop/IBus/Factory", factory)?
            .name(BUS_NAME)?
            .build()
            .await
    })?;
    tracing::info!(version = kanaemi_core::VERSION, "kanaemi started");
    let bus = connection.clone();
    shell.ask(move |shell| shell.connection = Some(bus));
    // Every message passes here as well; the messages end when the bus goes,
    // even when ibus-daemon is killed and cannot end the engine itself.
    for _ in zbus::blocking::MessageIterator::from(zbus::blocking::Connection::from(connection)) {}
    tracing::info!("IBus went away");
    Ok(())
}
