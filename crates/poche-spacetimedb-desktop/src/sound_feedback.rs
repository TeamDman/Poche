// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Restrained local gesture feedback, never replayed from network pose echoes.
use super::{
    DragState, RenderSurface, UiScreen, UiState, animate_and_place_cards, money::MoneyState,
};
use bevy::{audio::Volume, prelude::*};

pub(super) struct SoundFeedbackPlugin;
impl Plugin for SoundFeedbackPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Assets<AudioSource>>() {
            app.init_asset::<AudioSource>();
        }
        app.init_resource::<SoundFeedback>()
            .add_systems(Startup, setup_sounds)
            .add_systems(Update, gesture_sounds.after(animate_and_place_cards));
    }
}

#[derive(Resource)]
pub(super) struct SoundFeedback {
    pub enabled: bool,
    pub pickup_count: u64,
    pub release_count: u64,
    pub audible_count: u64,
    previous: Option<HeldPiece>,
}
impl Default for SoundFeedback {
    fn default() -> Self {
        Self {
            enabled: true,
            pickup_count: 0,
            release_count: 0,
            audible_count: 0,
            previous: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct HeldPiece {
    key: String,
    coin: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cue {
    coin: bool,
    pickup: bool,
}

impl SoundFeedback {
    fn observe(&mut self, held: Option<HeldPiece>, in_table: bool) -> Vec<Cue> {
        if !in_table {
            self.previous = None;
            return vec![];
        }
        if held == self.previous {
            return vec![];
        }
        let mut cues = Vec::with_capacity(2);
        if let Some(previous) = self.previous.take() {
            self.release_count += 1;
            cues.push(Cue {
                coin: previous.coin,
                pickup: false,
            });
        }
        if let Some(current) = held.as_ref() {
            self.pickup_count += 1;
            cues.push(Cue {
                coin: current.coin,
                pickup: true,
            });
        }
        self.previous = held;
        cues
    }
}

#[derive(Resource)]
struct Sounds {
    card_pickup: Handle<AudioSource>,
    card_release: Handle<AudioSource>,
    coin_pickup: Handle<AudioSource>,
    coin_release: Handle<AudioSource>,
}

fn setup_sounds(mut commands: Commands, mut assets: ResMut<Assets<AudioSource>>) {
    let mut sound = |coin, pickup| {
        assets.add(AudioSource {
            bytes: synthesize(Cue { coin, pickup }).into(),
        })
    };
    commands.insert_resource(Sounds {
        card_pickup: sound(false, true),
        card_release: sound(false, false),
        coin_pickup: sound(true, true),
        coin_release: sound(true, false),
    });
}

fn gesture_sounds(
    mut commands: Commands,
    state: Res<UiState>,
    surface: Res<RenderSurface>,
    drag: Res<DragState>,
    money: Res<MoneyState>,
    sounds: Res<Sounds>,
    mut feedback: ResMut<SoundFeedback>,
) {
    let held = drag
        .card_key
        .as_ref()
        .map(|key| HeldPiece {
            key: key.clone(),
            coin: false,
        })
        .or_else(|| {
            money.held_coin().map(|(key, _, _)| HeldPiece {
                key: key.into(),
                coin: true,
            })
        });
    let cues = feedback.observe(held, state.screen == UiScreen::Table);
    // Automation observes lifecycle counters, never opens an output device or
    // makes the user's speakers sound during a background test.
    if !feedback.enabled || matches!(*surface, RenderSurface::Windowless { .. }) {
        return;
    }
    for cue in cues {
        let handle = match (cue.coin, cue.pickup) {
            (false, true) => &sounds.card_pickup,
            (false, false) => &sounds.card_release,
            (true, true) => &sounds.coin_pickup,
            (true, false) => &sounds.coin_release,
        };
        commands.spawn((
            AudioPlayer::new(handle.clone()),
            PlaybackSettings {
                volume: Volume::Linear(0.35),
                ..PlaybackSettings::DESPAWN
            },
        ));
        feedback.audible_count += 1;
    }
}

/// Original short PCM sounds generated in memory. No downloads or runtime
/// asset path dependency: a soft paper tap and a damped metallic coin clink.
fn synthesize(cue: Cue) -> Vec<u8> {
    let rate = 22_050_u32;
    let samples = if cue.coin { 3_087_u32 } else { 1_764_u32 };
    let data_bytes = samples * 2;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    let mut noise = 0x51f1_ae35_u32;
    for n in 0..samples {
        let t = n as f32 / rate as f32;
        let attack = (t * 900.).min(1.);
        let envelope = attack
            * (-t * if cue.coin { 38. } else { 65. }).exp()
            * (1. - n as f32 / samples as f32);
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (f64::from(noise) / f64::from(u32::MAX) * 2. - 1.) as f32;
        let frequency = if cue.pickup { 1_650. } else { 1_280. };
        let tone = (std::f32::consts::TAU * frequency * t).sin();
        let value = if cue.coin {
            0.5 * tone + 0.25 * (std::f32::consts::TAU * frequency * 2.73 * t).sin() + 0.08 * white
        } else {
            0.42 * white + 0.18 * (std::f32::consts::TAU * 190. * t).sin()
        };
        #[allow(clippy::cast_possible_truncation)]
        let sample = (value * envelope * 10_000.) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::audio::Decodable;
    #[test]
    fn one_pickup_and_release_not_one_sound_per_pose_echo() {
        let mut state = SoundFeedback::default();
        let piece = HeldPiece {
            key: "coin".into(),
            coin: true,
        };
        assert_eq!(
            state.observe(Some(piece.clone()), true),
            vec![Cue {
                coin: true,
                pickup: true
            }]
        );
        for _ in 0..100 {
            assert!(state.observe(Some(piece.clone()), true).is_empty());
        }
        assert_eq!(
            state.observe(None, true),
            vec![Cue {
                coin: true,
                pickup: false
            }]
        );
        assert!(state.observe(None, true).is_empty());
        assert_eq!((state.pickup_count, state.release_count), (1, 1));
    }
    #[test]
    fn leaving_or_resuming_does_not_play_a_spurious_drop() {
        let mut state = SoundFeedback::default();
        state.observe(
            Some(HeldPiece {
                key: "card".into(),
                coin: false,
            }),
            true,
        );
        assert!(state.observe(None, false).is_empty());
        assert!(state.observe(None, true).is_empty());
        assert_eq!(state.release_count, 0);
    }
    #[test]
    fn original_wav_effects_decode_and_end_without_clipping() {
        for coin in [false, true] {
            for pickup in [false, true] {
                let bytes = synthesize(Cue { coin, pickup });
                assert_eq!(&bytes[0..4], b"RIFF");
                let source = AudioSource {
                    bytes: bytes.into(),
                };
                let samples: Vec<_> = source.decoder().collect();
                assert!(!samples.is_empty());
                assert!(
                    samples
                        .iter()
                        .all(|sample| sample.is_finite() && sample.abs() < 0.4)
                );
                assert!(samples.iter().any(|sample| sample.abs() > 0.01));
            }
        }
    }
}
