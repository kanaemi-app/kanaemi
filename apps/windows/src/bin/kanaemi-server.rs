//! Writes for the text services that cannot: one runs in the processes of an
//! AppContainer, such as the Start menu's search box, which may neither
//! write the user's files nor read their settings folder.
//!
//! It starts when the user signs in and stays running. It lets
//! AppContainers read the settings, the dictionaries, the romaji tables and
//! the ranking model, but not the files of what the user typed; and it
//! appends the dictionary lines they send over its pipe to the user custom
//! dictionary.
//!
//! It also takes other programs' requests on the port the settings name, as
//! the text services, one in each application, cannot share one port.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    server::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("kanaemi-server runs only on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
mod server {
    use std::collections::VecDeque;
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    use kanaemi::pipe::{self, MAX_LINE};
    use kanaemi_config::{DICTIONARY_DIR, FILE_NAME, MODEL_FILE, ROMAJI_DIR, USER_CUSTOM_FILE};
    use kanaemi_engine::{FileSink, LineSink, TextDictionary};
    use windows::Win32::Foundation::*;
    use windows::Win32::Security::Authorization::*;
    use windows::Win32::Security::*;
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::System::Pipes::*;
    use windows::core::*;

    /// All application packages, and the less privileged ones the Start
    /// menu's search box runs as.
    const APP_CONTAINERS: [&str; 2] = ["*S-1-15-2-1", "*S-1-15-2-2"];
    /// How many lines that failed to be written are kept to try again.
    const MAX_UNWRITTEN: usize = 1000;
    /// How long a client may take to send its line.
    const RECEIVE_TIMEOUT: Duration = Duration::from_secs(2);
    /// Keeps a console window from flashing for each `icacls`.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn run() {
        kanaemi_runtime::init_logging();
        let Some(dir) = kanaemi_config::dir() else {
            tracing::warn!("APPDATA is not set; nothing to serve");
            return;
        };
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), %error, "settings folder not created");
        }
        let name = match pipe::name() {
            Ok(name) => name,
            Err(error) => {
                tracing::warn!(%error, "pipe name unknown");
                return;
            }
        };
        let readable = dir.clone();
        std::thread::spawn(move || keep_readable(&readable));
        // Only the server that holds the pipe serves the port.
        let serving = {
            let dir = dir.clone();
            move || control::start(dir)
        };
        match serve(&name, &dir, serving) {
            // Another server of this user already answers.
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.0 as i32) => {
                tracing::info!("another server is running");
            }
            Err(error) => tracing::warn!(%error, "pipe not served"),
            Ok(()) => {}
        }
    }

    /// Grants AppContainers read access to what they may read, again each
    /// time the settings folder changes: the settings file and the model are
    /// replaced whole, and a new file does not inherit a grant meant for the
    /// folder alone.
    fn keep_readable(dir: &Path) {
        let wide: Vec<u16> = dir
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain([0])
            .collect();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_LIST_DIRECTORY.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        };
        grant_reads(dir);
        let Ok(handle) = handle else {
            tracing::warn!(path = %dir.display(), "settings folder not watched");
            return;
        };
        let mut buffer = vec![0u8; 4096];
        loop {
            let mut len = 0u32;
            let changed = unsafe {
                ReadDirectoryChangesW(
                    handle,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    // Files moved into the dictionary and romaji folders keep the
                    // access they had, so those folders are watched too.
                    true,
                    FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
                    Some(&mut len),
                    None,
                    None,
                )
            };
            if let Err(error) = changed {
                tracing::warn!(%error, "settings folder no longer watched");
                return;
            }
            grant_reads(dir);
        }
    }

    fn grant_reads(dir: &Path) {
        // The folder alone, so AppContainers can look up the files in it.
        grant(dir, "(RX)", false);
        // Everything already in them too: a file moved in does not inherit.
        for folder in [DICTIONARY_DIR, ROMAJI_DIR] {
            grant(&dir.join(folder), "(OI)(CI)(RX)", true);
        }
        for file in [FILE_NAME, MODEL_FILE] {
            grant(&dir.join(file), "(RX)", false);
        }
    }

    fn grant(path: &Path, rights: &str, recursive: bool) {
        if !path.exists() {
            return;
        }
        let mut icacls = Command::new("icacls");
        icacls.arg(path).creation_flags(CREATE_NO_WINDOW);
        for sid in APP_CONTAINERS {
            icacls.arg("/grant").arg(format!("{sid}:{rights}"));
        }
        if recursive {
            icacls.arg("/T");
        }
        match icacls.output() {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                tracing::warn!(path = %path.display(), status = %output.status, "read access not granted")
            }
            Err(error) => tracing::warn!(path = %path.display(), %error, "icacls not run"),
        }
    }

    /// Who may reach a pipe of this user: the user may do anything, and
    /// AppContainers only what `client_access` allows, `READ_CONTROL` among
    /// it to see who made the pipe. The low label lets processes at low
    /// integrity, as AppContainers are, reach a pipe that is otherwise
    /// medium. Kept for as long as the server runs.
    fn pipe_security(client_access: u32) -> io::Result<SECURITY_ATTRIBUTES> {
        let sid = pipe::user_sid()?;
        let sddl = format!(
            "D:(A;;GA;;;SY)(A;;GA;;;{sid})(A;;{client_access:#010x};;;AC)(A;;{client_access:#010x};;;S-1-15-2-2)S:(ML;;NW;;;LW)"
        );
        let sddl = HSTRING::from(sddl);
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(io::Error::other)?;
        Ok(SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: FALSE,
        })
    }

    /// Serves the dictionary pipe, calling `serving` once it holds it.
    fn serve(name: &str, dir: &Path, serving: impl FnOnce()) -> io::Result<()> {
        // AppContainers may only write data.
        let attributes = pipe_security(pipe::CLIENT_ACCESS)?;
        let name = HSTRING::from(name);
        let instance = |first: bool| {
            let mut mode = PIPE_ACCESS_INBOUND;
            if first {
                // Fails when another server already made the pipe.
                mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
            }
            let pipe = unsafe {
                CreateNamedPipeW(
                    &name,
                    mode,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    0,
                    MAX_LINE as u32,
                    0,
                    Some(&attributes),
                )
            };
            if pipe.is_invalid() {
                return Err(io::Error::last_os_error());
            }
            Ok(pipe)
        };
        let mut writer = Writer {
            custom: dir.join(USER_CUSTOM_FILE),
            unwritten: VecDeque::new(),
        };
        let mut next = instance(true)?;
        tracing::info!("serving the pipe");
        serving();
        loop {
            let pipe = next;
            let connected = unsafe { ConnectNamedPipe(pipe, None) };
            // A client may connect between the pipe's creation and the call.
            if connected.is_err() && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                unsafe {
                    let _ = CloseHandle(pipe);
                }
                next = instance(false)?;
                continue;
            }
            // Another instance waits while this one is read, so a client that
            // comes meanwhile is let in and read next.
            next = instance(false)?;
            // One client at a time, in the order they came: a deletion sent
            // after a registration is written after it, and a client that
            // never finishes holds the pipe for a moment, not a thread.
            let bytes = receive(pipe);
            unsafe {
                let _ = DisconnectNamedPipe(pipe);
                let _ = CloseHandle(pipe);
            }
            writer.write(&bytes);
        }
    }

    /// What one client sent, up to its newline, as much as arrives in time.
    fn receive(pipe: HANDLE) -> Vec<u8> {
        let deadline = Instant::now() + RECEIVE_TIMEOUT;
        let mut bytes = Vec::new();
        while bytes.len() <= MAX_LINE && !bytes.contains(&b'\n') && Instant::now() < deadline {
            let mut available = 0u32;
            // Fails once the client has closed and nothing is left to read.
            if unsafe { PeekNamedPipe(pipe, None, 0, None, Some(&mut available), None) }.is_err() {
                break;
            }
            if available == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let want = (available as usize).min(MAX_LINE + 1 - bytes.len());
            let mut chunk = vec![0u8; want];
            let mut read = 0u32;
            if unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut read), None) }.is_err() {
                break;
            }
            bytes.extend_from_slice(&chunk[..read as usize]);
        }
        bytes
    }

    /// Appends the lines received to the user custom dictionary. A line it
    /// could not write is written again, in its place, before the next.
    struct Writer {
        custom: PathBuf,
        unwritten: VecDeque<String>,
    }

    impl Writer {
        fn write(&mut self, bytes: &[u8]) {
            let Some(line) = valid_line(bytes) else {
                tracing::warn!(
                    len = bytes.len(),
                    "a line that is not a dictionary line was refused"
                );
                return;
            };
            // Those waiting go first, so the dictionary keeps the order sent.
            if self.flush() {
                self.unwritten.push_back(line.to_owned());
                self.flush();
            } else if self.unwritten.len() < MAX_UNWRITTEN {
                self.unwritten.push_back(line.to_owned());
            } else {
                // Lines that keep failing are kept within reason: any
                // AppContainer can send them.
                tracing::warn!("a line refused: too many lines not yet written");
            }
        }

        /// Writes the lines waiting, oldest first; returns whether none is
        /// left.
        fn flush(&mut self) -> bool {
            let mut sink = FileSink::new(&self.custom);
            while let Some(line) = self.unwritten.front() {
                if let Err(error) = sink.append(line) {
                    tracing::warn!(%error, unwritten = self.unwritten.len(), "user custom dictionary not written");
                    return false;
                }
                self.unwritten.pop_front();
                tracing::info!("a line from a sandboxed text service written");
            }
            true
        }
    }

    /// Only one whole line the user custom dictionary reads: any
    /// AppContainer can reach the pipe, and must not write anything else.
    fn valid_line(bytes: &[u8]) -> Option<&str> {
        if bytes.len() > MAX_LINE {
            return None;
        }
        let line = std::str::from_utf8(bytes).ok()?.strip_suffix('\n')?;
        if line.contains(['\n', '\r']) || line.starts_with('#') {
            return None;
        }
        let (_, invalid) = TextDictionary::parse_user_custom(line);
        (invalid.is_empty() && !line.trim().is_empty()).then_some(line)
    }

    /// Other programs' requests on the port the settings name, answered
    /// from what the text services report over the control pipe.
    ///
    /// One thread keeps the record and answers, in the order the requests
    /// came; each process's connection is read and written on a thread of
    /// its own, and a field that does not take a mode in time is answered
    /// for, so no application holds the others up.
    mod control {
        use std::collections::HashMap;
        use std::fs::File;
        use std::io;
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use std::path::{Path, PathBuf};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
        use std::time::{Duration, Instant};

        use kanaemi::control::{ANSWER_WITHIN, Desk, MAX_LINE, Report, Step};
        use kanaemi::link::{self, Outbox};
        use kanaemi::pipe;
        use kanaemi_config::{FILE_NAME, Settings};
        use kanaemi_engine::FileStamp;
        use kanaemi_runtime::{ControlPort, ControlRequest};
        use windows::Win32::Foundation::*;
        use windows::Win32::Storage::FileSystem::*;
        use windows::Win32::System::IO::*;
        use windows::Win32::System::Pipes::*;
        use windows::Win32::System::SystemInformation::GetTickCount64;
        use windows::Win32::System::Threading::CreateEventW;
        use windows::core::*;

        /// How often the settings are looked at for the port they name.
        const SETTINGS_POLL: Duration = Duration::from_secs(1);
        /// How many processes may be connected at once; each holds a thread.
        const MAX_LINKS: usize = 256;
        /// How many reports and requests wait for the record's thread; a
        /// connection sending more waits for room.
        const MAX_JOBS: usize = 1024;

        enum Job {
            /// Requests came in on the port.
            Requests,
            Linked(u64, Arc<Outbox>),
            Report(u64, Report),
            Gone(u64),
        }

        pub(super) fn start(dir: PathBuf) {
            let spawned = std::thread::Builder::new()
                .name("control".to_owned())
                .spawn(move || serve(&dir));
            if let Err(error) = spawned {
                tracing::warn!(%error, "other programs' requests not served");
            }
        }

        fn serve(dir: &Path) {
            let (jobs, received) = mpsc::sync_channel(MAX_JOBS);
            let mut port = ControlPort::new({
                let jobs = jobs.clone();
                // A full queue is read before the next requests are taken.
                move || {
                    let _ = jobs.try_send(Job::Requests);
                }
            });
            let mut desk = Desk::<ControlRequest>::default();
            let mut links = HashMap::new();
            let mut settings = Watched::default();
            let mut piped = false;
            loop {
                if let Some(named) = settings.port_if_changed(dir) {
                    port.listen_on(named);
                    // Text services connect only while the pipe is there, so
                    // it is made once a port is named; it stays after.
                    if named.is_some() && !piped {
                        piped = true;
                        listen_on_pipe(jobs.clone());
                    }
                }
                let steps = next(&received, &mut desk, &mut links, settings.next_look);
                let Some(mut steps) = steps else {
                    return;
                };
                let now = Instant::now();
                steps.extend(desk.tick(now));
                while desk.has_room() {
                    let requests = port.take_requests();
                    if requests.is_empty() {
                        break;
                    }
                    for request in requests {
                        let to_set = request.mode_to_set();
                        steps.extend(desk.request(request, to_set, now));
                    }
                }
                for step in steps {
                    match step {
                        Step::Send(link, command) => {
                            // One that is not sent is answered for in time.
                            if let Some(outbox) = links.get(&link) {
                                let by =
                                    unsafe { GetTickCount64() } + ANSWER_WITHIN.as_millis() as u64;
                                outbox.send(command.encode(by));
                            }
                        }
                        Step::Answer(request, mode) => port.answer(request, mode),
                        Step::Tell(mode) => port.tell(mode),
                    }
                }
            }
        }

        /// Waits for the next job, until the request handed to a field is
        /// to be answered or the settings are to be looked at; `None` once
        /// no job can come.
        fn next(
            received: &Receiver<Job>,
            desk: &mut Desk<ControlRequest>,
            links: &mut HashMap<u64, Arc<Outbox>>,
            look_at: Instant,
        ) -> Option<Vec<Step<ControlRequest>>> {
            let until = desk.deadline().map_or(look_at, |by| by.min(look_at));
            let job = match received.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => return Some(Vec::new()),
                Err(RecvTimeoutError::Disconnected) => return None,
            };
            let now = Instant::now();
            Some(match job {
                Job::Requests => Vec::new(),
                Job::Linked(link, outbox) => {
                    links.insert(link, outbox);
                    Vec::new()
                }
                Job::Report(link, report) => desk.report(link, report, now),
                Job::Gone(link) => {
                    links.remove(&link);
                    desk.gone(link, now)
                }
            })
        }

        /// The settings file, read again when it changed, as a text service
        /// reads it.
        struct Watched {
            /// The stamp it was read at; `None` before the first read.
            read_at: Option<Option<FileStamp>>,
            next_look: Instant,
        }

        impl Default for Watched {
            fn default() -> Self {
                Self {
                    read_at: None,
                    next_look: Instant::now(),
                }
            }
        }

        impl Watched {
            /// The port the settings name, when it is time to look and they
            /// changed since read.
            fn port_if_changed(&mut self, dir: &Path) -> Option<Option<u16>> {
                if Instant::now() < self.next_look {
                    return None;
                }
                self.next_look = Instant::now() + SETTINGS_POLL;
                let path = dir.join(FILE_NAME);
                // Stamped before reading, so a change while reading is seen
                // next time.
                let stamp = FileStamp::of(&path);
                if self.read_at == Some(stamp) {
                    return None;
                }
                let text = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
                    // Read again at the next look.
                    Err(_) => return None,
                };
                self.read_at = Some(stamp);
                Some(Settings::load(text, dir).0.control_port)
            }
        }

        fn listen_on_pipe(jobs: SyncSender<Job>) {
            let spawned = std::thread::Builder::new()
                .name("control-pipe".to_owned())
                .spawn(move || {
                    if let Err(error) = accept(&jobs) {
                        tracing::warn!(%error, "control pipe not served");
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "control pipe not served");
            }
        }

        /// Lets each process's text services in, one connection each.
        fn accept(jobs: &SyncSender<Job>) -> io::Result<()> {
            // AppContainers also read what the server sends them; nothing
            // else is sent over the pipe than the modes set for their own
            // fields.
            let attributes = super::pipe_security(pipe::CONTROL_CLIENT_ACCESS)?;
            let name = HSTRING::from(pipe::control_name()?);
            let instance = |first: bool| {
                let mut mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
                if first {
                    // Fails when another program already made the pipe.
                    mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
                }
                let pipe = unsafe {
                    CreateNamedPipeW(
                        &name,
                        mode,
                        PIPE_TYPE_BYTE
                            | PIPE_READMODE_BYTE
                            | PIPE_WAIT
                            | PIPE_REJECT_REMOTE_CLIENTS,
                        PIPE_UNLIMITED_INSTANCES,
                        MAX_LINE as u32,
                        MAX_LINE as u32,
                        0,
                        Some(&attributes),
                    )
                };
                if pipe.is_invalid() {
                    return Err(io::Error::last_os_error());
                }
                Ok(unsafe { File::from_raw_handle(pipe.0) })
            };
            let connected = event()?;
            let open = Arc::new(AtomicUsize::new(0));
            let mut next = instance(true)?;
            tracing::info!("serving the control pipe");
            for id in 0u64.. {
                let pipe = next;
                let joined = connect(&pipe, &connected);
                // Another instance waits while this one is set up.
                next = instance(false)?;
                if !joined {
                    continue;
                }
                if open.load(Ordering::SeqCst) >= MAX_LINKS {
                    tracing::warn!("a control connection refused: too many");
                    continue;
                }
                serve_link(id, pipe, jobs.clone(), open.clone());
            }
            Ok(())
        }

        /// Waits for a client on `pipe`; returns whether one came.
        fn connect(pipe: &File, connected: &OwnedHandle) -> bool {
            let pipe = HANDLE(pipe.as_raw_handle());
            let mut overlapped = OVERLAPPED {
                hEvent: HANDLE(connected.as_raw_handle()),
                ..Default::default()
            };
            match unsafe { ConnectNamedPipe(pipe, Some(&mut overlapped)) } {
                Ok(()) => true,
                // A client may connect between the pipe's creation and the
                // call.
                Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => true,
                Err(error) if error.code() == ERROR_IO_PENDING.to_hresult() => {
                    unsafe { GetOverlappedResult(pipe, &overlapped, &mut 0, true) }.is_ok()
                }
                Err(_) => false,
            }
        }

        fn serve_link(id: u64, pipe: File, jobs: SyncSender<Job>, open: Arc<AtomicUsize>) {
            let outbox = match Outbox::new() {
                Ok(outbox) => Arc::new(outbox),
                Err(error) => {
                    tracing::warn!(%error, "control connection not served");
                    return;
                }
            };
            if jobs.send(Job::Linked(id, outbox.clone())).is_err() {
                return;
            }
            open.fetch_add(1, Ordering::SeqCst);
            let spawned = std::thread::Builder::new()
                .name("control-link".to_owned())
                .spawn({
                    let (jobs, open) = (jobs.clone(), open.clone());
                    move || {
                        link::run(pipe, &outbox, |line| match Report::decode(line) {
                            Some(report) => {
                                let report = report.heard_at(unsafe { GetTickCount64() });
                                jobs.send(Job::Report(id, report)).is_ok()
                            }
                            // Anything else ends the connection: any
                            // AppContainer can reach the pipe.
                            None => false,
                        });
                        let _ = jobs.send(Job::Gone(id));
                        open.fetch_sub(1, Ordering::SeqCst);
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "control connection not served");
                let _ = jobs.send(Job::Gone(id));
                open.fetch_sub(1, Ordering::SeqCst);
            }
        }

        fn event() -> io::Result<OwnedHandle> {
            let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }?;
            Ok(unsafe { OwnedHandle::from_raw_handle(event.0) })
        }
    }
}
