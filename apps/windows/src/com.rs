//! What makes the DLL a COM server TSF can find: the class factory, the
//! exports COM calls, and the registration of the text service.

#![allow(non_snake_case)]

use std::ffi::{OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

use crate::registration::registered_dll;
use crate::tip::TextService;

pub const CLSID_KANAEMI: GUID = GUID::from_u128(0x04448b65_d29f_4f36_84c7_5fdba91ef4b9);
pub const PROFILE_KANAEMI: GUID = GUID::from_u128(0x33dc0029_2833_47af_b58e_3507230dc30a);
/// What the candidate list is, to an application that draws it itself.
pub const CANDIDATE_LIST_ELEMENT: GUID = GUID::from_u128(0x1915853b_4879_4e18_b9b3_465bbd7da083);
const LANGID_JAPANESE: u16 = 0x0411;
/// What the text service is: a keyboard, and one that works in immersive
/// applications and shows in the system tray.
const CATEGORIES: [GUID; 3] = [
    GUID_TFCAT_TIP_KEYBOARD,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
];
const NAME: &str = "かなえみ";

static MODULE: AtomicIsize = AtomicIsize::new(0);

/// The DLL's own module, which owns the window classes it registers.
pub fn instance() -> HINSTANCE {
    HINSTANCE(MODULE.load(Ordering::Relaxed) as *mut c_void)
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let unknown: IUnknown = TextService::new().into();
        unsafe { unknown.query(riid, ppv).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[unsafe(no_mangle)]
extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(module.0 as isize, Ordering::Relaxed);
    }
    TRUE
}

#[unsafe(no_mangle)]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if unsafe { *rclsid } != CLSID_KANAEMI {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory.into();
    unsafe { factory.query(riid, ppv) }
}

/// The DLL stays loaded: an application may activate the text service
/// again at any time, and its state lives in thread-locals.
#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

#[unsafe(no_mangle)]
extern "system" fn DllRegisterServer() -> HRESULT {
    match register() {
        Ok(()) => S_OK,
        Err(error) => error.code(),
    }
}

#[unsafe(no_mangle)]
extern "system" fn DllUnregisterServer() -> HRESULT {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(categories) =
            CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
        {
            for category in CATEGORIES {
                let _ = categories.UnregisterCategory(&CLSID_KANAEMI, &category, &CLSID_KANAEMI);
            }
        }
        // Takes the profiles with it, and what is left of the text service
        // under the TIP key.
        if let Ok(profiles) = CoCreateInstance::<_, ITfInputProcessorProfiles>(
            &CLSID_TF_InputProcessorProfiles,
            None,
            CLSCTX_INPROC_SERVER,
        ) {
            let _ = profiles.Unregister(&CLSID_KANAEMI);
        }
        let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, &HSTRING::from(clsid_key()));
    }
    S_OK
}

fn clsid_key() -> String {
    format!(r"Software\Classes\CLSID\{{{CLSID_KANAEMI:?}}}")
}

/// Where the DLL itself is, to find what is installed beside it.
pub(crate) fn module_path() -> Option<std::path::PathBuf> {
    let wide = module_wide_path();
    (!wide.is_empty()).then(|| std::path::PathBuf::from(String::from_utf16_lossy(&wide)))
}

fn module_wide_path() -> Vec<u16> {
    let mut buffer = vec![0u16; 1024];
    let module = HMODULE(MODULE.load(Ordering::Relaxed) as *mut c_void);
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buffer) };
    buffer.truncate(len as usize);
    buffer
}

fn set_value(key: HKEY, name: PCWSTR, value: &[u16]) -> Result<()> {
    let mut data = value.to_vec();
    data.push(0);
    let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
    unsafe { RegSetValueExW(key, name, None, REG_SZ, Some(bytes)).ok() }
}

fn register() -> Result<()> {
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(format!(r"{}\InprocServer32", clsid_key())),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .ok()?;
    }
    let dll: Vec<u16> = {
        let module = PathBuf::from(OsString::from_wide(&module_wide_path()));
        registered_dll(&module).as_os_str().encode_wide().collect()
    };
    let written = set_value(key, PCWSTR::null(), &dll).and_then(|()| {
        let model: Vec<u16> = "Apartment".encode_utf16().collect();
        set_value(key, w!("ThreadingModel"), &model)
    });
    unsafe {
        let _ = RegCloseKey(key);
    }
    written?;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let profiles: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        let name: Vec<u16> = NAME.encode_utf16().collect();
        profiles.RegisterProfile(
            &CLSID_KANAEMI,
            LANGID_JAPANESE,
            &PROFILE_KANAEMI,
            &name,
            &[],
            0,
            HKL::default(),
            0,
            true,
            0,
        )?;
        let categories: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        for category in CATEGORIES {
            categories.RegisterCategory(&CLSID_KANAEMI, &category, &CLSID_KANAEMI)?;
        }
    }
    Ok(())
}
