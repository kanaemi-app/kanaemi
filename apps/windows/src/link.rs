//! One connection over the control pipe, run on a thread of its own. Both
//! ends wait to read and write at once, which a pipe opened for synchronous
//! I/O lets through one call at a time, so it is opened for overlapped I/O.
//!
//! Neither end waits on the other for long: a line the other side does not
//! take in time ends the connection.

use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{ERROR_IO_PENDING, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects};
use windows::core::PCWSTR;

use crate::control::{Lines, MAX_LINE};

/// How many lines wait to be sent; one more ends the connection, as the
/// other side has stopped reading.
const MAX_UNSENT: usize = 64;
/// How long a line may take to be written.
const WRITE_WITHIN_MS: u32 = 1000;

/// The lines to send over one connection, from any thread.
pub struct Outbox {
    lines: Mutex<VecDeque<String>>,
    /// Wakes the connection's thread to send, or to end.
    ready: OwnedHandle,
    ended: AtomicBool,
}

impl Outbox {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            lines: Mutex::default(),
            ready: event(false)?,
            ended: AtomicBool::new(false),
        })
    }

    /// Hands `line` to the connection's thread; returns whether the
    /// connection goes on.
    pub fn send(&self, line: String) -> bool {
        if self.ended() {
            return false;
        }
        {
            let mut lines = self
                .lines
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if lines.len() >= MAX_UNSENT {
                drop(lines);
                self.end();
                return false;
            }
            lines.push_back(line);
        }
        let _ = unsafe { SetEvent(handle(&self.ready)) };
        true
    }

    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    /// Ends the connection: its thread lets the pipe go.
    pub fn end(&self) {
        self.ended.store(true, Ordering::SeqCst);
        let _ = unsafe { SetEvent(handle(&self.ready)) };
    }

    fn take(&self) -> VecDeque<String> {
        std::mem::take(
            &mut *self
                .lines
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }
}

/// Runs the connection over `pipe`, opened for overlapped I/O, until either
/// side ends it: sends what `outbox` is handed, and gives each line read to
/// `received`, which returns whether to go on.
pub fn run(pipe: File, outbox: &Outbox, mut received: impl FnMut(&str) -> bool) {
    if let Err(error) = serve(&pipe, outbox, &mut received) {
        tracing::debug!(%error, "control connection ended");
    }
    outbox.end();
}

fn serve(pipe: &File, outbox: &Outbox, received: &mut dyn FnMut(&str) -> bool) -> io::Result<()> {
    let pipe = HANDLE(pipe.as_raw_handle());
    let read_done = event(true)?;
    let written = event(true)?;
    let mut buffer = [0u8; MAX_LINE + 1];
    let mut lines = Lines::default();
    loop {
        let mut reading = OVERLAPPED {
            hEvent: handle(&read_done),
            ..Default::default()
        };
        if let Err(error) = unsafe { ReadFile(pipe, Some(&mut buffer), None, Some(&mut reading)) }
            && error.code() != ERROR_IO_PENDING.to_hresult()
        {
            return Err(error.into());
        }
        let sent = send_until_read(pipe, outbox, &read_done, &written);
        if !matches!(sent, Ok(true)) {
            // The read still refers to the buffer: it ends before the
            // buffer goes.
            unsafe {
                let _ = CancelIoEx(pipe, Some(&reading));
                let _ = GetOverlappedResult(pipe, &reading, &mut 0, true);
            }
            return sent.map(|_| ());
        }
        let mut read = 0u32;
        unsafe { GetOverlappedResult(pipe, &reading, &mut read, false) }?;
        let Some(arrived) = lines.push(&buffer[..read as usize]) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a line of the control pipe",
            ));
        };
        for line in arrived {
            if !received(&line) {
                return Ok(());
            }
        }
    }
}

/// Sends the lines handed to the outbox until the read under way is done;
/// returns whether it is, or `false` once the outbox ends.
fn send_until_read(
    pipe: HANDLE,
    outbox: &Outbox,
    read_done: &OwnedHandle,
    written: &OwnedHandle,
) -> io::Result<bool> {
    let woken_by = [handle(read_done), handle(&outbox.ready)];
    loop {
        let woke = unsafe { WaitForMultipleObjects(&woken_by, false, INFINITE) };
        if woke == WAIT_OBJECT_0 {
            return Ok(true);
        }
        if woke.0 != WAIT_OBJECT_0.0 + 1 {
            return Err(io::Error::last_os_error());
        }
        for line in outbox.take() {
            write(pipe, &format!("{line}\n"), written)?;
        }
        if outbox.ended() {
            return Ok(false);
        }
    }
}

fn write(pipe: HANDLE, line: &str, written: &OwnedHandle) -> io::Result<()> {
    let mut writing = OVERLAPPED {
        hEvent: handle(written),
        ..Default::default()
    };
    let started = unsafe { WriteFile(pipe, Some(line.as_bytes()), None, Some(&mut writing)) };
    if let Err(error) = started
        && error.code() != ERROR_IO_PENDING.to_hresult()
    {
        return Err(error.into());
    }
    let done = unsafe { GetOverlappedResultEx(pipe, &writing, &mut 0, WRITE_WITHIN_MS, false) };
    if let Err(error) = done {
        // The line is let go only once the write has ended.
        unsafe {
            let _ = CancelIoEx(pipe, Some(&writing));
            let _ = GetOverlappedResult(pipe, &writing, &mut 0, true);
        }
        return Err(error.into());
    }
    Ok(())
}

fn event(manual_reset: bool) -> io::Result<OwnedHandle> {
    let event = unsafe { CreateEventW(None, manual_reset, false, PCWSTR::null()) }?;
    Ok(unsafe { OwnedHandle::from_raw_handle(event.0) })
}

fn handle(owned: &OwnedHandle) -> HANDLE {
    HANDLE(owned.as_raw_handle())
}
