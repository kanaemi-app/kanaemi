//! Drives Kanaemi end to end: makes it the input method, types into Notepad
//! as a user would, and writes what Notepad holds to
//! `C:\Users\Public\kanaemi_drive.txt`.
//!
//!     kanaemi-drive <key>...
//!
//! A key is `R` or `L` (the right or left Shift alone), `SPC`, `RET`, `ESC`,
//! `BS`, `WIN` (opens the Start menu, whose search box runs in an
//! AppContainer), `SERVER` (starts the server that writes for it), `INDICATOR`
//! and `CANDIDATES` (report whether that window of Kanaemi shows), `WINDOWS`
//! (where each candidate window is and what owns it), `SIBLINGS` (the
//! top-level windows of the process showing candidates, with their z-order
//! bands), `PAUSE` (four seconds, for a screenshot), or text to type (each
//! character with the keys the keyboard layout types it with).
//!
//! It has to run on the interactive desktop, where SendInput reaches the
//! windows on the screen; a remote shell has none. With vitro:
//!
//!     vitro launch <vm> 'C:\…\kanaemi-drive.exe' R ';kanji' SPC RET

#![cfg_attr(not(windows), allow(unused))]

#[cfg(not(windows))]
fn main() {
    eprintln!("kanaemi-drive runs only on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    drive::main();
}

#[cfg(windows)]
mod drive {
    use std::fs;
    use std::thread::sleep;
    use std::time::Duration;

    use windows::Win32::Foundation::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::TextServices::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::*;

    use kanaemi::com::{CLSID_KANAEMI as CLSID, PROFILE_KANAEMI as PROFILE};
    const OUT: &str = r"C:\Users\Public\kanaemi_drive.txt";

    fn key(vk: VIRTUAL_KEY, scan: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) {
        for i in inputs {
            unsafe { SendInput(std::slice::from_ref(i), std::mem::size_of::<INPUT>() as i32) };
            sleep(Duration::from_millis(40));
        }
    }

    fn tap(vk: VIRTUAL_KEY) {
        send(&[key(vk, 0, false), key(vk, 0, true)]);
    }

    fn chord(modifier: VIRTUAL_KEY, vk: VIRTUAL_KEY) {
        send(&[
            key(modifier, 0x2a, false),
            key(vk, 0, false),
            key(vk, 0, true),
            key(modifier, 0x2a, true),
        ]);
    }

    /// The text of Notepad's editor, read from the control itself: Kanaemi's
    /// default bindings turn Ctrl+A into Home, so select-all and copy would not.
    fn editor(notepad: HWND) -> Option<HWND> {
        for class in [w!("RichEditD2DPT"), w!("Edit")] {
            if let Ok(h) = unsafe { FindWindowExW(Some(notepad), None, class, PCWSTR::null()) } {
                return Some(h);
            }
            // Windows 11 Notepad nests the editor one level down.
            let mut child =
                unsafe { FindWindowExW(Some(notepad), None, PCWSTR::null(), PCWSTR::null()) }.ok();
            while let Some(c) = child {
                if let Ok(h) = unsafe { FindWindowExW(Some(c), None, class, PCWSTR::null()) } {
                    return Some(h);
                }
                child = unsafe {
                    FindWindowExW(Some(notepad), Some(c), PCWSTR::null(), PCWSTR::null())
                }
                .ok();
            }
        }
        None
    }

    fn text_of(editor: HWND) -> String {
        let mut buffer = vec![0u16; 4096];
        let n = unsafe {
            SendMessageW(
                editor,
                WM_GETTEXT,
                Some(WPARAM(buffer.len())),
                Some(LPARAM(buffer.as_mut_ptr() as isize)),
            )
        };
        String::from_utf16_lossy(&buffer[..n.0 as usize])
    }

    fn clear(editor: HWND) {
        let empty = [0u16];
        unsafe {
            SendMessageW(
                editor,
                WM_SETTEXT,
                None,
                Some(LPARAM(empty.as_ptr() as isize)),
            )
        };
    }

    pub fn main() {
        let mut report = Vec::new();
        if let Err(e) = run(&mut report) {
            report.push(format!("error: {e:?}"));
        }
        let _ = fs::write(OUT, report.join("\r\n"));
    }

    /// Every window of `class`: its process, whether it shows, where, and its
    /// owner's process.
    fn describe(class: PCWSTR) -> Vec<String> {
        let process = |window: HWND| {
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
            let name = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!("(Get-Process -Id {pid}).ProcessName"),
                ])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .unwrap_or_default();
            format!("{name}({pid})")
        };
        let mut out = Vec::new();
        let mut after = None;
        while let Ok(window) = unsafe { FindWindowExW(None, after, class, PCWSTR::null()) } {
            let mut rect = RECT::default();
            unsafe {
                let _ = GetWindowRect(window, &mut rect);
            }
            let owner = unsafe { GetWindow(window, GW_OWNER) }.ok();
            out.push(format!(
                "window {:?} in {} visible={} rect=({},{})-({},{}) owner={}",
                window.0,
                process(window),
                unsafe { IsWindowVisible(window) }.as_bool(),
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                owner.map_or("none".to_owned(), process),
            ));
            after = Some(window);
        }
        out
    }

    /// Whether a window of Kanaemi's `class` shows, in whichever application.
    fn visible(class: PCWSTR) -> bool {
        let mut after = None;
        while let Ok(window) = unsafe { FindWindowExW(None, after, class, PCWSTR::null()) } {
            if unsafe { IsWindowVisible(window) }.as_bool() {
                return true;
            }
            after = Some(window);
        }
        false
    }

    /// The z-order band `window` shows in, by an undocumented call that no
    /// import library lists.
    fn window_band(window: HWND) -> u32 {
        type GetWindowBand = unsafe extern "system" fn(HWND, *mut u32) -> BOOL;
        let mut band = 0;
        unsafe {
            let Ok(user32) =
                windows::Win32::System::LibraryLoader::GetModuleHandleW(w!("user32.dll"))
            else {
                return 0;
            };
            if let Some(found) =
                windows::Win32::System::LibraryLoader::GetProcAddress(user32, s!("GetWindowBand"))
            {
                let get: GetWindowBand = std::mem::transmute(found);
                let _ = get(window, &mut band);
            }
        }
        band
    }

    /// The top-level windows of the process that has a window of `class`,
    /// with their thread, class, band and place, to see what could own it.
    fn siblings(class: PCWSTR) -> Vec<String> {
        let Ok(ours) = (unsafe { FindWindowExW(None, None, class, PCWSTR::null()) }) else {
            return vec!["no window".into()];
        };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(ours, Some(&mut pid)) };
        let mut out = Vec::new();
        let mut after = None;
        while let Ok(window) = unsafe { FindWindowExW(None, after, PCWSTR::null(), PCWSTR::null()) }
        {
            after = Some(window);
            let mut owner_pid = 0;
            let thread = unsafe { GetWindowThreadProcessId(window, Some(&mut owner_pid)) };
            if owner_pid != pid {
                continue;
            }
            let mut name = [0u16; 128];
            let len = unsafe { GetClassNameW(window, &mut name) } as usize;
            let band = window_band(window);
            let mut rect = RECT::default();
            unsafe {
                let _ = GetWindowRect(window, &mut rect);
            }
            out.push(format!(
                "{:?} thread={thread} class={} band={band} visible={} rect=({},{})-({},{})",
                window.0,
                String::from_utf16_lossy(&name[..len]),
                unsafe { IsWindowVisible(window) }.as_bool(),
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
            ));
        }
        out
    }

    fn notepad_windows() -> Vec<HWND> {
        let mut found = Vec::new();
        while let Ok(window) =
            unsafe { FindWindowExW(None, found.last().copied(), w!("Notepad"), PCWSTR::null()) }
        {
            found.push(window);
        }
        found
    }

    /// A Notepad window showing a file of the run's own. Any other text,
    /// even in a window just opened, may be a draft Notepad restored for the
    /// user, and is never cleared.
    fn notepad_window() -> Result<HWND> {
        let name = format!("kanaemi-drive-{}.txt", std::process::id());
        let file = std::env::temp_dir().join(&name);
        fs::write(&file, "").map_err(|_| Error::from(E_FAIL))?;
        std::process::Command::new("notepad.exe")
            .arg(&file)
            .spawn()
            .map_err(|_| Error::from(E_FAIL))?;
        for _ in 0..50 {
            sleep(Duration::from_millis(200));
            let ours = notepad_windows().into_iter().find(|&window| {
                let mut title = [0u16; 512];
                let len = unsafe { GetWindowTextW(window, &mut title) } as usize;
                String::from_utf16_lossy(&title[..len]).starts_with(&name)
            });
            if let Some(window) = ours {
                return Ok(window);
            }
        }
        Err(Error::new(
            E_FAIL,
            "no Notepad window showing the run's file",
        ))
    }

    fn run(report: &mut Vec<String>) -> Result<()> {
        let keys: Vec<String> = std::env::args().skip(1).collect();
        let found;
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            // Without Notepad the keys still go, as to the Start menu, but
            // there is no text to read back.
            found = match notepad_window() {
                Ok(notepad) => {
                    let _ = ShowWindow(notepad, SW_RESTORE);
                    let fg = SetForegroundWindow(notepad).as_bool();
                    report.push(format!("foreground set: {fg}"));
                    // A Notepad just started may take a moment to show its editor.
                    (0..25).find_map(|_| {
                        sleep(Duration::from_millis(200));
                        editor(notepad)
                    })
                }
                Err(error) => {
                    report.push(format!("notepad: {error}"));
                    None
                }
            };
            match found {
                Some(ed) => clear(ed),
                None => report.push("no editor".into()),
            }
            let mgr: ITfInputProcessorProfileMgr =
                CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
            let r = mgr.ActivateProfile(
                TF_PROFILETYPE_INPUTPROCESSOR,
                0x0411,
                &CLSID,
                &PROFILE,
                HKL::default(),
                TF_IPPMF_FORSESSION
                    | TF_IPPMF_ENABLEPROFILE
                    | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
            );
            report.push(format!("activate profile: {r:?}"));
            sleep(Duration::from_millis(800));
        }
        for k in &keys {
            match k.as_str() {
                "R" => send(&[key(VK_RSHIFT, 0x36, false), key(VK_RSHIFT, 0x36, true)]),
                "L" => send(&[key(VK_LSHIFT, 0x2a, false), key(VK_LSHIFT, 0x2a, true)]),
                "SPC" => tap(VK_SPACE),
                "RET" => tap(VK_RETURN),
                "ESC" => tap(VK_ESCAPE),
                "BS" => tap(VK_BACK),
                "PAUSE" => sleep(Duration::from_secs(4)),
                "SERVER" => {
                    // Started from the desktop, as it is at sign-in; it leaves when
                    // another already runs.
                    let server =
                        std::path::Path::new(r"C:\Program Files\Kanaemi\kanaemi-server.exe");
                    if let Err(e) = std::process::Command::new(server).spawn() {
                        report.push(format!("server: {e}"));
                    }
                    sleep(Duration::from_secs(1));
                }
                "INDICATOR" => report.push(format!(
                    "indicator: {}",
                    visible(w!("KanaemiModeIndicator"))
                )),
                "CANDIDATES" => {
                    report.push(format!("candidates: {}", visible(w!("KanaemiCandidates"))))
                }
                "WINDOWS" => report.extend(describe(w!("KanaemiCandidates"))),
                "SIBLINGS" => report.extend(siblings(w!("KanaemiCandidates"))),
                "WIN" => {
                    tap(VK_LWIN);
                    sleep(Duration::from_secs(2));
                }
                text => {
                    for c in text.encode_utf16() {
                        // The key in the low byte, Shift as the high byte's bit 0;
                        // -1 when the layout has no key for it.
                        let scanned = unsafe { VkKeyScanW(c) };
                        if scanned == -1 {
                            report.push(format!("no key types {:?}", char::from_u32(c.into())));
                            continue;
                        }
                        let vk = VIRTUAL_KEY((scanned & 0xff) as u16);
                        if scanned & 0x100 != 0 {
                            chord(VK_SHIFT, vk);
                        } else {
                            tap(vk);
                        }
                    }
                }
            }
            sleep(Duration::from_millis(150));
        }
        sleep(Duration::from_millis(500));
        if let Some(ed) = found {
            report.push(format!("text: [{}]", text_of(ed)));
            clear(ed);
        }
        Ok(())
    }
}
