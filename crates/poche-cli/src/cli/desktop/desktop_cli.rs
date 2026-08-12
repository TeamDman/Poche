use facet::Facet;
use figue as args;
use poche_native_ui::NativeUiLaunchOptions;

/// Native graphical-client launch options retained by the unified executable.
#[derive(Facet, Default, PartialEq)]
#[facet(rename_all = "kebab-case")]
pub struct DesktopArgs {
    #[facet(args::named, default)]
    pub debug_overlay: bool,
    #[facet(args::named)]
    pub play_card: Option<String>,
    #[facet(args::named)]
    pub screenshot: Option<String>,
    #[facet(args::named)]
    pub acceptance_report: Option<String>,
    /// Persist one real render-target capture through the shared artifact pipeline.
    #[facet(args::named)]
    pub capture_artifact_root: Option<String>,
    #[facet(args::named)]
    pub exit_after_seconds: Option<f64>,
}

impl DesktopArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "launch"
    }

    /// Launch the Bevy leaf adapter without reparsing process arguments.
    ///
    /// # Errors
    ///
    /// Returns a launch-option, fixture, or renderer startup failure.
    pub fn invoke(self) -> eyre::Result<bool> {
        let options = NativeUiLaunchOptions {
            play_card: self.play_card,
            screenshot: self.screenshot.map(Into::into),
            acceptance_report: self.acceptance_report.map(Into::into),
            exit_after_seconds: self.exit_after_seconds,
            debug_overlay: self.debug_overlay,
            hidden_window: false,
            capture_provider: None,
            capture_context: None,
            external_tracing: true,
        };
        if let Some(root) = self.capture_artifact_root {
            let persisted = poche_native_ui::run_fixture_capture_acceptance(options, root)
                .map_err(|error| eyre::eyre!(error))?;
            println!("{}", persisted.manifest_path.display());
        } else {
            poche_native_ui::run(options).map_err(|error| eyre::eyre!(error))?;
        }
        Ok(true)
    }
}
