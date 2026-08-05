// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use burn::{
    backend::{flex::FlexDevice, wgpu::WgpuDevice},
    prelude::*,
    tensor::{TensorData, backend::AutodiffBackend},
};
use poche_burn::{
    ActorCriticConfig, CheckpointManifest, CpuBackend, GpuBackend, PpoBatch, PpoConfig, PpoLearner,
    digest, masked_probabilities,
};
use poche_rl::{ACTION_COUNT, OBSERVATION_SIZE, PocheRlEnv, REWARD_ID, RlSpec, SPEC_ID};
use serde::Serialize;

#[derive(Serialize)]
struct BackendEvidence {
    schema_version: u16,
    burn_version: &'static str,
    backend: &'static str,
    observation_size: usize,
    action_count: usize,
    legal_actions: usize,
    illegal_probability_sum: f32,
    forward_backward: bool,
    checkpoint_round_trip: bool,
    model_bytes: usize,
    optimizer_bytes: usize,
    output_digest: String,
    empirical_only: bool,
}

#[allow(clippy::too_many_lines)] // One backend transaction is easier to compare when kept contiguous.
fn run<B: AutodiffBackend>(backend: &'static str, device: &B::Device) -> BackendEvidence {
    B::seed(device, 0x5050_4f43_4845);
    let model_config = ActorCriticConfig::poche_v1(32);
    let ppo = PpoConfig::default();
    let mut learner = PpoLearner::<B>::new(model_config, ppo, device);
    let env = PocheRlEnv::reset(17).expect("checked full-rule environment seed");
    let decision = env.decision().expect("initial policy turn");
    let action = decision
        .legal_mask
        .iter()
        .position(|legal| *legal)
        .expect("policy turn has legal action");
    let legal_actions = decision.legal_mask.iter().filter(|legal| **legal).count();
    let legal_count = u16::try_from(legal_actions).expect("action vocabulary fits u16");
    let old_log_probability = -f32::from(legal_count).ln();
    learner
        .update(
            &PpoBatch {
                observations: decision.observation.to_vec(),
                legal_masks: decision.legal_mask.to_vec(),
                actions: vec![action],
                old_log_probabilities: vec![old_log_probability],
                advantages: vec![1.0],
                returns: vec![1.0],
            },
            device,
        )
        .expect("valid backend PPO update");

    let input = Tensor::<B, 2>::from_data(
        TensorData::new(decision.observation.to_vec(), [1, OBSERVATION_SIZE]),
        device,
    );
    let before = learner
        .model
        .forward(input.clone())
        .logits
        .into_data()
        .to_vec::<f32>()
        .expect("backend logits convert to f32");
    let probabilities = masked_probabilities(&before, &decision.legal_mask);
    let illegal_probability_sum = probabilities
        .iter()
        .zip(decision.legal_mask)
        .filter(|(_, legal)| !*legal)
        .map(|(probability, _)| probability)
        .sum();
    let (model_bytes, optimizer_bytes) = learner.record_bytes().expect("backend record bytes");
    let mut before_bytes = Vec::with_capacity(before.len() * 4);
    for value in &before {
        before_bytes.extend_from_slice(&value.to_le_bytes());
    }
    let policy_probe_digest = digest(&before_bytes);
    let spec_hash = RlSpec::poche_2p_v1()
        .semantic_hash()
        .expect("checked RL spec serializes");
    let manifest = CheckpointManifest {
        schema_version: 1,
        run_id: format!("backend-smoke-{backend}"),
        burn_version: "0.21.0".to_owned(),
        spec_id: SPEC_ID.to_owned(),
        spec_hash: spec_hash.clone(),
        reward_id: REWARD_ID.to_owned(),
        model: model_config,
        ppo,
        seed: 0x5050_4f43_4845,
        updates: 1,
        model_digest: digest(&model_bytes),
        optimizer_digest: digest(&optimizer_bytes),
        policy_probe_digest,
    };
    manifest
        .validate(
            SPEC_ID,
            &spec_hash,
            REWARD_ID,
            &model_bytes,
            &optimizer_bytes,
        )
        .expect("fresh manifest validates");
    let restored = PpoLearner::<B>::new(model_config, ppo, device)
        .load_record_bytes(model_bytes.clone(), optimizer_bytes.clone(), device)
        .expect("backend checkpoint restores");
    let after = restored
        .model
        .forward(input)
        .logits
        .into_data()
        .to_vec::<f32>()
        .expect("restored logits convert to f32");
    let mut output_bytes = Vec::with_capacity(after.len() * 4);
    for value in &after {
        output_bytes.extend_from_slice(&value.to_le_bytes());
    }
    BackendEvidence {
        schema_version: 1,
        burn_version: "0.21.0",
        backend,
        observation_size: OBSERVATION_SIZE,
        action_count: ACTION_COUNT,
        legal_actions,
        illegal_probability_sum,
        forward_backward: true,
        checkpoint_round_trip: before == after,
        model_bytes: model_bytes.len(),
        optimizer_bytes: optimizer_bytes.len(),
        output_digest: digest(&output_bytes),
        empirical_only: true,
    }
}

fn main() {
    let backend = std::env::args().nth(1).unwrap_or_else(|| "cpu".to_owned());
    let evidence = match backend.as_str() {
        "cpu" => run::<CpuBackend>("flex-cpu", &FlexDevice),
        "gpu" => run::<GpuBackend>("wgpu-default", &WgpuDevice::default()),
        _ => {
            eprintln!("usage: poche-burn-backend-smoke [cpu|gpu]");
            std::process::exit(2);
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&evidence).expect("evidence serializes")
    );
    if evidence.illegal_probability_sum.to_bits() != 0.0_f32.to_bits()
        || !evidence.checkpoint_round_trip
    {
        std::process::exit(1);
    }
}
