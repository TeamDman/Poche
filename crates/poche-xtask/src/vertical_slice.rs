// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reproducible, inspectable spatial-tabletop publication bundle.

use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use poche_conformance::check_spatial_alloy_layout_micro;
use poche_protocol::{
    CommandId, EventId, GameActionWire, GovernanceCommandV1, GovernanceCommandWire,
    GovernedActionWire, PrincipalId, RecoveryActionWire, VoteChoiceWire,
};
use poche_runtime::{
    InProcessSmokeReport, ReplicatedMicroReport, SmokeOperation, SmokeStepRecord, SmokeTranscript,
    run_in_process_smoke_transcript, run_replicated_micro_check,
};
use poche_session::{
    ActionKnowledge, AuditedGameAction, FindingConfidence, GovernanceInvocation, GovernanceState,
    HistoryActionDisposition, ProposalStatus, RetrospectiveAudit,
};
use poche_spatial::{
    AabbMm, CardLocation, HalfExtentsMm, Point3Mm, ZoneId, interpret_score_sheet,
    reconstruct_animation_endpoint, resolve_card_play, resolve_drag_play, spatial_scene_hash_hex,
};
use poche_ui::{
    LiveClientInput, LiveClientPresentation, TabletopHtmlSupplement, embedded_spatial_fixture,
    render_tabletop_semantic_html,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA_VERSION: u16 = 1;
const SMOKE_SEED: u64 = 0x5eed;
const ARTIFACT_SUBDIRECTORY: &str = "spatial-vertical-slice";
const CHECKED_RECEIPT: &str = "docs/evidence/spatial-vertical-slice.json";
const CHECKED_WITNESS: &str = "tests/fixtures/spatial/alloy-overlap-negative-control-v1.json";
const PHASE_THREE_RELEASE_RECEIPT: &str = "docs/evidence/phase-3-release.json";
const ALLOY_RECEIPT: &str = "target/spatial-alloy-layout-micro/receipt.json";
const ALLOY_COMMAND: &str = "OverlapNegativeControl";
const WITNESS_TOKEN: &str = "<!--__POCHE_SPATIAL_WITNESS__-->";
const ENDPOINT_TOKEN: &str = "<!--__POCHE_SPATIAL_ENDPOINT__-->";

#[derive(Debug)]
pub struct VerticalSliceBuildReport {
    pub transcript_steps: usize,
    pub scene_hash: String,
    pub alloy_witness: String,
    pub artifact_directory: PathBuf,
}

#[derive(Debug)]
pub struct PagesBuildReport {
    pub pages: usize,
    pub evidence_files: usize,
    pub output_directory: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlloyOverlapWitness {
    schema_version: u16,
    source_model: String,
    command: String,
    outcome: String,
    scope: String,
    relation: String,
    zones: Vec<String>,
    shared_abstract_cell: String,
    meaning: String,
    qualification: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "the checked receipt makes the millimetre unit explicit on every coordinate"
)]
struct PointReceipt {
    x_mm: i32,
    y_mm: i32,
    z_mm: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpatialInteractionReceipt {
    scene_hash: String,
    face_code: u8,
    face_label: String,
    object_projection_epoch: u64,
    object_ordinal: u8,
    source: String,
    destination: String,
    from: PointReceipt,
    to: PointReceipt,
    duration_milliseconds: u32,
    typed_drag_equal: bool,
    score_text: Vec<String>,
    semantic_html_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditReceipt {
    offending_action: String,
    revealing_action: String,
    findings_before_reveal: usize,
    finding_id: String,
    confidence: String,
    revealed_card: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GovernanceReceipt {
    proposal_id: String,
    action: String,
    visible_accused_vote_counted: bool,
    eligible_voters: u16,
    approvals: u16,
    rejections: u16,
    effect: String,
    target_kicked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptReceipt {
    steps: usize,
    ndjson_hash: String,
    coverage: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerticalSliceReceipt {
    schema_version: u16,
    scope_id: String,
    composition_qualification: String,
    transcript: TranscriptReceipt,
    lifecycle: InProcessSmokeReport,
    replicated_devices: ReplicatedMicroReport,
    spatial_interaction: SpatialInteractionReceipt,
    delayed_audit: AuditReceipt,
    recovery_vote: GovernanceReceipt,
    alloy_overlap_witness: AlloyOverlapWitness,
    artifacts: Vec<String>,
    exclusions: Vec<String>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct NdjsonStep<'a> {
    schema_version: u16,
    step: usize,
    input: &'a poche_runtime::SmokeInput,
    record: &'a SmokeStepRecord,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SupplementalNdjsonEvent<'a, T> {
    schema_version: u16,
    step: usize,
    track: &'static str,
    event: &'static str,
    evidence: &'a T,
}

#[expect(
    clippy::too_many_lines,
    reason = "the vertical-slice acceptance remains chronological so its cross-track evidence order is auditable"
)]
pub fn build_vertical_slice(root: &Path) -> Result<VerticalSliceBuildReport, SliceError> {
    let root = normalize_root(root)?;
    let artifact_directory = root.join("target").join(ARTIFACT_SUBDIRECTORY);
    fs::create_dir_all(&artifact_directory).map_err(io_error)?;

    let (transcript, lifecycle) = run_in_process_smoke_transcript(SMOKE_SEED).map_err(problem)?;
    let mut transcript_ndjson = encode_transcript_ndjson(&transcript)?;
    validate_transcript(&transcript, &lifecycle)?;

    let replicated_devices =
        run_replicated_micro_check().map_err(|error| problem(error.to_string()))?;
    if !replicated_devices.convergence || replicated_devices.replicas < 2 {
        return Err(problem(
            "replicated micro-scenario did not converge across multiple devices",
        ));
    }

    let fixture = embedded_spatial_fixture().map_err(problem)?;
    let scene_hash = spatial_scene_hash_hex(&fixture.scene).map_err(debug_problem)?;
    let owned = fixture
        .scene
        .cards
        .iter()
        .find(|card| {
            card.face.is_some()
                && matches!(card.location, CardLocation::Hand { seat, .. } if seat == fixture.issuing_seat)
        })
        .ok_or_else(|| problem("embedded spatial fixture has no visible owned card"))?;
    let face = owned
        .face
        .ok_or_else(|| problem("selected spatial card unexpectedly has no visible face"))?;
    let typed = resolve_card_play(&fixture.layout, &fixture.scene, fixture.issuing_seat, face)
        .map_err(debug_problem)?;
    let dragged = resolve_drag_play(
        &fixture.layout,
        &fixture.scene,
        fixture.issuing_seat,
        owned.id,
        center_bound(&fixture.layout, ZoneId::Play)?,
    )
    .map_err(debug_problem)?;
    if typed != dragged {
        return Err(problem(
            "typed and dragged spatial play resolved differently",
        ));
    }
    let endpoint =
        reconstruct_animation_endpoint(&fixture.layout, typed.record).map_err(debug_problem)?;
    let score_text = interpret_score_sheet(&fixture.scene)
        .map_err(debug_problem)?
        .into_iter()
        .map(|(seat, score)| format!("seat {} = {score}", seat.get()))
        .collect::<Vec<_>>();
    let live = LiveClientPresentation::from_input(
        fixture.presentation.clone(),
        LiveClientInput {
            room_id: "checked-spatial-replay".to_owned(),
            authority_instance: "static-vertical-slice/0".to_owned(),
            authority_revision: 0,
            room_code: None,
            join_proof: None,
            seat_count: fixture.layout.id().players(),
            chat_draft: None,
            countdown_command: None,
            next_grant_epoch: 1,
            hand_requests: Vec::new(),
            hand_grants: Vec::new(),
            transcript_href: Some("transcript.ndjson".to_owned()),
            replay_href: Some("../../pages/replay/".to_owned()),
        },
    );
    let semantic_fragment = render_tabletop_semantic_html(
        &live,
        &fixture.scene,
        "vertical-slice-tabletop",
        "commands",
        &TabletopHtmlSupplement {
            status: Some(
                "static checked projection; controls are illustrative and have no live authority"
                    .to_owned(),
            ),
            viewer_href_prefix: None,
            findings: Vec::new(),
            proposals: Vec::new(),
        },
    )
    .map_err(debug_problem)?;
    let semantic_html = standalone_semantic_html(&semantic_fragment);
    let spatial_interaction = SpatialInteractionReceipt {
        scene_hash: scene_hash.clone(),
        face_code: face.code(),
        face_label: face.label(),
        object_projection_epoch: owned.id.projection_epoch,
        object_ordinal: owned.id.ordinal,
        source: location_label(typed.record.source),
        destination: location_label(typed.record.destination),
        from: point_receipt(endpoint.from.translation),
        to: point_receipt(endpoint.to.translation),
        duration_milliseconds: endpoint.duration_milliseconds,
        typed_drag_equal: true,
        score_text,
        semantic_html_hash: hash(semantic_html.as_bytes()),
    };

    let (audit, finding_id) = delayed_audit()?;
    let recovery_vote = recovery_vote(&audit, &finding_id)?;

    let alloy_report =
        check_spatial_alloy_layout_micro(&root).map_err(|error| problem(error.to_string()))?;
    let alloy_witness = extract_alloy_witness(&root.join(ALLOY_RECEIPT), alloy_report.scope)?;
    compare_checked_json(&root.join(CHECKED_WITNESS), &alloy_witness, "Alloy witness")?;

    let mut next_step = transcript.inputs.len();
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "replication",
        "multiple_devices_converged",
        &replicated_devices,
    )?;
    next_step += 1;
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "spatial",
        "typed_play_resolved",
        &spatial_interaction,
    )?;
    next_step += 1;
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "spatial",
        "drag_play_resolved_to_same_endpoint",
        &spatial_interaction,
    )?;
    next_step += 1;
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "spatial",
        "score_text_interpreted",
        &spatial_interaction.score_text,
    )?;
    next_step += 1;
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "audit",
        "delayed_cheat_finding",
        &audit,
    )?;
    next_step += 1;
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "governance",
        "recovery_vote_applied",
        &recovery_vote,
    )?;
    next_step += 1;
    append_ndjson_event(
        &mut transcript_ndjson,
        next_step,
        "replay",
        "final_state_replayed",
        &lifecycle,
    )?;
    next_step += 1;
    reject_secrets(&transcript_ndjson)?;
    let transcript_receipt = TranscriptReceipt {
        steps: next_step,
        ndjson_hash: hash(transcript_ndjson.as_bytes()),
        coverage: vec![
            "room-join".to_owned(),
            "multiple-devices".to_owned(),
            "all-deals".to_owned(),
            "typed-drag-equivalence".to_owned(),
            "delayed-cheat-finding".to_owned(),
            "recovery-vote".to_owned(),
            "score-text".to_owned(),
            "pause-unpause".to_owned(),
            "chat".to_owned(),
            "hand-grant-revoke".to_owned(),
            "disconnect-reconnect".to_owned(),
            "independent-final-replay".to_owned(),
        ],
    };

    let receipt = VerticalSliceReceipt {
        schema_version: SCHEMA_VERSION,
        scope_id: "poche-spatial-vertical-slice-v1".to_owned(),
        composition_qualification: "A deterministic evidence bundle composes the full in-process lifecycle, replicated-device micro-scenario, checked replay-derived exact-recipient scene, audit/governance reducers, and bounded Alloy layout witness. These components share contracts but are not one atomic live network execution.".to_owned(),
        transcript: transcript_receipt,
        lifecycle,
        replicated_devices,
        spatial_interaction,
        delayed_audit: audit,
        recovery_vote,
        alloy_overlap_witness: alloy_witness.clone(),
        artifacts: vec![
            "transcript.ndjson".to_owned(),
            "semantic-tabletop.html".to_owned(),
            "alloy-overlap.html".to_owned(),
            "receipt.json".to_owned(),
        ],
        exclusions: vec![
            "No native window, browser network, Veilid route, GPU frame, or external relay is driven by this offline command.".to_owned(),
            "The Alloy result is bounded to its printed layout-micro scope and the overlap assertion is a deliberate negative control.".to_owned(),
            "The HTML artifact is a static exact-recipient projection; its forms have no authority endpoint.".to_owned(),
            "The transcript contains invite references, never invite proofs, device private keys, hidden hands for another viewer, or room secrets.".to_owned(),
        ],
    };
    let receipt_json = pretty_json(&receipt)?;

    write_file(
        &artifact_directory.join("transcript.ndjson"),
        &transcript_ndjson,
    )?;
    write_file(
        &artifact_directory.join("semantic-tabletop.html"),
        &semantic_html,
    )?;
    write_file(
        &artifact_directory.join("alloy-overlap.html"),
        &standalone_witness_html(&alloy_witness),
    )?;
    write_file(&artifact_directory.join("receipt.json"), &receipt_json)?;
    compare_checked_text(
        &root.join(CHECKED_RECEIPT),
        &receipt_json,
        "vertical-slice receipt",
    )?;

    Ok(VerticalSliceBuildReport {
        transcript_steps: receipt.transcript.steps,
        scene_hash,
        alloy_witness: format!(
            "{}+{} share {}",
            alloy_witness.zones[0], alloy_witness.zones[1], alloy_witness.shared_abstract_cell
        ),
        artifact_directory,
    })
}

pub fn build_pages(root: &Path, output: &Path) -> Result<PagesBuildReport, SliceError> {
    let root = normalize_root(root)?;
    let witness: AlloyOverlapWitness = read_json(&root.join(CHECKED_WITNESS))?;
    let receipt: VerticalSliceReceipt = read_json(&root.join(CHECKED_RECEIPT))?;
    if receipt.alloy_overlap_witness != witness {
        return Err(problem("checked Pages receipt and Alloy witness disagree"));
    }
    fs::create_dir_all(output).map_err(io_error)?;
    let pages_directory = root.join("pages");
    let mut sources = fs::read_dir(&pages_directory)
        .map_err(io_error)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "html")
        })
        .collect::<Vec<_>>();
    sources.sort();
    if sources.is_empty() {
        return Err(problem("Pages source directory contains no HTML files"));
    }

    let git_sha = safe_git_sha(env::var("GITHUB_SHA").ok().as_deref());
    let build_time = escape_html(
        env::var("PUBLICATION_TIME")
            .ok()
            .as_deref()
            .unwrap_or("local-build"),
    );
    let witness_fragment = render_witness_fragment(&witness);
    let endpoint_fragment = render_endpoint_fragment(&receipt.spatial_interaction);
    for source in &sources {
        let file_name = source
            .file_name()
            .ok_or_else(|| problem("Pages source omitted a file name"))?;
        let mut html = fs::read_to_string(source).map_err(io_error)?;
        html = html
            .replace("__POCHE_GIT_SHA__", &git_sha)
            .replace("__POCHE_BUILD_TIME__", &build_time)
            .replace(WITNESS_TOKEN, &witness_fragment)
            .replace(ENDPOINT_TOKEN, &endpoint_fragment);
        if html.contains("__POCHE_") {
            return Err(problem(format!(
                "unresolved publication placeholder in {}",
                source.display()
            )));
        }
        write_file(&output.join(file_name), &html)?;
    }

    fs::copy(root.join("LICENSE"), output.join("LICENSE.txt")).map_err(io_error)?;
    let evidence_directory = output.join("evidence");
    fs::create_dir_all(&evidence_directory).map_err(io_error)?;
    fs::copy(
        root.join(CHECKED_RECEIPT),
        evidence_directory.join("spatial-vertical-slice.json"),
    )
    .map_err(io_error)?;
    fs::copy(
        root.join(CHECKED_WITNESS),
        evidence_directory.join("alloy-overlap-negative-control-v1.json"),
    )
    .map_err(io_error)?;
    publish_release_receipt(&root, &evidence_directory)?;

    for required in [
        "index.html",
        "status.html",
        "spatial.html",
        "LICENSE.txt",
        "evidence/spatial-vertical-slice.json",
        "evidence/alloy-overlap-negative-control-v1.json",
        "evidence/phase-3-release.json",
    ] {
        let metadata = fs::metadata(output.join(required)).map_err(io_error)?;
        if metadata.len() == 0 {
            return Err(problem(format!("Pages output {required} is empty")));
        }
    }
    let spatial = fs::read_to_string(output.join("spatial.html")).map_err(io_error)?;
    for marker in [
        "data-alloy-command=\"OverlapNegativeControl\"",
        "data-scene-hash=",
        "Bounded SAT counterexample",
        "Typed and dragged input",
    ] {
        if !spatial.contains(marker) {
            return Err(problem(format!(
                "built spatial page omitted marker {marker}"
            )));
        }
    }

    Ok(PagesBuildReport {
        pages: sources.len(),
        evidence_files: 3,
        output_directory: output.to_path_buf(),
    })
}

fn publish_release_receipt(root: &Path, evidence_directory: &Path) -> Result<(), SliceError> {
    let release_receipt =
        fs::read_to_string(root.join(PHASE_THREE_RELEASE_RECEIPT)).map_err(io_error)?;
    serde_json::from_str::<Value>(&release_receipt).map_err(json_error)?;
    reject_secrets(&release_receipt)?;
    write_file(
        &evidence_directory.join("phase-3-release.json"),
        &release_receipt,
    )
}

fn delayed_audit() -> Result<(AuditReceipt, String), SliceError> {
    let mut audit = RetrospectiveAudit::default();
    audit
        .append_action(AuditedGameAction {
            event_id: event("vertical-off-suit")?,
            sequence: 1,
            round_id: 1,
            actor: principal("john")?,
            disposition: HistoryActionDisposition::AttemptedStructurallyValid,
            action: GameActionWire::Play { card: 26 },
            led_suit: Some(0),
            knowledge_at_action: ActionKnowledge {
                known_held_cards_before: Vec::new(),
            },
        })
        .map_err(debug_problem)?;
    let before = audit
        .audit(&principal("alice-device")?)
        .map_err(debug_problem)?
        .len();
    audit
        .append_action(AuditedGameAction {
            event_id: event("vertical-later-club")?,
            sequence: 2,
            round_id: 1,
            actor: principal("john")?,
            disposition: HistoryActionDisposition::Accepted,
            action: GameActionWire::Play { card: 4 },
            led_suit: Some(1),
            knowledge_at_action: ActionKnowledge {
                known_held_cards_before: Vec::new(),
            },
        })
        .map_err(debug_problem)?;
    let findings = audit
        .audit(&principal("alice-device")?)
        .map_err(debug_problem)?;
    let finding = findings
        .first()
        .ok_or_else(|| problem("later public play did not produce a delayed finding"))?;
    if before != 0 || finding.confidence != FindingConfidence::DelayedPublicPlay {
        return Err(problem(
            "delayed audit confidence or pre-reveal result drifted",
        ));
    }
    let finding_id = finding.finding_id.as_str().to_owned();
    Ok((
        AuditReceipt {
            offending_action: finding.offending_action_id.as_str().to_owned(),
            revealing_action: finding.revealing_evidence_id.as_str().to_owned(),
            findings_before_reveal: before,
            finding_id: finding_id.clone(),
            confidence: "delayed_public_play".to_owned(),
            revealed_card: finding.revealed_card,
        },
        finding_id,
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "the recovery transcript keeps finding replay, excluded vote, approvals, and applied effect in causal order"
)]
fn recovery_vote(
    audit_receipt: &AuditReceipt,
    finding_id: &str,
) -> Result<GovernanceReceipt, SliceError> {
    let mut audit = RetrospectiveAudit::default();
    audit
        .append_action(AuditedGameAction {
            event_id: event(&audit_receipt.offending_action)?,
            sequence: 1,
            round_id: 1,
            actor: principal("john")?,
            disposition: HistoryActionDisposition::AttemptedStructurallyValid,
            action: GameActionWire::Play { card: 26 },
            led_suit: Some(0),
            knowledge_at_action: ActionKnowledge {
                known_held_cards_before: Vec::new(),
            },
        })
        .map_err(debug_problem)?;
    audit
        .append_action(AuditedGameAction {
            event_id: event(&audit_receipt.revealing_action)?,
            sequence: 2,
            round_id: 1,
            actor: principal("john")?,
            disposition: HistoryActionDisposition::Accepted,
            action: GameActionWire::Play { card: 4 },
            led_suit: Some(1),
            knowledge_at_action: ActionKnowledge {
                known_held_cards_before: Vec::new(),
            },
        })
        .map_err(debug_problem)?;
    let finding = audit
        .audit(&principal("alice-device")?)
        .map_err(debug_problem)?
        .into_iter()
        .find(|candidate| candidate.finding_id.as_str() == finding_id)
        .ok_or_else(|| problem("governance audit replay did not reproduce finding"))?;

    let mut state = GovernanceState::new(
        [principal("alice")?, principal("bob")?, principal("john")?],
        10,
    )
    .map_err(debug_problem)?;
    state
        .register_confirmed_finding(&audit, &finding.finding_id)
        .map_err(debug_problem)?;
    let start = state
        .submit(invocation(
            "vertical-start-kick",
            "alice",
            1,
            GovernanceCommandWire::StartVote {
                action: GovernedActionWire::Recover {
                    recovery: RecoveryActionWire::Kick {
                        target: principal("john")?,
                    },
                },
            },
        )?)
        .map_err(debug_problem)?;
    let proposal_id = start
        .proposal_id
        .ok_or_else(|| problem("recovery vote did not create a proposal"))?;
    let accused_vote = state
        .submit(invocation(
            "vertical-john-visible-no",
            "john",
            2,
            GovernanceCommandWire::Vote {
                proposal_id: proposal_id.clone(),
                choice: VoteChoiceWire::Reject,
            },
        )?)
        .map_err(debug_problem)?;
    state
        .submit(invocation(
            "vertical-alice-yes",
            "alice",
            3,
            GovernanceCommandWire::Vote {
                proposal_id: proposal_id.clone(),
                choice: VoteChoiceWire::Approve,
            },
        )?)
        .map_err(debug_problem)?;
    let approval = state
        .submit(invocation(
            "vertical-bob-yes",
            "bob",
            4,
            GovernanceCommandWire::Vote {
                proposal_id: proposal_id.clone(),
                choice: VoteChoiceWire::Approve,
            },
        )?)
        .map_err(debug_problem)?;
    let proposal = state
        .proposals()
        .iter()
        .find(|candidate| candidate.proposal_id == proposal_id)
        .ok_or_else(|| problem("approved recovery proposal disappeared"))?;
    let ProposalStatus::Approved { tally, .. } = proposal.status else {
        return Err(problem("recovery proposal did not reach approved status"));
    };
    if accused_vote.vote_counted != Some(false)
        || approval.effect_id.is_none()
        || !state.is_kicked(&principal("john")?)
    {
        return Err(problem("recovery vote visibility/counting/effect drifted"));
    }
    Ok(GovernanceReceipt {
        proposal_id: proposal_id.as_str().to_owned(),
        action: "recover.kick(john)".to_owned(),
        visible_accused_vote_counted: false,
        eligible_voters: tally.eligible,
        approvals: tally.approvals,
        rejections: tally.rejections,
        effect: "john removed from active roster".to_owned(),
        target_kicked: true,
    })
}

fn invocation(
    command_id: &str,
    issuer: &str,
    logical_tick: u64,
    command: GovernanceCommandWire,
) -> Result<GovernanceInvocation, SliceError> {
    Ok(GovernanceInvocation {
        command_id: CommandId::new(command_id).map_err(debug_problem)?,
        issuer: principal(issuer)?,
        logical_tick,
        command: GovernanceCommandV1::new(command).map_err(debug_problem)?,
    })
}

fn extract_alloy_witness(path: &Path, scope: &str) -> Result<AlloyOverlapWitness, SliceError> {
    let receipt: Value = read_json(path)?;
    let instance = receipt
        .pointer("/commands/OverlapNegativeControl/solution/0/instances/0")
        .ok_or_else(|| problem("Alloy receipt omitted overlap counterexample instance"))?;
    let layout = instance
        .pointer("/values/Layout$0/outer")
        .and_then(Value::as_array)
        .ok_or_else(|| problem("Alloy overlap instance omitted Layout.outer"))?;
    let cell_for = |zone: &str| -> Result<&str, SliceError> {
        layout
            .iter()
            .filter_map(Value::as_array)
            .find(|pair| pair.first().and_then(Value::as_str) == Some(zone))
            .and_then(|pair| pair.get(1))
            .and_then(Value::as_str)
            .ok_or_else(|| problem(format!("Alloy witness omitted {zone} outer cell")))
    };
    let deck = cell_for("DeckZone$0")?;
    let trump = cell_for("TrumpZone$0")?;
    if deck != trump {
        return Err(problem(
            "retained Alloy instance does not make deck and trump share an outer cell",
        ));
    }
    Ok(AlloyOverlapWitness {
        schema_version: SCHEMA_VERSION,
        source_model: "models/alloy/spatial.als".to_owned(),
        command: ALLOY_COMMAND.to_owned(),
        outcome: "sat".to_owned(),
        scope: scope.to_owned(),
        relation: "Layout.outer".to_owned(),
        zones: vec!["deck".to_owned(), "trump".to_owned()],
        shared_abstract_cell: "shared-cell-0".to_owned(),
        meaning: "The deliberately false separation assertion admits a layout where the deck and trump outer volumes overlap. A first-match drop classifier could therefore choose by iteration order.".to_owned(),
        qualification: "This is one normalized SAT instance of a deliberate negative control in the printed finite Alloy scope; it is not a concrete millimetre layout and does not claim an unbounded theorem.".to_owned(),
    })
}

fn render_witness_fragment(witness: &AlloyOverlapWitness) -> String {
    format!(
        "<figure class=\"witness\" data-alloy-command=\"{}\"><svg viewBox=\"0 0 640 280\" role=\"img\" aria-labelledby=\"alloy-witness-title alloy-witness-desc\"><title id=\"alloy-witness-title\">Alloy overlap counterexample</title><desc id=\"alloy-witness-desc\">The deck and trump outer zones occupy one shared abstract cell, violating zone separation.</desc><rect class=\"zone deck\" x=\"95\" y=\"55\" width=\"275\" height=\"170\" rx=\"18\"/><rect class=\"zone trump\" x=\"270\" y=\"55\" width=\"275\" height=\"170\" rx=\"18\"/><text x=\"145\" y=\"95\">deck outer</text><text x=\"405\" y=\"95\">trump outer</text><text class=\"overlap-label\" x=\"320\" y=\"155\" text-anchor=\"middle\">{}</text></svg><figcaption><strong>Bounded SAT counterexample.</strong> {} <span>{}</span></figcaption></figure>",
        escape_html(&witness.command),
        escape_html(&witness.shared_abstract_cell),
        escape_html(&witness.meaning),
        escape_html(&witness.qualification),
    )
}

fn render_endpoint_fragment(receipt: &SpatialInteractionReceipt) -> String {
    format!(
        "<figure class=\"endpoint\" data-scene-hash=\"{}\"><svg viewBox=\"0 0 720 260\" role=\"img\" aria-labelledby=\"endpoint-title endpoint-desc\"><title id=\"endpoint-title\">Checked card endpoint</title><desc id=\"endpoint-desc\">Typed and dragged input select the same opaque card and resolve from an owned hand to the play zone.</desc><rect class=\"hand\" x=\"45\" y=\"65\" width=\"220\" height=\"130\" rx=\"18\"/><rect class=\"play\" x=\"455\" y=\"65\" width=\"220\" height=\"130\" rx=\"18\"/><path d=\"M245 130 C340 35 385 35 475 130\"/><polygon points=\"475,130 450,116 456,144\"/><text x=\"155\" y=\"115\" text-anchor=\"middle\">{}</text><text x=\"565\" y=\"115\" text-anchor=\"middle\">play zone</text><text x=\"360\" y=\"225\" text-anchor=\"middle\">typed play = drag release · {} ms</text></svg><figcaption><strong>One semantic endpoint.</strong> Card <code>{}</code>, object epoch {} ordinal {}, moves from <code>{}</code> to <code>{}</code>. The canonical scene fingerprint is <code>{}</code>.</figcaption></figure>",
        escape_html(&receipt.scene_hash),
        escape_html(&receipt.face_label),
        receipt.duration_milliseconds,
        escape_html(&receipt.face_label),
        receipt.object_projection_epoch,
        receipt.object_ordinal,
        escape_html(&receipt.source),
        escape_html(&receipt.destination),
        escape_html(&receipt.scene_hash),
    )
}

fn standalone_witness_html(witness: &AlloyOverlapWitness) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Poche Alloy overlap witness</title><style>body{{max-width:55rem;margin:3rem auto;padding:0 1rem;font:1rem/1.6 system-ui,sans-serif}}.zone{{fill-opacity:.45;stroke-width:4}}.deck{{fill:#5aa9e6;stroke:#25658f}}.trump{{fill:#ef6f6c;stroke:#9d3432}}text{{font:600 18px system-ui,sans-serif}}.overlap-label{{font-size:15px}}figcaption span{{display:block;color:#555;margin-top:.6rem}}</style></head><body><main><h1>Normalized Alloy overlap witness</h1>{}</main></body></html>",
        render_witness_fragment(witness)
    )
}

fn standalone_semantic_html(fragment: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Poche checked semantic tabletop</title><style>body{{max-width:72rem;margin:2rem auto;padding:0 1rem;font:1rem/1.55 system-ui,sans-serif}}section{{border-top:1px solid #aaa;padding:1rem 0}}table{{border-collapse:collapse}}th,td{{border:1px solid #999;padding:.35rem .6rem}}button{{padding:.5rem;margin:.2rem}}</style></head><body>{fragment}<footer><p>Static exact-recipient artifact: forms are inspectable but have no live authority endpoint.</p></footer></body></html>"
    )
}

fn encode_transcript_ndjson(transcript: &SmokeTranscript) -> Result<String, SliceError> {
    if transcript.inputs.len() != transcript.records.len() {
        return Err(problem(
            "smoke transcript inputs and records have different lengths",
        ));
    }
    let mut ndjson = String::new();
    for (index, (input, record)) in transcript
        .inputs
        .iter()
        .zip(&transcript.records)
        .enumerate()
    {
        let line = serde_json::to_string(&NdjsonStep {
            schema_version: SCHEMA_VERSION,
            step: index,
            input,
            record,
        })
        .map_err(json_error)?;
        ndjson.push_str(&line);
        ndjson.push('\n');
    }
    Ok(ndjson)
}

fn append_ndjson_event<T: Serialize>(
    ndjson: &mut String,
    step: usize,
    track: &'static str,
    event: &'static str,
    evidence: &T,
) -> Result<(), SliceError> {
    let line = serde_json::to_string(&SupplementalNdjsonEvent {
        schema_version: SCHEMA_VERSION,
        step,
        track,
        event,
        evidence,
    })
    .map_err(json_error)?;
    ndjson.push_str(&line);
    ndjson.push('\n');
    Ok(())
}

fn validate_transcript(
    transcript: &SmokeTranscript,
    report: &InProcessSmokeReport,
) -> Result<(), SliceError> {
    let checks = [
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::Join { .. })
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::ApplySeededChance { .. })
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::Pause)
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::Unpause)
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::Chat { .. })
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::GrantHand { .. })
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::RevokeHand { .. })
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::Disconnect)
        }),
        has_operation(transcript, |operation| {
            matches!(operation, SmokeOperation::Reconnect)
        }),
        report
            .verified
            .iter()
            .any(|item| item == "transcript-replay"),
    ];
    if checks.into_iter().all(|passed| passed) {
        Ok(())
    } else {
        Err(problem(
            "registered lifecycle transcript lost a required vertical-slice operation",
        ))
    }
}

fn has_operation(
    transcript: &SmokeTranscript,
    predicate: impl Fn(&SmokeOperation) -> bool,
) -> bool {
    transcript
        .inputs
        .iter()
        .any(|input| predicate(&input.operation))
}

fn center_bound(layout: &poche_spatial::SpatialLayout, zone: ZoneId) -> Result<AabbMm, SliceError> {
    let volume = layout
        .zones()
        .iter()
        .find(|candidate| candidate.id == zone)
        .ok_or_else(|| problem("registered layout omitted the play zone"))?;
    let center = Point3Mm::new(
        i32::midpoint(volume.inner.min.x.get(), volume.inner.max.x.get()),
        i32::midpoint(volume.inner.min.y.get(), volume.inner.max.y.get()),
        i32::midpoint(volume.inner.min.z.get(), volume.inner.max.z.get()),
    );
    AabbMm::from_center(center, HalfExtentsMm::new(32, 1, 44))
        .ok_or_else(|| problem("play-zone center did not admit a card bound"))
}

fn location_label(location: CardLocation) -> String {
    match location {
        CardLocation::Deck { index_from_bottom } => format!("deck[{index_from_bottom}]"),
        CardLocation::Trump => "trump".to_owned(),
        CardLocation::Hand {
            seat,
            index_from_left,
        } => format!("hand(seat={})[{index_from_left}]", seat.get()),
        CardLocation::Play { seat } => format!("play(seat={})", seat.get()),
        CardLocation::Won { seat, trick, index } => {
            format!("won(seat={},trick={trick},index={index})", seat.get())
        }
    }
}

const fn point_receipt(point: Point3Mm) -> PointReceipt {
    PointReceipt {
        x_mm: point.x.get(),
        y_mm: point.y.get(),
        z_mm: point.z.get(),
    }
}

fn reject_secrets(text: &str) -> Result<(), SliceError> {
    for marker in [
        "runtime-only-smoke-player-invite",
        "runtime-only-smoke-spectator-invite",
        "private_key",
        "secret_key",
    ] {
        if text.contains(marker) {
            return Err(problem(format!(
                "vertical-slice artifact contains forbidden marker {marker}"
            )));
        }
    }
    Ok(())
}

fn compare_checked_json<T>(path: &Path, actual: &T, label: &str) -> Result<(), SliceError>
where
    T: for<'de> Deserialize<'de> + PartialEq,
{
    let expected: T = read_json(path)?;
    if &expected == actual {
        Ok(())
    } else {
        Err(problem(format!("{label} drifted from {}", path.display())))
    }
}

fn compare_checked_text(path: &Path, actual: &str, label: &str) -> Result<(), SliceError> {
    let expected = fs::read_to_string(path).map_err(|error| {
        problem(format!(
            "could not read checked {label} {}: {error}; generated candidate is in target/{ARTIFACT_SUBDIRECTORY}",
            path.display()
        ))
    })?;
    if normalize_newlines(&expected).trim_end() == normalize_newlines(actual).trim_end() {
        Ok(())
    } else {
        Err(problem(format!(
            "{label} drifted from {}; inspect target/{ARTIFACT_SUBDIRECTORY}/receipt.json",
            path.display()
        )))
    }
}

fn normalize_root(root: &Path) -> Result<PathBuf, SliceError> {
    let canonical = root.canonicalize().map_err(io_error)?;
    let text = canonical.to_string_lossy();
    Ok(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text)))
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn pretty_json(value: &impl Serialize) -> Result<String, SliceError> {
    let mut json = serde_json::to_string_pretty(value).map_err(json_error)?;
    json.push('\n');
    Ok(json)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, SliceError> {
    let text = fs::read_to_string(path).map_err(io_error)?;
    serde_json::from_str(&text).map_err(json_error)
}

fn write_file(path: &Path, contents: &str) -> Result<(), SliceError> {
    fs::write(path, contents).map_err(io_error)
}

fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn principal(value: &str) -> Result<PrincipalId, SliceError> {
    PrincipalId::new(value).map_err(debug_problem)
}

fn event(value: &str) -> Result<EventId, SliceError> {
    EventId::new(value).map_err(debug_problem)
}

fn safe_git_sha(value: Option<&str>) -> String {
    value
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
        .unwrap_or("local-source")
        .to_owned()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn io_error(error: std::io::Error) -> SliceError {
    let message = error.to_string();
    drop(error);
    problem(message)
}

fn json_error(error: serde_json::Error) -> SliceError {
    let message = error.to_string();
    drop(error);
    problem(message)
}

fn debug_problem(error: impl fmt::Debug) -> SliceError {
    problem(format!("{error:?}"))
}

fn problem(message: impl Into<String>) -> SliceError {
    SliceError(message.into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SliceError(String);

impl fmt::Display for SliceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SliceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_renderer_retains_semantics_and_scope() {
        let witness = AlloyOverlapWitness {
            schema_version: 1,
            source_model: "model.als".to_owned(),
            command: ALLOY_COMMAND.to_owned(),
            outcome: "sat".to_owned(),
            scope: "micro".to_owned(),
            relation: "Layout.outer".to_owned(),
            zones: vec!["deck".to_owned(), "trump".to_owned()],
            shared_abstract_cell: "shared-cell-0".to_owned(),
            meaning: "overlap".to_owned(),
            qualification: "bounded".to_owned(),
        };
        let html = render_witness_fragment(&witness);
        assert!(html.contains("data-alloy-command=\"OverlapNegativeControl\""));
        assert!(html.contains("shared-cell-0"));
        assert!(html.contains("bounded"));
    }

    #[test]
    fn publication_substitutions_are_injection_safe() {
        assert_eq!(safe_git_sha(Some("abc-123")), "abc-123");
        assert_eq!(safe_git_sha(Some("<script>")), "local-source");
        assert_eq!(escape_html("<x & y>"), "&lt;x &amp; y&gt;");
    }
}
