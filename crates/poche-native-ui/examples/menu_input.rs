//! Opt-in GPU/menu input acceptance; no visible window, OS clipboard or network.
#[cfg(feature = "input-probe")]
fn main() -> Result<(), String> {
    use poche_native_ui::desktop_menu::{
        DesktopConnectionWorker, DesktopMenuRequest, InvitationValidator,
        input_probe::{self, IsolatedClipboard, MenuScenario},
    };
    let directory = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or("pass a fresh evidence directory")?;
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = DesktopConnectionWorker::start(move |request| {
        tx.send(request).unwrap();
        Err("Network unavailable. Try again.")
    })?;
    let outcome = input_probe::run(
        worker,
        InvitationValidator(|text| text == "valid-isolated-invitation"),
        "Menu Pilot",
        MenuScenario::ConnectionFailure {
            invitation: "valid-isolated-invitation".to_owned(),
            error: "Network unavailable. Try again.",
        },
        IsolatedClipboard::default(),
        &directory,
    )?;
    assert!(outcome.is_none());
    let requests: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(requests.as_slice(), [DesktopMenuRequest::Join { name, invitation }, DesktopMenuRequest::Create { name: second }]
        if name == "Menu Pilot" && second == name && invitation == "valid-isolated-invitation")
    );
    println!(
        "Rendered menu: keyboard name, invalid clipboard refusal, valid prefill, explicit Join, visible failure and Create retry passed."
    );
    Ok(())
}

#[cfg(not(feature = "input-probe"))]
fn main() {
    panic!("enable input-probe to run the menu input harness");
}
