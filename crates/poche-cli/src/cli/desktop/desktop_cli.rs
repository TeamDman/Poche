use facet::Facet;
use figue as args;
use poche_native_ui::{NativeLiveDevice, NativeRenderMode, NativeUiLaunchOptions};

use crate::cli::live_device::LiveDeviceConfig;

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
    /// Join an existing certified room as the selected global device profile.
    #[facet(args::named)]
    pub room: Option<String>,
    /// Fresh per-instance root for opt-in incremental developer control.
    #[cfg(feature = "dev-control")]
    #[facet(args::named)]
    pub dev_control_root: Option<String>,
    /// Stable label used to select this developer-controlled instance.
    #[cfg(feature = "dev-control")]
    #[facet(args::named)]
    pub dev_control_instance: Option<String>,
    /// Render the developer-controlled instance to an image without an OS window.
    #[cfg(feature = "dev-control")]
    #[facet(args::named, default)]
    pub dev_control_windowless: bool,
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
    pub fn invoke(self, live_config: &LiveDeviceConfig) -> eyre::Result<bool> {
        let dev_control_windowless = self.dev_control_windowless();
        let options = NativeUiLaunchOptions {
            play_card: self.play_card,
            screenshot: self.screenshot.map(Into::into),
            acceptance_report: self.acceptance_report.map(Into::into),
            exit_after_seconds: self.exit_after_seconds,
            debug_overlay: self.debug_overlay,
            render_mode: if dev_control_windowless {
                NativeRenderMode::WindowlessImage
            } else {
                NativeRenderMode::InteractiveWindow
            },
            capture_provider: None,
            capture_context: None,
            external_tracing: true,
        };
        if self.room.is_some() && self.capture_artifact_root.is_some() {
            return Err(eyre::eyre!(
                "--room and --capture-artifact-root cannot be combined"
            ));
        }
        if let Some(room) = self.room {
            let (client, room_id) = live_config.existing_room_client(&room)?;
            let live =
                NativeLiveDevice::connect(client, room_id).map_err(|error| eyre::eyre!(error))?;
            poche_native_ui::run_live(options, live).map_err(|error| eyre::eyre!(error))?;
        } else if let Some(root) = self.capture_artifact_root {
            let persisted = poche_native_ui::run_fixture_capture_acceptance(options, root)
                .map_err(|error| eyre::eyre!(error))?;
            println!("{}", persisted.manifest_path.display());
        } else {
            if options.play_card.is_some()
                || options.screenshot.is_some()
                || options.acceptance_report.is_some()
                || options.exit_after_seconds.is_some()
            {
                poche_native_ui::run(options).map_err(|error| eyre::eyre!(error))?;
            } else {
                let worker = super::connection::worker().map_err(|error| eyre::eyre!(error))?;
                #[cfg(feature = "dev-control")]
                if let Some(root) = self.dev_control_root {
                    poche_native_ui::run_menu_with_file_control(
                        options,
                        worker,
                        super::connection::validator(),
                        poche_native_ui::desktop_menu::live_control::FileControlOptions {
                            root: root.into(),
                            instance_id: self
                                .dev_control_instance
                                .expect("validated developer-control instance"),
                        },
                    )
                    .map_err(|error| eyre::eyre!(error))?;
                } else {
                    poche_native_ui::run_menu(options, worker, super::connection::validator())
                        .map_err(|error| eyre::eyre!(error))?;
                }
                #[cfg(not(feature = "dev-control"))]
                poche_native_ui::run_menu(options, worker, super::connection::validator())
                    .map_err(|error| eyre::eyre!(error))?;
            }
        }
        Ok(true)
    }

    const fn dev_control_windowless(&self) -> bool {
        #[cfg(feature = "dev-control")]
        {
            self.dev_control_windowless
        }
        #[cfg(not(feature = "dev-control"))]
        {
            false
        }
    }
}
