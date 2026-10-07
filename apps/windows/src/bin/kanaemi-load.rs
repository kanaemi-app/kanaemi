//! Loads a DLL as COM loads a text service, and prints each named export with
//! the file it runs in, one per line:
//!
//!     kanaemi-load <dll> <export>...
//!
//! The packaging checks with it that the ARM64X kanaemi.dll hands each kind
//! of process the DLL built for it.

#[cfg(not(windows))]
fn main() {
    eprintln!("kanaemi-load runs only on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    use std::ffi::CString;

    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::*;
    use windows::core::{HSTRING, PCSTR, PCWSTR};

    let mut args = std::env::args().skip(1);
    let Some(dll) = args.next() else {
        fail("usage: kanaemi-load <dll> <export>...");
    };
    // COM loads an in-process server by its full path, looking for what the
    // server needs in the server's folder.
    let module = unsafe {
        LoadLibraryExW(
            &HSTRING::from(dll.as_str()),
            None,
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    }
    .unwrap_or_else(|error| fail(&format!("{dll}: {error}")));
    for name in args {
        let symbol = CString::new(name.as_str()).unwrap_or_else(|_| fail(&name));
        let Some(export) = (unsafe { GetProcAddress(module, PCSTR(symbol.as_ptr().cast())) })
        else {
            fail(&format!("{name}: not exported"));
        };
        let mut owner = HMODULE::default();
        unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                PCWSTR(export as *const u16),
                &mut owner,
            )
        }
        .unwrap_or_else(|error| fail(&format!("{name}: {error}")));
        let mut path = vec![0u16; 1024];
        let len = unsafe { GetModuleFileNameW(Some(owner), &mut path) };
        println!("{name} {}", String::from_utf16_lossy(&path[..len as usize]));
    }
}

#[cfg(windows)]
fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
