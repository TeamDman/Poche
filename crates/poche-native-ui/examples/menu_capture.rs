//! Windowless inspection of the real main menu, without a network or fixture.
use poche_native_ui::{
    NativeRenderMode, NativeUiLaunchOptions,
    desktop_menu::{DesktopConnectionWorker, InvitationValidator},
};

fn main() -> Result<(), String> {
    let path = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or("pass an output PNG path")?;
    if path.exists() {
        return Err(
            "choose a fresh output path so stale images cannot count as evidence".to_owned(),
        );
    }
    let options = NativeUiLaunchOptions {
        render_mode: NativeRenderMode::WindowlessImage,
        screenshot: Some(path.clone()),
        exit_after_seconds: Some(3.),
        ..Default::default()
    };
    let worker =
        DesktopConnectionWorker::start(|_| Err("Capture harness has no network connection."))?;
    poche_native_ui::run_menu(options, worker, InvitationValidator(|_| false))?;
    if !path.is_file() {
        return Err("menu capture did not produce an image".to_owned());
    }
    let rendered = image::open(&path)
        .map_err(|_| "menu capture is not a readable image")?
        .to_rgba8();
    let first = rendered.get_pixel(0, 0);
    if rendered.pixels().all(|pixel| pixel == first) {
        return Err("menu capture is blank/uniform".to_owned());
    }
    Ok(())
}
