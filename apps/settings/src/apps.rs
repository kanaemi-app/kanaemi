//! Applications as the OS shows them, to pick one by its name and icon
//! rather than write the name the IME knows it by.

/// An application, by the name the IME knows it by.
#[derive(Clone, Debug, PartialEq)]
pub struct App {
    /// As the settings file writes it: a bundle identifier on macOS.
    pub id: String,
    /// As the OS shows it; the id when the OS knows of no such application.
    pub name: String,
    /// A `data:` URL of the icon, when there is one.
    pub icon: Option<String>,
}

/// Whether applications can be picked here, or are written by name.
pub const PICKABLE: bool = cfg!(target_os = "macos");

/// The applications running that show in the Dock, each once, by name.
/// Kanaemi's own, such as this one, take no keys to leave alone.
pub fn running() -> Vec<App> {
    let mut apps = by_name(imp::running());
    apps.retain(|app| !app.id.starts_with(OWN_PREFIX));
    apps
}

const OWN_PREFIX: &str = "io.github.kanaemi-app.";

/// `id` as the OS shows it.
pub fn describe(id: &str) -> App {
    imp::describe(id).unwrap_or_else(|| App {
        id: id.to_owned(),
        name: id.to_owned(),
        icon: None,
    })
}

/// Asks for an application among those installed.
pub async fn choose() -> Option<App> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("キーを置き換えないアプリ")
        .set_directory("/Applications")
        .add_filter("アプリ", &["app"])
        .pick_file()
        .await?;
    imp::at(file.path())
}

/// Sorted by name, each id once whatever its case.
fn by_name(mut apps: Vec<App>) -> Vec<App> {
    apps.sort_by_cached_key(|app| (app.name.to_lowercase(), app.id.to_lowercase()));
    let mut seen = Vec::<String>::new();
    apps.retain(|app| {
        let id = app.id.to_lowercase();
        let new = !seen.contains(&id);
        seen.push(id);
        new
    });
    apps
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::Path;

    use base64::Engine;
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSApplicationActivationPolicy, NSBitmapImageFileType, NSBitmapImageRep, NSImage,
        NSWorkspace,
    };
    use objc2_foundation::{NSBundle, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL};

    use super::App;

    /// Drawn at twice the size the page shows it, for Retina screens.
    const ICON_SIDE: f64 = 64.0;

    pub fn running() -> Vec<App> {
        let workspace = NSWorkspace::sharedWorkspace();
        workspace
            .runningApplications()
            .iter()
            .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .filter_map(|app| {
                let id = app.bundleIdentifier()?.to_string();
                let name = app
                    .localizedName()
                    .map_or_else(|| id.clone(), |name| name.to_string());
                Some(App {
                    icon: app.icon().as_deref().and_then(png_url),
                    id,
                    name,
                })
            })
            .collect()
    }

    pub fn describe(id: &str) -> Option<App> {
        let url = NSWorkspace::sharedWorkspace()
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(id))?;
        let app = of_bundle(&url)?;
        // The id as written, whatever case the bundle writes it in.
        Some(App {
            id: id.to_owned(),
            ..app
        })
    }

    pub fn at(path: &Path) -> Option<App> {
        let url = NSURL::from_file_path(path)?;
        of_bundle(&url)
    }

    fn of_bundle(url: &NSURL) -> Option<App> {
        let bundle = NSBundle::bundleWithURL(url)?;
        let id = bundle.bundleIdentifier()?.to_string();
        let info = |key: &str| {
            bundle
                .objectForInfoDictionaryKey(&NSString::from_str(key))
                .and_then(|value| value.downcast::<NSString>().ok())
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
        };
        let stem = || {
            url.to_file_path()
                .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        };
        let name = info("CFBundleDisplayName")
            .or_else(|| info("CFBundleName"))
            .or_else(stem)
            .unwrap_or_else(|| id.clone());
        let path = url.path()?;
        let icon = png_url(&NSWorkspace::sharedWorkspace().iconForFile(&path));
        Some(App { id, name, icon })
    }

    fn png_url(image: &NSImage) -> Option<String> {
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(ICON_SIDE, ICON_SIDE));
        // SAFETY: `rect` lives through the call; no hints are given.
        let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
        let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
        // SAFETY: an empty dictionary is of any type.
        let png = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png.to_vec());
        Some(format!("data:image/png;base64,{encoded}"))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;

    use super::App;

    pub fn running() -> Vec<App> {
        Vec::new()
    }

    pub fn describe(_: &str) -> Option<App> {
        None
    }

    pub fn at(_: &Path) -> Option<App> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: None,
        }
    }

    #[test]
    fn applications_come_by_name_each_once() {
        let apps = by_name(vec![
            app("com.mitchellh.ghostty", "Ghostty"),
            app("com.apple.Safari", "Safari"),
            app("com.apple.finder", "finder"),
            app("com.mitchellh.Ghostty", "Ghostty"),
        ]);
        let ids: Vec<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "com.apple.finder",
                "com.mitchellh.ghostty",
                "com.apple.Safari"
            ]
        );
    }

    #[test]
    fn an_application_the_os_does_not_know_shows_its_id() {
        let app = describe("org.example.not-installed");
        assert_eq!(
            (app.id.as_str(), app.name.as_str(), app.icon),
            (
                "org.example.not-installed",
                "org.example.not-installed",
                None
            )
        );
    }
}
