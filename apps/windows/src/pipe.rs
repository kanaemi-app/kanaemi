//! The named pipe a text service in an AppContainer hands dictionary lines
//! to, for the server running as the user to write: an AppContainer cannot
//! write the user's files.
//!
//! One line per connection, ended by a newline; the client closes after
//! writing and waits for no answer, so a server that hangs never stops
//! typing.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;

use kanaemi_engine::LineSink;
use windows::Win32::Foundation::{ERROR_PIPE_BUSY, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{
    GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, TOKEN_USER,
    TokenIsAppContainer, TokenUser,
};
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::core::{HSTRING, PWSTR};

/// How long a client waits for a server busy with another.
const BUSY_WAIT_MS: u32 = 200;

/// A line longer than this is not one the IME writes.
pub const MAX_LINE: usize = 64 * 1024;

/// `FILE_WRITE_DATA`, `FILE_READ_ATTRIBUTES`, `READ_CONTROL` and
/// `SYNCHRONIZE`: all a client needs, `READ_CONTROL` to see who made the
/// pipe. Plain write access includes `FILE_APPEND_DATA`, which on a pipe is
/// the right to create instances of it, and is not granted to AppContainers.
pub const CLIENT_ACCESS: u32 = 0x0012_0082;

/// [`CLIENT_ACCESS`] and `FILE_READ_DATA`: a text service also reads the
/// modes the server sets over the control pipe.
pub const CONTROL_CLIENT_ACCESS: u32 = CLIENT_ACCESS | 0x0000_0001;

/// The owners a pipe of this user may have: the user, or the administrators
/// when an elevated installer started the server. Pipe names are shared by
/// every user, so another could make one first to collect the words sent.
const ADMINISTRATORS: &str = "S-1-5-32-544";

/// `GetCurrentProcessToken()`, an inline pseudo-handle the bindings lack.
fn current_token() -> HANDLE {
    HANDLE(-4isize as *mut c_void)
}

/// Pipe names are shared by every session, so each user's has their SID.
pub fn name() -> io::Result<String> {
    Ok(format!(r"\\.\pipe\kanaemi-{}", user_sid()?))
}

/// The pipe text services tell the server of their fields over, and are
/// given the modes other programs set.
pub fn control_name() -> io::Result<String> {
    Ok(format!(r"\\.\pipe\kanaemi-{}-control", user_sid()?))
}

pub fn user_sid() -> io::Result<String> {
    let mut buffer = vec![0u8; 256];
    let mut len = 0u32;
    unsafe {
        GetTokenInformation(
            current_token(),
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            &mut len,
        )
    }
    .map_err(io::Error::other)?;
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_string(user.User.Sid)
}

fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.map_err(io::Error::other)?;
    let sid = unsafe { text.to_string() }.map_err(io::Error::other);
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    sid
}

/// Whether the pipe was made by this user, and so by their server.
pub(crate) fn made_by_user(pipe: &std::fs::File) -> io::Result<bool> {
    let mut owner = PSID::default();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let asked = unsafe {
        GetSecurityInfo(
            HANDLE(pipe.as_raw_handle()),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            Some(&mut descriptor),
        )
    };
    asked.ok().map_err(io::Error::other)?;
    let made = sid_string(owner)
        .map(|owner| owner == user_sid().unwrap_or_default() || owner == ADMINISTRATORS);
    unsafe {
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    made
}

/// Whether this process runs in an AppContainer.
pub fn in_app_container() -> bool {
    let mut is = 0u32;
    let mut len = 0u32;
    let asked = unsafe {
        GetTokenInformation(
            current_token(),
            TokenIsAppContainer,
            Some((&mut is as *mut u32).cast()),
            4,
            &mut len,
        )
    };
    asked.is_ok() && is != 0
}

/// Sends each line to the server.
pub struct PipeSink;

impl LineSink for PipeSink {
    fn append(&mut self, line: &str) -> io::Result<()> {
        // The server takes no longer line, and a line that does not fit in
        // the pipe's buffer would block typing until the server reads it.
        if line.len() >= MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the line is too long for the server",
            ));
        }
        let name = name()?;
        let open = || OpenOptions::new().access_mode(CLIENT_ACCESS).open(&name);
        let mut pipe = match open() {
            // The server is reading another client: wait a moment, not more,
            // as typing waits too.
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => {
                let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name.as_str()), BUSY_WAIT_MS) };
                open()?
            }
            opened => opened?,
        };
        if !made_by_user(&pipe)? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the pipe was made by another user",
            ));
        }
        pipe.write_all(format!("{line}\n").as_bytes())
    }
}
