//! Inspect the native tabletop and shared-world hand viewport without a window.
use poche_native_ui::{NativeRenderMode, NativeUiLaunchOptions};

fn main() -> Result<(), String> {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from)
        .ok_or("pass a fresh output PNG path")?;
    if path.exists() { return Err("output already exists".to_owned()); }
    poche_native_ui::run(NativeUiLaunchOptions {
        render_mode: NativeRenderMode::WindowlessImage,
        screenshot: Some(path.clone()),
        exit_after_seconds: Some(3.0),
        ..Default::default()
    })?;
    let image = image::open(path).map_err(|error| error.to_string())?.to_rgba8();
    if image.pixels().all(|pixel| pixel == image.get_pixel(0, 0)) {
        return Err("uniform capture".to_owned());
    }
    Ok(())
}
