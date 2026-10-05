//! On Windows the program carries the logo as its icon, drawn from the SVG at
//! the sizes the shell asks for, and the name Task Manager shows.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        windows::embed_resources();
    }
}

#[cfg(windows)]
mod windows {
    use std::fs::File;
    use std::path::PathBuf;

    const LOGO: &str = "assets/logo/kanaemi-icon.svg";
    const RESOURCES: &str = "kanaemi-settings.rc";
    /// The sizes Windows picks from for title bars, the taskbar, the Start
    /// menu and Explorer's views at each display scale.
    const SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

    pub fn embed_resources() {
        println!("cargo:rerun-if-changed={LOGO}");
        println!("cargo:rerun-if-changed={RESOURCES}");
        let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
        let icon = out.join("kanaemi-settings.ico");
        write_icon(&icon);
        // rc reads the path as a C string, where a backslash escapes.
        let define = format!(
            "ICON_PATH=\"{}\"",
            icon.display().to_string().replace('\\', "/")
        );
        embed_resource::compile(RESOURCES, [define])
            .manifest_optional()
            .expect("resources compiled");
    }

    fn write_icon(path: &PathBuf) {
        let svg = std::fs::read(LOGO).expect("logo read");
        let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())
            .expect("logo parsed");
        let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
        for size in SIZES {
            let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("nonzero size");
            let scale = size as f32 / tree.size().width();
            let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
            resvg::render(&tree, transform, &mut pixmap.as_mut());
            // tiny-skia keeps colors multiplied by alpha; an icon keeps them
            // straight.
            let rgba = pixmap
                .pixels()
                .iter()
                .flat_map(|pixel| {
                    let color = pixel.demultiply();
                    [color.red(), color.green(), color.blue(), color.alpha()]
                })
                .collect();
            let image = ico::IconImage::from_rgba_data(size, size, rgba);
            dir.add_entry(ico::IconDirEntry::encode(&image).expect("icon encoded"));
        }
        dir.write(File::create(path).expect("icon created"))
            .expect("icon written");
    }
}
