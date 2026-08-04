# Poche formal models and executable Rust environment

**Plan status:** Ready for `/goal` execution

**Primary implementation root:** `D:\Repos\Games\poche-3` on orphan branch `model-checking`

**Last updated:** 2026-08-03

**Intent audit:** Passed 2026-08-03 against all original user messages available in this task

## How to update this plan

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

Put a work item's status in its heading. Update its heading and its completion
notes together. A phase is complete only when every work item in it is `[x]`.
Record decisions, commit IDs, validation results, and follow-ups below the task
they affect; do not append a detached chronological work log.

Use `[!]` only with the exact blocker, the last evidence gathered, and the
condition that would unblock it. Keep at most one current implementation focus
unless the plan explicitly identifies independent parallel tracks and owners.

## Guidance preservation protocol

The authoritative guidance ledger below exists to prevent user intent from
being weakened by summarization, plan cleanup, or context compaction.

- Read the guidance ledger before changing scope, architecture, ordering, or
  acceptance criteria.
- Add new material guidance as a stable `U` requirement before editing tasks.
- Do not remove or silently generalize a guidance entry. If the user changes a
  decision, mark the old entry `Superseded by Ux` and add the replacement.
- Preserve distinctions the user made between tools even when a shared
  abstraction is possible.
- Map every active `U` requirement to concrete tasks and completion evidence in
  the traceability table.
- When a rule or requirement is intentionally unsupported by one backend, record
  that disposition explicitly; omission is not an acceptable disposition.
- Before handoff, verify that every active `U` requirement and every extracted
  Poche rule ID still appears in its respective traceability/coverage table.

## Authoritative user guidance ledger

| ID | Active guidance | Required plan consequence |
| --- | --- | --- |
| U1 | Copy `D:\OneDrive\Documents\Ideas\poche\main.typ` into the new branch. | Repository foundation owns `docs/main.typ`; migration is non-destructive until the copy renders and is committed. |
| U2 | GitHub should offer a “click here for pretty view” link while generated PDF and/or HTML output does not inflate Git history. | GitHub Actions builds Typst output and deploys it through GitHub Pages; generated PDF/HTML is not committed. |
| U3 | Model Poche comprehensively in NuSMV, Alloy, Scryer Prolog, and Rust. | Each language has an independent native track, a rule-coverage obligation, native execution, and backend-specific acceptance evidence. |
| U4 | The four tracks need not begin as translations from Rust. Native model files are useful, relatively easy oracle material. | Handwritten native oracle models precede or accompany the shared Facet/Weavy representation; they are not demoted to temporary syntax samples. |
| U5 | Later, formal methods and the Facet ecosystem should verify that the native oracle models agree with the behavior of the Rust model. | Cross-model conformance compares states, legal actions, transitions, queries, and properties. Automatic source generation is optional later work. |
| U6 | Alloy and NuSMV primarily answer whether the game rules are consistent. | Alloy emphasizes structural/bounded consistency; NuSMV emphasizes transition completeness, deadlocks, safety, and temporal/liveness consistency. |
| U7 | Prolog primarily answers “what actions could have led to this state?” and “what actions could be taken from this state?” | The Prolog model exposes relational predecessor, successor, legal-action, rule, and explanation queries with variables in multiple positions. |
| U8 | The initial model-checking purpose is to encode `main.typ` in computerized representations for validation and exploration. | A stable rule catalog extracted from `main.typ` traces every modeled fact, transition, query, and property back to written intent. |
| U9 | Reinforcement learning is a practical later goal with a `State -> Observation -> Action selection via Policy -> Reward -> Policy improvement` cycle (clarified for the current goal by U26). | Make the game-side state, observation, legal-action, chance, transition, scoring, and terminal boundaries explicit now; defer RL-specific policy/reward APIs and training. |
| U10 | RL asks “what is the best way to play the game according to our loss function?” (clarified for Poche by U26 and U28). | Keep future policy improvement empirical unless separately proven; do not present training as model-checking proof, and preserve authoritative game scores independently of any later learning projection. |
| U11 | Rust state should be structurally strong: fixed player/hand cardinalities and phase enums are preferable to weak `Vec`/ECS bags where practical. | Finite/refined types and phase-specific state eliminate invalid shapes before solver assertions; residual relational invariants remain explicit. |
| U12 | Facet, Phon, and Weavy are intended building blocks. | Facet supplies reflection, Phon supplies schemas/interchange, and a restricted Weavy graph supplies inspectable computation; none is assumed to already provide Poche semantics. |
| U13 | Native Alloy, NuSMV, and Scryer installations should be used rather than rebuilding those engines. | Tool runners discover installed binaries and execute native models; no SAT/BDD/WAM reimplementation is required. |
| U14 | Existing `v2` Rust should not constrain the stricter model subset. | The orphan branch remains clean; `v2` is a later black-box/differential reference, not the model source. |
| U15 | Exact nuance must survive future plan edits and context compaction. | This ledger, the rule-coverage matrix, explicit open gates, and completion evidence are mandatory plan surfaces. |
| U16 | LLMs reduce the need for a perfect automatic translation mechanism, although a good mechanism remains desirable. | LLM-authored models/translations are allowed as candidates; native execution and conformance establish evidence, and generation remains a later option. |
| U17 | Rust may be harder to author than Prolog, Alloy, or NuSMV; those languages may be written first as goal material, or all four may iterate in parallel. | The oracle tracks may lead independently and run in parallel after stable rule IDs; Rust is not a mandatory upstream generator. |
| U18 | An opaque Rust function cannot simply be exposed as a virtual function to Prolog because Prolog needs the full computational relation for backtracking. | The strict Rust path constructs an inspectable graph; Prolog receives native relations/rules rather than callbacks for behavior that must run backward. |
| U19 | Cargo target directories interact poorly with OneDrive, motivating Git repositories/worktrees outside OneDrive. | Source and build output stay under the external repo roots; no normal Cargo target is placed in the OneDrive rules/notes folder. |
| U20 | A generated PDF is acceptable for local Typst iteration, but committing it creates disproportionate Git churn because it is much larger than `main.typ`. | Local PDF watching remains supported; public output is built/deployed, and generated document binaries remain ignored. |
| U21 | GitHub Pages must be planned with awareness that the new work is not currently on the repository's default branch. Changing the default branch is acceptable because `model-checking` will be the head of the project. | Verify remote state, push the unborn branch, deliberately make `model-checking` the default/head branch, configure Pages to deploy with GitHub Actions from it, and validate branch/environment settings before advertising the pretty-view link. |
| U22 | Poche is a proving ground for a broader Rust formal-methods engine informed by Alloy, NuSMV, Prolog, Facet, Weavy, and the earlier Vix-core interest. | Keep reusable finite/formal/interchange contracts separate from Poche rules where evidence supports it; record Vix-style incremental orchestration as an explicit later evaluation rather than silently forgetting it. |
| U23 | `D:\OneDrive\Documents\Ideas\model checking` may hold durable Markdown/research notes during brainstorming. | Preserve the existing research note as a source reference; distinguish external exploratory notes from the repository's executable plan and implementation authority. |
| U24 | `Arbitrary`/fuzzing can explore action selection, but stronger proof than sampled assertions is desired. | Keep generated testing as complementary sampled evidence; require exhaustive/bounded/native formal evidence for stronger claims and label confidence explicitly. |
| U25 | The project is in a brainstorming phase. | Preserve reversible working assumptions and explicit gates; do not present recommendations or exploratory architecture as confirmed user decisions. |
| U26 | The current implementation goal excludes reinforcement-learning adapters, policy experiments, and training. RL is a later goal whose shape should be anticipated now so the formal core does not require an avoidable redesign. | Keep RL-specific crates, framework adapters, baselines, and training out of current completion criteria; retain explicit state, observation, legal-action, chance, transition, round-score, and terminal boundaries as future-compatible game contracts. |
| U27 | The new orphan branch and its native/formal model artifacts should use MPL-2.0. | Add an MPL-2.0 root license and consistent package/artifact metadata before importing implementation source. |
| U28 | Poche score at the end of a round is a meaningful future intermediary reward signal because score is the game objective, unlike proxy signals such as chess material. | Preserve round boundaries and raw per-player round-score events explicitly; a later RL goal may project those scores into its training API without inventing reward shaping in the formal rules. |
| U29 | “Comprehensive” means full-rule semantic coverage in all four model tracks, while exhaustive proof claims may be limited to explicitly named finite micro-scopes. | Audit every stable full-game rule in every applicable track; label each bounded/exhaustive result with its scope and use targeted checks or simulation for larger/full-deck configurations. |

## Intent audit evidence

- **Pass 1 — extraction:** Reread every original user message from the initial
  Alloy/NuSMV/Prolog brainstorming request through the current-goal, licensing,
  proof-scope, and future-reward decisions. U1–U25 preserve the original intent;
  U26–U29 preserve the newly confirmed goal boundary, MPL-2.0 choice, round-score
  signal, and comprehensive-versus-exhaustive distinction.
- **Pass 2 — traceability:** Verified every active U1–U29 requirement has one
  authoritative ledger row and one guidance-traceability row. Checked the new
  decisions against scope, gates, execution order, task acceptance criteria,
  deferred-work sections, overall completion, and risks. RL training, legacy-v2
  comparison, and automatic source generation are not hidden completion gates.
- **Pass 3 — adversarial omission:** Reread the user messages after the revised
  plan and searched specifically for scope leakage or weakened qualifiers:
  independent native oracles; Alloy/NuSMV consistency; forward/reverse Prolog;
  full-game rule coverage versus named finite proof scopes; strong Rust shapes;
  future RL compatibility without current RL implementation; end-of-round score
  as the future intermediary signal; MPL-2.0; untracked generated documentation;
  and `model-checking` as the deliberate project head. No remaining omission was
  found.
- **Known source limitation:** None; the original user messages for this planning
  conversation are available in the current context.

## Purpose

Build a portfolio of faithful, executable Poche models derived from
`docs/main.typ`:

- a comprehensive Rust environment with strong finite state/action types;
- a native Alloy model for structural and bounded consistency exploration;
- a native NuSMV model for state-transition and temporal consistency;
- and a native Scryer Prolog model for relational predecessor/successor questions.

The current goal stops after those four models are checked natively, compared
through common evidence, and published with their rulebook. It preserves clean
contracts needed by a future RL goal, but it does not implement RL adapters,
policies, training, automatic target-language generation, or legacy-v2
compatibility.

Poche is the first demanding client of a potentially reusable Rust formal-methods
substrate. Generic finite domains, formal expressions, interchange schemas, and
backend evidence should avoid unnecessary card-game coupling, while Poche rules
remain in Poche-owned crates. Vix-style incremental orchestration is a recorded
future evaluation, not a first-milestone dependency.

The first native models are independent oracle material written in each
language's natural idiom. A shared Facet/Phon/Weavy representation is developed
after their semantics and useful queries are concrete. Its first obligation is
to execute the Rust model and help verify cross-model behavioral agreement, not
to generate the other languages automatically.

The project succeeds only when coverage and confidence are stated precisely:

- “comprehensive model” means every applicable rule in `docs/main.typ` has an
  encoded, tested, or explicitly inapplicable disposition for that track;
- “exhaustive” always names the finite scope explored;
- bounded Alloy results are not generalized beyond their scope;
- fuzzing remains sampled evidence;
- and future learned-policy quality must not be confused with rule consistency.

## Scope

### In scope

- Copying and maintaining `docs/main.typ` in the repository.
- GitHub Actions and Pages publication of PDF and, if viable, HTML output without
  tracking generated document binaries.
- A stable Poche rule catalog with source anchors and cross-track coverage.
- Independent, tracked native oracle models:
  - `models/alloy/`;
  - `models/nusmv/`;
  - `models/prolog/`;
  - Rust crates under `crates/`.
- Native execution with Alloy 6.2.0, NuSMV 2.7.1, and installed Scryer Prolog.
- Strong Rust finite domains, refinements, phase-specific states, and explicit
  legal actions/chance events.
- State, per-agent observation, legal action, chance, transition, round-score,
  outcome, and terminal contracts suitable for a later multi-agent RL adapter.
- A restricted pure Weavy formal dialect and concrete evaluator.
- Facet reflection and Phon interchange for schemas, fixtures, traces, model
  identity, observations, scores/outcomes, bindings, and external-tool results.
- Explicit-state Rust checking for selected finite scopes.
- Safety, deadlock, and finite-state termination/liveness analysis.
- `Arbitrary`/property-based testing as complementary sampled evidence.
- Cross-model fixture and property agreement without assuming one model is
  correct merely because it generated the others.

### Out of scope for the current goal

- Translating arbitrary Rust MIR, LLVM IR, native calls, allocation, I/O, RNG,
  clocks, or side effects into solver logic.
- Reimplementing Alloy, NuSMV, a SAT solver, BDD package, or the Prolog WAM.
- Complete general-purpose `.als`, `.smv`, or `.pl` parsers.
- Exhaustively enumerating every full 52-card Poche execution.
- Claiming a finite-scope result proves all supported player/deck sizes without
  a separate cutoff, induction, or abstraction argument.
- RL framework adapters, episode APIs, baseline policies, policy training, and
  claims about learned policy quality or optimality.
- Automatic Rust/Weavy-to-Alloy/NuSMV/Prolog source generation; handwritten
  oracle models remain the maintained baseline for this goal.
- Trace-level comparison with the legacy `v2` implementation.
- Bevy presentation, online multiplayer, or an ECS runtime.
- Modifying Facet, Weavy, or Phon upstream before local evidence identifies a
  reusable change.
- Integrating Vix-core before parse/lower/check invalidation and interactive
  recomputation become measured needs; the interest remains explicitly deferred.
- Treating the legacy `v2` engine as normative rules.
- Committing generated PDF/HTML output.

## Established foundation

These facts were verified locally on 2026-08-03 and should not be silently
reopened:

- `D:\Repos\Games\poche-3` is an empty Git worktree on the unborn orphan branch
  `model-checking`; `PLAN.md` is its only current file.
- The remote `TeamDman/Poche` repository currently has default branch `main`.
  A read of the Pages REST endpoint returned HTTP 404, so Pages is not configured
  yet. Both facts were verified with `gh` on 2026-08-03.
- The same repository has sibling reference worktrees:
  - `D:\Repos\Games\Poche` at `caece9d` on `v2`;
  - `D:\Repos\Games\poche-2` at `0a19283` on `main`.
- The current rules draft is
  `D:\OneDrive\Documents\Ideas\poche\main.typ`. It specifies 2–51 players,
  52 cards, hands capped at seven, deal/bid/trick/score phases, follow-suit,
  trump, dealer rotation, pot payments, scoring, shared ties, and the `1..m..1`
  hand-size schedule.
- The `v2` reference engine contains useful behavior to compare rather than copy:
  - `crates/game/src/state.rs` owns `State` and `State::tick`;
  - `crates/game/src/rules/rule.rs` dispatches a rule-stack phase machine;
  - `crates/game/src/actions/play_card_action.rs` enumerates card plays;
  - `crates/game/src/actions/place_bet_action.rs` enumerates bids;
  - `crates/game/src/rules/assertions.rs` checks card conservation and phase
    assumptions;
  - `crates/game/tests/bruh.rs` is one seeded four-player smoke run, not
    exhaustive evidence.
- `G:\Programming\Repos\facet\weavy` version `0.2.2` provides
  `Lowered<BlockId, Op>`, a non-recursive runner, and
  `WeavyOp::Intrinsic(Intrinsic)` for caller-defined semantics. It does not
  already define finite Poche logic.
- `G:\Programming\Repos\facet\phon` version `0.2.0-rc.5` distinguishes fixed
  arrays from dynamic lists in its schemas. It does not express semantic
  refinements such as card uniqueness or `bid <= hand_size`.
- Native tools are installed at:
  - `G:\Programming\Caches\CARGO_HOME\bin\scryer-prolog.exe`, reporting
    `v0.10.0-17-ge4d96925`;
  - `C:\Program Files\alloy-6.2.0-windows-amd64\bin\alloy.exe`;
  - `C:\Program Files\NuSMV-2.7.1-win64\bin\NuSMV.exe`, reporting NuSMV 2.7.1.
- No `typst*.exe` was found in the inspected Cargo bin directory and `typst` was
  not discoverable in the current shell. Typst needs an explicit local/CI pin.
- Prior architecture research is recorded at
  `D:\OneDrive\Documents\Ideas\model checking\2026-08-03-rust-formal-methods-engine.md`.

## Confirmed decisions and constraints

- Development starts from the empty orphan branch.
- `docs/main.typ` is normative human intent. No computerized model is assumed
  correct merely because another model was translated from it.
- Rust, Alloy, NuSMV, and Prolog are all intended comprehensive models, with
  tool-specific questions and a common rule-coverage obligation.
- Comprehensive means every stable full-game rule has evidence or an explicit
  applicability disposition in each track. Exhaustive claims are limited to
  explicitly named finite scopes; the first is two players, two suits, three
  ranks per suit, two cards per player, one trump card, and one undealt card.
- Native handwritten oracle models come before automatic cross-language
  generation. Generation remains optional until cross-model comparison proves
  it useful and sufficiently independent, and is excluded from the current goal.
- Rust is the host implementation language, but only an explicit finite/pure
  graph is available to formal backends; arbitrary Rust callbacks remain opaque.
- Facet reflects types; Phon transports typed values and identities; Weavy carries
  executable lowered computation. Semantic refinements remain explicit.
- The full environment state is distinct from a player's observation. Hidden
  cards must not leak through ordinary policy inputs.
- Chance decisions such as deals are distinct from player policy decisions.
- Legal action selection belongs to the environment contract; a policy chooses
  among legal actions but does not define game legality.
- RL implementation is outside the current goal. The formal game nevertheless
  preserves round boundaries and raw per-player end-of-round scores because
  those scores are the intended intermediary reward signal for a later RL goal.
- The repository and its native/generated textual model artifacts use MPL-2.0.
- Local executable paths are not committed. Discovery uses `PATH` and optional
  `ALLOY_BIN`, `NUSMV_BIN`, `SCRYER_PROLOG_BIN`, and `TYPST_BIN` overrides.
- Cargo sources/build targets remain outside OneDrive. `docs/main.pdf` and HTML
  build output are ignored.
- GitHub Pages, not Git history, owns public generated Typst output.
- `model-checking` is intended to become the repository's default/head branch
  after its first commit is pushed. The Pages workflow and `github-pages`
  deployment protection must target that deliberate default branch.
- Results distinguish exhaustive finite checking, bounded checking, sampled
  fuzzing, unsupported behavior, and unknown/tool failure. Future RL evidence
  will remain a distinct empirical category.

## Working assumptions

No unresolved working assumption changes the current goal boundary. Any new
material assumption discovered during implementation must be added to the
guidance ledger or reopened as a gate before dependent work proceeds.

## Design gate dispositions

Decisions go under the affected task and, when durable, in `docs/decisions/`.

| Gate | Status | Required decision | Working recommendation | Acceptance consequence | Blocks |
| --- | --- | --- | --- | --- | --- |
| G1 | Decided | First Rust formal authoring surface: builder, restricted-syntax macro, or handwritten IR. | Typed builder first; macro sugar only after semantics stabilize. | Every supported computation is visibly graph construction and independently interpretable. | 3.3 onward |
| G2 | Decided | Finite-domain/refinement representation beyond Phon schemas. | Canonical `FiniteDomain` plus validators. | Every state/action encoding is finite, deterministic, and rejects invalid bit patterns. | 3.1 onward |
| G3 | Decided | Reproducible Facet/Phon/Weavy dependency source for local and CI builds. | Released versions where sufficient, otherwise exact Git revisions recorded during 1.2; never sibling paths. | Clean clones build without sibling-path assumptions. | 1.2 onward |
| G4 | Decided | Exact first exhaustive card/player/hand scope. | Two players, two suits, three ranks per suit, two cards per player, one trump card, and one undealt card. | All tools share one named micro-scope and its proof claims. | 2.1 onward |
| G5 | Decided | Authority and conflict policy among rulebook, four native models, and later shared graph. | Rulebook states intent; disagreement is explicit; no implementation wins automatically. | Mismatches are classified rather than overwritten. | 2.5, 6.5 |
| G6 | Decided | Meaning of universal termination. | Every path from every initial state reaches absorbing `Finished`; no first-scope stuttering. | Rust SCC and NuSMV properties check the same liveness semantics. | 2.2, 4.2, 5.3, 6.4 |
| G7 | Decided | Root license and treatment of generated/native model artifacts. | MPL-2.0 throughout the repository, with consistent metadata/notices before source import. | Root license, notices, and artifact headers agree. | First source commit |
| G8 | Deferred | First-goal `v2` compatibility. | Excluded from this goal; a later goal may compare traces only after independent models agree. | Legacy structure cannot shape or block the formal core. | Follow-up goal |
| G9 | Decided | Observation visibility, acting-agent identity, and chance-agent semantics. | Per-player observations expose only table-public data plus that player's private hand; chance is explicit. | A later policy adapter cannot receive hidden opponents' cards accidentally. | 3.2, 4.1 |
| G10 | Deferred | Future RL reward API and any additional loss projection. | Preserve raw end-of-round scores as the intended intermediary reward signal; defer RL-specific projection/API to the RL goal. | Formal scoring remains authoritative and is not distorted by premature shaping. | Follow-up goal |
| G11 | Decided | Exact definition of “comprehensive” per native track. | Every stable full-game rule ID has `modeled`, `checked/queried`, or reasoned `not applicable` evidence in each track; exhaustive claims name their finite scope. | Completeness is audited rather than asserted or confused with full-deck enumeration. | 1.4, 2.5, 6.5 |
| G12 | Deferred | Whether automatic Rust/Weavy-to-native-language generation is desirable. | Excluded from this goal; retain handwritten independent oracles and revisit after agreement evidence exists. | Generation cannot replace the independent oracle baseline or block current completion. | Follow-up goal |
| G13 | Decided | Pages formats: PDF, Typst HTML, or both. | Publish PDF and landing page first; add HTML only after fidelity comparison. | README pretty-view link is stable and generated artifacts stay out of Git. | 9.1 |
| G14 | Decided | Remote default-branch and Pages deployment policy for the orphan `model-checking` branch. | After the first push, make `model-checking` the default branch; configure Pages source as GitHub Actions and restrict the `github-pages` environment to the intended default branch. | Repository landing/clone/PR defaults and Pages deployment agree on the project head; branch changes are verified before publishing. | 1.5, 9.1 |

## Source and implementation references

### Local references

- Rules source: `D:\OneDrive\Documents\Ideas\poche\main.typ`
- Legacy state stepping:
  `D:\Repos\Games\Poche\crates\game\src\state.rs`
- Legacy phase dispatch:
  `D:\Repos\Games\Poche\crates\game\src\rules\rule.rs`
- Legacy legal card actions:
  `D:\Repos\Games\Poche\crates\game\src\actions\play_card_action.rs`
- Legacy invariants:
  `D:\Repos\Games\Poche\crates\game\src\rules\assertions.rs`
- Weavy custom intrinsic boundary:
  `G:\Programming\Repos\facet\weavy\src\ir.rs`
- Weavy lowered runner:
  `G:\Programming\Repos\facet\weavy\src\lib.rs`
- Phon schema specification:
  `G:\Programming\Repos\facet\phon\docs\content\spec.md`
- Scryer embedding API:
  `G:\Programming\Repos\scryer-prolog\src\machine\lib_machine\mod.rs`
- Alloy lowering reference:
  `G:\Programming\Repos\org.alloytools.alloy\org.alloytools.alloy.core\src\main\java\edu\mit\csail\sdg\translator\TranslateAlloyToKodkod.java`
- Alloy temporal/Electrod reference:
  `G:\Programming\Repos\org.alloytools.alloy\org.alloytools.alloy.core\src\main\java\edu\mit\csail\sdg\translator\ElectrodPrinter.java`
- NuSMV source reference: `G:\Programming\Repos\NuSMV-2.7.1`

### Authoritative external workflow references

- GitHub Pages publishing sources and custom Actions workflows:
  `https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site`
- GitHub Pages custom workflow requirements:
  `https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages`
- GitHub default-branch behavior and change prerequisites:
  `https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-branches-in-your-repository/changing-the-default-branch`

### Baseline inspection commands

```pwsh
git -C D:\Repos\Games\poche-3 status --short --branch
git -C D:\Repos\Games\poche-3 worktree list
rg -n "pub struct State|fn tick|pub enum Rule|assert_invariants" `
  D:\Repos\Games\Poche\crates\game\src
```

## Guidance traceability

| Guidance | Primary plan coverage |
| --- | --- |
| U1 | 1.3 |
| U2 | 1.3, 9.1 |
| U3 | 2.1–2.5, 4.1–4.4, 6.1–6.5 |
| U4 | 2.1–2.5, G12, deferred generation follow-up |
| U5 | 3.1–3.4, 4.4, 6.1–6.5 |
| U6 | 2.1, 2.2, 6.3, 6.4 |
| U7 | 2.3, 6.2 |
| U8 | 1.4, 2.5, 6.5 |
| U9 | 3.2, 4.1–4.2, deferred RL follow-up |
| U10 | G10, 3.2, deferred RL follow-up |
| U11 | 3.1, 4.1 |
| U12 | 3.1–3.4, 6.1–6.5 |
| U13 | 1.2, 2.1–2.3, 6.1 |
| U14 | G8, deferred legacy follow-up |
| U15 | Guidance protocol, 1.4, 2.5, 6.5, 9.3 |
| U16 | 2.1–2.5, 6.5, G12, deferred generation follow-up, 9.2 |
| U17 | Execution order, 2.1–2.5, 4.1–4.4 |
| U18 | 2.3, 3.3, 6.2, deferred generation follow-up |
| U19 | 1.2, confirmed constraints |
| U20 | 1.3, 9.1 |
| U21 | Established foundation, G14, 1.5, 9.1, overall completion criteria |
| U22 | Purpose, scope non-goal, 3.1–3.4, 9.2 |
| U23 | Established foundation and source references |
| U24 | Scope, 5.1–5.4, overall completion criteria |
| U25 | Plan status, working assumptions, G1–G14 |
| U26 | Current-goal boundary, scope, 3.2, 4.1–4.2, deferred RL follow-up, overall completion criteria |
| U27 | Confirmed decisions, G7, 1.1, overall completion criteria |
| U28 | G10, 2.4, 3.2, 4.2, deferred RL follow-up, overall completion criteria |
| U29 | Purpose, scope, G4, G11, 1.4, 2.1–2.5, 5.1–5.4, 6.5, overall completion criteria |

## Execution order

```text
Repository + rules + stable rule IDs
  -> push orphan branch, establish deliberate default branch, configure Pages policy
  -> independent native oracle tracks (Alloy, NuSMV, Prolog, Rust)
  -> finite/observation/score/interchange contracts
  -> strict Facet/Weavy Rust environment
  -> exhaustive Rust safety and liveness checking
  -> cross-model agreement using native tools and common fixtures
  -> Pages publication, contributor documentation, final coverage audit
```

The native oracle tracks may proceed in parallel after the rule catalog exists.
They must not copy one another mechanically: independence is useful because
agreement among differently expressed models provides stronger evidence than
checking a generator against its own output.

## Modeling and acceptance matrix

| Track | Primary questions | Comprehensive coverage obligation | Native validation | Evidence |
| --- | --- | --- | --- | --- |
| Typst rules | What game is intended? | Every normative rule has a stable rule ID and source anchor. | Pinned Typst render | Pending |
| Rust oracle/environment | What are valid states, observations, legal actions, transitions, round scores/outcomes, and terminal outcomes? | Every applicable rule ID has executable examples/properties or an explicit disposition. | Rust tests, explicit exploration, replay | Pending |
| Alloy 6.2.0 | Do structurally valid deals/states/traces exist, and do bounded assertions find contradictions or counterexamples? | Every structural/relational rule ID is modeled or explicitly inapplicable; selected temporal facts may be included. | Native Alloy runs with scopes recorded | Pending |
| NuSMV 2.7.1 | Can phases deadlock, violate invariants, or avoid termination? Are temporal rules mutually consistent? | Every state/transition/temporal rule ID is represented or explicitly inapplicable. | Native NuSMV invariant/CTL/LTL runs and traces | Pending |
| Scryer Prolog | Which actions are legal now, which predecessor actions/states can explain this state, and how are rule conclusions derived? | Every relational/action/scoring rule ID has forward and, where meaningful, reverse queries. | Native Scryer query corpus and answer sets | Pending |
| Full 52-card game | How does the real game behave? | All rules are expressible, but exhaustive claims are limited to named abstractions/scopes. | Simulation/fuzzing plus targeted symbolic checks | Pending |

## Phase 1 — Repository, rules, and traceability foundation

### [x] 1.1 Record architecture, completeness, Pages, and licensing decisions

**Work:**

- Record the decided/deferred disposition of G1–G14 before dependent work.
- Record durable decisions under `docs/decisions/`.
- Preserve rejected alternatives and acceptance consequences.
- Add the MPL-2.0 root license and matching package metadata before importing
  implementation source.

**Validation:**

```pwsh
$open = rg -n "^\| G(?:[1-9]|1[0-4]) \| Open \|" PLAN.md docs/decisions 2>$null
if ($LASTEXITCODE -eq 0) { $open; throw "Architecture gates remain open" }
```

**Completion criteria:** G1–G14 retain their decisions/deferred dispositions and
evidence consequences; a fresh agent can identify the current-goal boundary and
which choices govern every downstream track.

**Completion notes (2026-08-03):** `docs/decisions/0001-initial-modeling-decisions.md`
records every gate and consequence. `LICENSE` contains the full MPL-2.0 text;
workspace/package metadata uses the SPDX identifier `MPL-2.0`. A gate scan found
zero `Open` rows. Deferred G8, G10, and G12 are explicitly outside this goal and
therefore are not hidden blockers.

### [x] 1.2 Scaffold the reproducible workspace and native-tool doctor

**Work:**

- Add root workspace metadata, toolchain policy, `.gitignore`, `README.md`, and
  minimal `poche-xtask` infrastructure.
- Pin Facet/Phon/Weavy reproducibly without committed sibling-directory paths.
- Discover native tools through `PATH` or documented environment overrides.
- Report exact versions and distinguish unavailable from failed.

**Validation:**

```pwsh
cargo metadata --format-version 1 --no-deps
cargo test --workspace
cargo run -p poche-xtask -- doctor
```

**Completion criteria:** A clean clone resolves dependencies and reports Rust,
Alloy, NuSMV, Scryer, and Typst status without machine-specific committed paths.

**Completion notes (2026-08-03):** Added the Rust 1.96.0 workspace,
`poche-xtask`, `Cargo.lock`, ignore policy, README, native-tool documentation,
and `tools/versions.toml`. Published crates.io versions are pinned exactly:
Facet 0.50.0-rc.5, Phon 0.2.0-rc.5, and Weavy 0.2.2. `cargo metadata`, workspace
tests, formatting, and Clippy passed. `doctor` reported Rust 1.96.0, Alloy 6.2.0,
NuSMV 2.7.1, Scryer `v0.10.0-17-ge4d96925`, and Typst 0.15.1 as available when
given machine-local overrides. It also distinguishes missing tools from broken
explicit overrides and handles NuSMV's version-bearing `-h` exit status 2.

### [x] 1.3 Copy, render, and establish repository ownership of `main.typ`

**Work:**

- Copy the OneDrive rules to `docs/main.typ` without deleting the original until
  the repository copy renders and is committed.
- Ignore `docs/main.pdf` and generated HTML/SVG/Page output.
- Pin/install a local and CI Typst version.
- Document that Pages artifacts, not Git blobs, provide the public pretty view.

**Validation:**

```pwsh
& $env:TYPST_BIN compile docs/main.typ "$env:TEMP\poche-rules.pdf"
git check-ignore docs/main.pdf
git ls-files "*.pdf"
```

**Completion criteria:** `docs/main.typ` renders reproducibly, no generated PDF
is tracked, and the migration did not destructively remove the prior source.

**Completion notes (2026-08-03):** Copied the OneDrive source byte-for-byte:
both files have SHA-256
`74AFF8F332F89D9EB4EA31B9898A5D87B7F8741D9767EC7FA79AD810164ADCDD`.
Official Typst 0.15.1 compiled it into an ignored five-page local PDF (192,974
bytes). All five pages were rendered to PNG and visually inspected; headings,
columns, tables, glyphs, and callouts were legible with no clipping or overlap.
The template warns that TeX Gyre Termes/Cursor are absent locally, so Task 9.1
must install those fonts in CI rather than silently depending on fallback fonts.
The original OneDrive file remains untouched, `docs/main.pdf` is ignored, and no
PDF is tracked.

### [x] 1.4 Extract stable rule IDs and create the cross-track coverage ledger

**Work:**

- Create `docs/rules-coverage.md` with one stable ID per normative rule or tightly
  coupled rule group in `docs/main.typ`.
- Record source section/anchor, plain-language meaning, and applicability to
  Rust, Alloy, NuSMV, and Prolog. Record future-RL relevance separately when a
  rule affects observation, action, chance, round scoring, or termination.
- Require each applicable cell to become `modeled`, `validated`, or explicitly
  `deferred`; use reasoned `not applicable` only when the tool's role genuinely
  does not cover the rule.
- Add an audit command that detects missing active guidance IDs and incomplete
  rule dispositions.

**Validation:**

```pwsh
cargo run -p poche-xtask -- coverage audit
rg -n "U(?:[1-9]|1[0-9]|2[0-9])" PLAN.md
```

**Completion criteria:** Every normative rule has an immutable ID/source anchor,
all four model tracks have explicit dispositions, future-RL-relevant boundaries
are flagged, and all U1–U29 requirements remain traceable.

**Completion notes (2026-08-03):** `docs/rules-coverage.md` defines 61 stable
rule IDs with precise `docs/main.typ` line anchors, Rust/Alloy/NuSMV/Prolog
dispositions, future-RL relevance, and explanatory notes. It explicitly
distinguishes full-rule comprehensiveness from named-scope exhaustiveness and
marks physical/documentary non-applicability with reasons. `poche-xtask coverage
audit` validates row structure, IDs, anchors, disposition vocabulary, and the
paired U1–U29 guidance ledger/traceability rows. The structural audit passed and
truthfully reported current `todo` counts (Rust 57, Alloy 53, NuSMV 53, Prolog
56); strict `--track`/`--all` modes fail until selected model evidence replaces
those cells.

### [x] 1.5 Establish the remote default branch and Pages deployment policy

**Work:**

- Create the first local commit and push the unborn `model-checking` branch
  before attempting any remote default-branch change.
- Re-read the remote default branch, open pull requests, branch protection, and
  repository rulesets that could be affected.
- With explicit execution authorization, change the remote default branch from
  `main` to `model-checking`; do not delete `main` as part of this task.
- Configure Pages publishing source as GitHub Actions rather than a committed
  build-output branch.
- Configure the `github-pages` environment/deployment policy so only the intended
  default/head branch can publish production rules output.
- Make future Pages workflow triggers name `model-checking` explicitly or derive
  the verified default-branch policy without assuming `main`.

**Validation:**

```pwsh
gh repo view TeamDman/Poche --json defaultBranchRef,url,visibility
gh api repos/TeamDman/Poche/pages
gh api repos/TeamDman/Poche/rulesets
gh pr list --repo TeamDman/Poche --state open --json number,baseRefName,headRefName
```

**Completion criteria:** The remote branch exists, `defaultBranchRef.name` is
`model-checking`, Pages reports an Actions/workflow publishing configuration,
affected protections/PR bases are recorded or updated deliberately, and `main`
has not been destructively removed.

**Completion notes (2026-08-03):** After pushing foundation commit `765cb9c`,
GitHub preflight showed default `main`, zero open PRs, no rulesets or branch
protection, no Pages site, and no environments. Changed the default to
`model-checking`, created Pages in workflow mode at
`https://teamdman.github.io/Poche/`, and verified HTTPS enforcement. GitHub
created a `github-pages` environment with custom deployment policy restricted to
`model-checking`. Actions are enabled; the default workflow token is read-only.
The untouched `main` branch remains at `0a19283`. Full evidence and rollback
steps are in `docs/decisions/0002-github-head-and-pages-policy.md`.

## Phase 2 — Independent native oracle models

### [x] 2.1 Build the comprehensive Alloy oracle model

**Work:**

- Author tracked `.als` modules directly from the rule catalog, using Alloy's
  relational idioms rather than generated Rust-shaped syntax.
- Model players, seat order, cards, hands, deck/trump, bids, tricks, ownership,
  phases, scoring outcomes, and bounded traces where useful.
- Add `run` commands proving intended structures exist and `check` assertions for
  structural consistency, card partition/conservation, legal play, winner rules,
  score relationships, and other Alloy-applicable rule IDs.
- Record exact scopes and avoid claiming scope-independent proof.

**Validation:**

```pwsh
cargo run -p poche-xtask -- oracle check alloy
cargo run -p poche-xtask -- coverage audit --track alloy
```

**Completion criteria:** Alloy 6.2.0 natively accepts the model; every applicable
rule ID has a fact/predicate/assertion and recorded result or explicit limitation.

**Completed 2026-08-03:** `models/alloy/poche.als` is an independent relational
oracle over the complete 52-card deck, cyclic seats, parametric schedule,
complete-round snapshots, phase traces, scoring/money separation, final winners,
First Jack, and repeated-tie High Card. The native Alloy 6.2.0 runner completed
seven satisfiable witnesses and eight assertions with no counterexample using
seven-bit integers, the exact full deck, two-player/two-trick round scopes, and
three-player selection/winner scopes as recorded in every command and generated
receipt. `docs/alloy-oracle.md` records the proof boundary and intentional atomic
deal abstraction. Strict Alloy coverage reports zero `todo` cells across all 61
rules; physical and documentary exclusions retain reasons.

### [x] 2.2 Build the comprehensive NuSMV oracle model

**Work:**

- Author tracked `.smv` modules directly from the phase/transition rules.
- Model state/input/chance variables, initial states, legal nondeterministic
  transitions, absorbing termination, and any decided fairness semantics.
- Add invariant, deadlock, CTL/LTL, and termination properties for all applicable
  rule IDs.
- Preserve and normalize counterexample traces.

**Validation:**

```pwsh
cargo run -p poche-xtask -- oracle check nusmv
cargo run -p poche-xtask -- coverage audit --track nusmv
```

**Completion criteria:** NuSMV 2.7.1 checks the model natively; safety and
termination meanings are explicit, and every applicable rule ID has evidence.

**Completed 2026-08-03:** `models/nusmv/poche.smv` independently models the
complete two-player thirteen-round transition system with explicit environment
inputs, lifecycle phases, dealer/leader rotation, bids, abstract legal card
attributes, conserved 52-card zone counts, trick winners, scoring, money, and an
absorbing terminal state. A separate symbolic `2..51` parameter checks maximum
hand, feasibility, and round-count formulas. Native NuSMV 2.7.1 with sound
cone-of-influence reduction passed 46 invariant/CTL/LTL properties, including
`AG EX TRUE`, `AF finished`, and `F finished`, without fairness. The runner
preserves the full native log/counterexample and normalized property evidence
under ignored `target/nusmv-oracle/`. Strict NuSMV coverage has zero `todo`
cells; `docs/nusmv-oracle.md` states the exact card-identity abstraction.

### [x] 2.3 Build the comprehensive Scryer Prolog oracle model

**Work:**

- Author tracked `.pl` modules for valid states, legal actions, transitions,
  trick winners, scoring, and derived rule explanations.
- Support forward queries such as `legal_action(State, Action)` and
  `step(State, Action, Next)`.
- Support reverse queries such as `step(Previous, Action, State)` within declared
  finite/domain constraints, plus focused “what could have led here?” relations.
- State where modes, tabling, finite constraints, or bounded history are required
  to avoid unproductive infinite search.

**Validation:**

```pwsh
cargo run -p poche-xtask -- oracle check prolog
cargo run -p poche-xtask -- coverage audit --track prolog
```

**Completion criteria:** Installed Scryer loads the program and the query corpus
returns expected forward/reverse answer sets for every applicable rule ID.

**Completed 2026-08-03:** `models/prolog/poche.pl` independently defines finite
relations for all player schedules, the standard deck, clockwise deals, legal
bids/plays, trick winners, bid scoring in both directions, final winners/money,
First Jack, repeated-tie High Card, card restoration, and semantic score rows.
Its full-identity two-player one-card round supports `legal_action/2`, `step/3`,
`predecessor/3`, and `replay/3`. Installed Scryer passed a 16-query corpus,
including complete forward replay, per-state card conservation, reverse bid and
play predecessor recovery, and a reverse score query whose sole cause is `3-3`.
The runner requires an explicit success marker because Scryer may report an
uncaught goal error with exit status zero, and preserves the native transcript.
Strict Prolog coverage has zero `todo` cells; `docs/prolog-oracle.md` records
productive modes and the one-round transition bound.

### [x] 2.4 Build the comprehensive conventional Rust oracle environment

**Work:**

- Implement the clearest finite executable environment before forcing every
  computation through the formal Weavy subset.
- Define full state, per-player observation, actions, chance events, transition
  outcomes, per-player round scores, game outcomes, and terminal state contracts.
- Use fixed/refined types and phase enums where practical; keep constructors
  private when validity must be preserved.
- Enumerate legal actions explicitly and support deterministic trace replay.

**Validation:**

```pwsh
cargo test -p poche-oracle-rust
cargo run -p poche-xtask -- oracle check rust
cargo run -p poche-xtask -- coverage audit --track rust
```

**Completion criteria:** Every Rust-applicable rule ID has executable behavior
and focused tests; the environment can replay fixed deals/actions deterministically.

**Completion notes (2026-08-03):** Added `poche-oracle-rust`, an independent
const-generic model supporting every rulebook player count from 2 through 51.
Semantic state uses fixed arrays/fixed-capacity card zones and phase-specific
variants for deal, bid, play, score, and finished states. It exposes explicit
chance deals, player actions, environment settlement, per-player observations,
raw round-score events, money separately from score, deterministic replay,
First-Jack/High-Card selection, card conservation, follow-suit legality, trump
winner selection, the parametric schedule, dealer rotation, shared winners, and
pot division with explicit remainder. Ten focused tests passed, including
complete deterministic games for 2 and 51 players. `poche-xtask oracle check
rust` completed a 13-round two-player trace in 150 transitions with final scores
`[60, 70]` and pot 180 cents. Strict Rust coverage reports 0 `todo` cells;
workspace formatting, tests, and Clippy with `-D warnings` pass.

### [x] 2.5 Audit oracle completeness without forcing premature agreement

**Work:**

- Review all native models against `docs/rules-coverage.md`.
- Record known disagreements and underspecification without changing one model
  merely to match another.
- Classify gaps as missing rule encoding, tool-role limitation, scope limitation,
  ambiguous written rule, or expected semantic difference.
- Create the initial shared scenario/query/property fixture inventory.

**Validation:**

```pwsh
cargo run -p poche-xtask -- coverage audit --all
cargo run -p poche-xtask -- oracle report
```

**Completion criteria:** All four native models meet the G11 completeness policy;
remaining disagreements are explicit inputs to Phase 6 rather than omissions.

**Completed 2026-08-03:** `docs/oracle-audit.md` confirms G11 coverage across
all 61 rules and four tracks, records each oracle's scope/evidence strength, and
classifies 11 explicit scope, tool-role, expected-semantic, or written-rule
differences with zero missing-rule gaps. `fixtures/oracle-inventory.toml` seeds
15 shared scenarios, four properties, and three reverse/action queries without
claiming Phase 6 serialization or agreement prematurely. `oracle report`
validates all native/model documentation artifacts, performs the strict
four-track coverage audit, checks the inventory/audit shape, and passes.

## Phase 3 — Shared finite, formal, environment, and interchange contracts

### [x] 3.1 Implement finite domains and refinement-safe encodings

**Work:**

- Define canonical finite domains for enums, bounded integers, fixed arrays,
  tuples, options, finite sets, and card partitions.
- Layer semantic refinements over Facet/Phon shapes for nonempty player counts,
  bounded bids, unique cards, and valid indices.
- Provide deterministic enumeration, cardinality, encode/decode, and validation.
- Reject invalid bit patterns rather than silently normalizing them.

**Validation:**

```pwsh
cargo test -p poche-domain finite_domain
cargo test -p poche-domain refinement
cargo test -p poche-domain phon_roundtrip
```

**Completion criteria:** Every selected-scope state/action component has a
canonical finite encoding and exhaustive round-trip/refinement tests.

**Completed 2026-08-03:** `poche-domain` defines dense `FiniteDomain`
cardinality/enumeration/encode/decode contracts for bounded integers, options,
pairs, fixed arrays, finite sets, indices, suits/ranks/cards, distinct card
sequences, and the G4 phase/turn/action/score components. The six-card G4
partition accepts exactly two cards per player, one trump, and one undealt card
and densely enumerates all 180 valid partitions rather than normalizing invalid
assignments. Facet supplies wire reflection and Phon round trips the selected
scope wire, after which player count, bid, unique cards, and partition are
revalidated. All specified filtered tests and Clippy with `-D warnings` pass;
every first spare code and malformed refinement is rejected.

### [ ] 3.2 Define the environment, observation, chance, and scoring boundary

**Work:**

- Define a game-environment contract containing `State`, `AgentId`,
  `Observation`, `Action`, `ChanceAction`, `TransitionOutcome`, `RoundScore`,
  game outcome, and terminal status.
- Make `observe(State, AgentId)` explicit so a future policy adapter need not
  receive the full hidden state.
- Keep `legal_actions` and game transition semantics independent of any future
  action-selection policy.
- Preserve raw per-player end-of-round scores, cumulative score/money outcomes,
  and their rule origins. Do not add an RL-specific reward projection in this
  goal.
- Define deterministic seeding/replay for chance without placing RNG state in
  the formal game state.

**Validation:**

```pwsh
cargo test -p poche-environment contract
cargo test -p poche-environment observation_visibility
cargo test -p poche-environment round_score_events
```

**Completion criteria:** Formal checking and simulation share one environment
boundary; hidden information is explicit, and a later RL adapter can consume
round scores without changing formal game semantics.

### [ ] 3.3 Define a pure Poche formal dialect over Weavy

**Work:**

- Define local Weavy intrinsics for typed constants/inputs, records/enums,
  equality/order, Boolean logic, conditionals, fixed indexing, and fixed-bound
  quantification/folding.
- Represent state, observation, legal-action, transition, scoring, and property
  expressions plus source/rule origins.
- Validate and reject opaque host calls, effects, dynamic allocation, unbounded
  iteration, hidden RNG, and unsupported arithmetic.
- Implement the G1 authoring surface without hiding unsupported Rust behavior.

**Validation:**

```pwsh
cargo test -p poche-formal expression_eval
cargo test -p poche-formal lowering_validation
cargo test -p poche-formal origin_mapping
```

**Completion criteria:** The complete supported computation graph can be
interpreted independently of arbitrary Rust, with source-oriented diagnostics.

### [ ] 3.4 Define Phon model, fixture, trace, observation, and result schemas

**Work:**

- Define schemas for model identity, scopes, states, observations, actions,
  chance actions, round scores/outcomes, transitions, state diffs, traces, Prolog bindings,
  solver results, statistics, and raw diagnostics.
- Carry rule IDs, schema IDs, semantic hashes, model/oracle revision, scope,
  backend, property/query ID, and confidence kind.
- Ensure fixtures can be consumed independently by native tool adapters.

**Validation:**

```pwsh
cargo test -p poche-interchange phon_roundtrip
cargo test -p poche-interchange semantic_identity
cargo test -p poche-interchange fixture_compatibility
```

**Completion criteria:** Cross-model evidence cannot be mistaken for another
scope, rule revision, scoring contract, observation contract, or proof strength.

## Phase 4 — Strict Facet/Weavy Rust model

### [ ] 4.1 Model strong phase-specific state, observations, and actions

**Work:**

- Implement the first finite scope with phase-specific state variants rather
  than optional-field or ECS bags.
- Represent fixed player/hand/card cardinalities and card partitions strongly.
- Distinguish full state, public table information, each player's private hand,
  legal player actions, and chance actions.
- Keep policy, UI, I/O, clocks, and RNG outside semantic state.

**Validation:**

```pwsh
cargo test -p poche-model state_domain_is_finite
cargo test -p poche-model observation_schema
cargo test -p poche-model action_enumeration
```

**Completion criteria:** The raw domain is finite/measurable, invalid local shapes
are excluded where practical, and policy observations contain only allowed data.

### [ ] 4.2 Encode initial, legal, transition, terminal, and scoring computations

**Work:**

- Encode deal, trump, bid, play, trick winner, scoring, pot, dealer/hand schedule,
  and terminal progression from rule IDs.
- Expose every player/chance choice explicitly.
- Make `Finished` absorbing under G6 and prohibit accidental stuttering.
- Compute observations, per-round score events, and cumulative outcomes from
  transitions.
- Attach rule/source origins to decisions and diffs.

**Validation:**

```pwsh
cargo test -p poche-model transition_examples
cargo test -p poche-model follow_suit_examples
cargo test -p poche-model scoring_examples
cargo test -p poche-model terminal_and_scoring
```

**Completion criteria:** Every Rust-applicable rule has an executable formal
computation and focused examples agree with the rule catalog.

### [ ] 4.3 Declare safety, consistency, and liveness properties

**Work:**

- Declare card conservation/partition, legal actor, observation confidentiality,
  fixed bids, follow-suit, trick completeness/winner, leader rotation,
  trick-count conservation, scoring/pot correctness, phase progress, deadlock
  freedom, and universal termination.
- Mark structurally guaranteed facts separately from state-space properties.
- Map every property to rule IDs and applicable native oracle checks.

**Validation:**

```pwsh
cargo test -p poche-model property_catalog
cargo run -p poche-xtask -- coverage audit --track rust
```

**Completion criteria:** Every claim has a stable ID, formal expression,
rule/source mapping, explanation, and declared verification strategy.

### [ ] 4.4 Compare conventional and strict Rust models

**Work:**

- Replay the shared fixture corpus against the Phase 2 Rust oracle and strict
  Facet/Weavy model.
- Compare observations, legal action sets, transition outcomes, round scores,
  cumulative outcomes, and terminal status.
- Classify discrepancies rather than assuming the stricter model wins.

**Validation:**

```pwsh
cargo test -p poche-conformance rust_models
cargo run -p poche-xtask -- compare rust-oracle rust-formal
```

**Completion criteria:** The agreed fixture corpus matches or carries explicit
classified discrepancies tied to rule IDs and decisions.

## Phase 5 — Exhaustive Rust checking and sampled testing

### [ ] 5.1 Implement deterministic explicit-state exploration

**Work:**

- Enumerate every initial state and legal player/chance action for the selected
  scope.
- Implement BFS with canonical hashing, predecessor/action records, and shortest
  counterexample reconstruction.
- Report state/transition counts, depth, duplicates, and termination reason.
- Add symmetry only after equivalence validation.

**Validation:**

```pwsh
cargo test -p poche-check explicit_small_graphs
cargo test -p poche-check shortest_counterexample
cargo run -p poche-xtask -- check rust-explicit --scope micro
```

**Completion criteria:** Runs are deterministic and a known false invariant
produces the expected shortest trace.

### [ ] 5.2 Exhaustively check the safety catalog

**Work:**

- Check every reachable-state safety/consistency property.
- Add injected follow-suit, trick-winner, observation-leak, and scoring defects.
- Emit common Phon counterexamples with state/observation diffs and rule origins.

**Validation:**

```pwsh
cargo test -p poche-check safety_catalog
cargo test -p poche-check injected_defects
```

**Completion criteria:** The micro-model passes every safety claim and each
injected defect yields a readable, relevant counterexample.

### [ ] 5.3 Check deadlocks and universal termination

**Work:**

- Detect reachable nonterminal deadlocks.
- Compute SCCs and any cycle capable of avoiding `Finished` forever.
- Emit prefix-plus-cycle lassos.
- Where useful, define a lexicographic progress measure matching the phase model.

**Validation:**

```pwsh
cargo test -p poche-check liveness_small_graphs
cargo test -p poche-check nonterminating_lasso
cargo run -p poche-xtask -- check rust-explicit --property game-terminates
```

**Completion criteria:** Universal termination is proven for the named scope or a
replayable lasso is reported; injected stuttering is detected.

### [ ] 5.4 Add complementary `Arbitrary` and larger-scope tests

**Work:**

- Generate valid initial states and legal player/chance traces through the
  environment contract.
- Compare concrete and formal transition evaluation.
- Exercise larger/full-deck scenarios and label all results sampled.
- Reuse failing seeds as fixed regression traces.

**Validation:**

```pwsh
cargo test -p poche-model proptest_transition_equivalence
cargo test -p poche-model proptest_larger_scopes
```

**Completion criteria:** Generated tests respect refinements, reproduce failures,
and cannot be confused with exhaustive or bounded proofs.

## Phase 6 — Native tool integration and cross-model agreement

### [ ] 6.1 Implement native runners and normalized fixture adapters

**Work:**

- Run handwritten oracle files with the installed Alloy, NuSMV, and Scryer tools.
- Preserve raw stdout/stderr and fail closed on unknown output.
- Convert common fixtures to each native model's input conventions and normalize
  answers/instances/traces without requiring generated model source.

**Validation:**

```pwsh
cargo test -p poche-native-tools
cargo run -p poche-xtask -- oracle check all
```

**Completion criteria:** Every installed native tool executes its independent
model reproducibly and returns typed success/failure/unknown diagnostics.

### [ ] 6.2 Verify Prolog predecessor/successor agreement

**Work:**

- Compare Rust and Prolog legal-action and next-state answer sets.
- Compare bounded predecessor `(Previous, Action)` explanations for target states.
- Compare trick winner, scoring, and rule explanation relations.
- Normalize answer order and record intentional mode/constraint limits.

**Validation:**

```pwsh
cargo test -p poche-conformance prolog
cargo run -p poche-xtask -- compare rust prolog --fixtures tests/fixtures/prolog
```

**Completion criteria:** Applicable query answer sets agree for the complete
fixture corpus or carry explicit rule-linked discrepancies.

### [ ] 6.3 Verify Alloy structural and bounded agreement

**Work:**

- Compare valid/invalid structural fixtures, relations, and bounded assertions.
- Compare selected state/transition instances where both models expose them.
- Preserve Alloy scope information in every result.
- Inject known structural defects to prove assertions are discriminating.

**Validation:**

```pwsh
cargo test -p poche-conformance alloy
cargo run -p poche-xtask -- compare rust alloy --scope micro
```

**Completion criteria:** Rust and Alloy agree on applicable fixtures/properties
within named scopes, and discrepancies are classified rather than hidden.

### [ ] 6.4 Verify NuSMV transition and temporal agreement

**Work:**

- Compare initial states, legal one-step transitions, invariants, deadlocks, and
  universal termination outcomes.
- Normalize NuSMV counterexamples and compare their observable projections to
  Rust traces/lassos.
- Inject matching liveness defects in controlled fixtures.

**Validation:**

```pwsh
cargo test -p poche-conformance nusmv
cargo run -p poche-xtask -- compare rust nusmv --scope micro
```

**Completion criteria:** NuSMV and Rust agree on the named temporal semantics and
injected defects produce corresponding counterexamples.

### [ ] 6.5 Complete the cross-model rule and behavior audit

**Work:**

- Audit every rule ID across Rust, Alloy, NuSMV, and Prolog.
- Compare common ground evaluations, legal actions, transitions, properties,
  queries, observations where applicable, and counterexample projections.
- Preserve tool-specific strengths rather than forcing identical surface APIs.
- Update the acceptance matrix with native commands, versions, scopes, hashes,
  counts, and limitations.

**Validation:**

```pwsh
cargo run -p poche-xtask -- coverage audit --all
cargo run -p poche-xtask -- compare all --scope micro
cargo test --workspace
```

**Completion criteria:** Every rule/track cell has evidence or an explicit
disposition, and every advertised agreement claim is reproducible.

## Deferred follow-up — Automatic target-language generation

This section is retained as future context only. It is not a current-goal task
or completion obligation.

### Future generation feasibility study

**Possible future work:**

- Select a representative subset of the already-agreed model graph.
- Prototype at most one target lowering without replacing the handwritten oracle.
- Measure semantic coverage, readability, source mapping, maintenance cost, and
  whether cross-checking remains sufficiently independent.
- Record a decision to adopt, defer, or reject generation separately for Alloy,
  NuSMV, and Prolog.

**Possible future validation:**

```pwsh
cargo test -p poche-generation-spike
cargo run -p poche-xtask -- generation report
```

**Future exit criterion:** Decide separately for Alloy, NuSMV, and Prolog whether
generation is worth adopting; never silently overwrite or reclassify a
handwritten oracle.

## Deferred follow-up — Reinforcement learning

This entire section is non-executable context for a later goal. The current goal
only preserves the game-level boundaries it will need: per-agent observation,
legal actions, explicit chance, deterministic transitions/replay, round-score
events, cumulative outcomes, and terminal state.

End-of-round per-player score is the intended intermediary reward signal because
Poche score directly measures the game objective. A future RL goal may define
additional versioned projections, but must not change or obscure raw scoring.

### Future: validate observations and information boundaries

**Work:**

- Test each phase's per-player observation against allowed public/private data.
- Define observational equivalence for hidden opponent hands where applicable.
- Ensure legal-action masks do not leak hidden information beyond game rules.
- Keep privileged full-state observations available only to explicitly named
  debugging/centralized-training interfaces.

**Validation:**

```pwsh
cargo test -p poche-environment observation_noninterference
cargo test -p poche-environment legal_mask_visibility
```

**Completion criteria:** Ordinary policies cannot distinguish states that should
be observationally equivalent for their player.

### Future: provide a deterministic multi-agent episode adapter

**Work:**

- Expose reset, acting agent/chance, observation, legal actions/mask, step,
  rewards, terminal/truncated, and replay/seed operations.
- Support self-play and separately named policies per seat.
- Serialize episodes through Phon with model/reward/observation version IDs.
- Keep framework-specific adapters outside the core environment contract.

**Validation:**

```pwsh
cargo test -p poche-rl episode_contract
cargo test -p poche-rl deterministic_replay
```

**Completion criteria:** Identical seeds/chance actions/policies reproduce the
same episode and any episode can be replayed through the validated Rust model.

### Future: establish reward/loss versions and baseline policies

**Work:**

- Use per-player end-of-round score as the primary intermediary reward signal;
  preserve cumulative score, pot/money outcome, win/share/tie, and any later
  training projection as distinct outputs.
- Implement random-legal and simple rule-based policies.
- Evaluate baselines across fixed and randomized scenario suites.
- Report legality, return distribution, score/win metrics, and confidence—not
  only one aggregate reward.

**Validation:**

```pwsh
cargo test -p poche-rl reward_versions
cargo run -p poche-xtask -- rl evaluate --policies random,heuristic
```

**Completion criteria:** Baselines are reproducible and every reported objective
names its reward/loss version and evaluation distribution.

### Future: train and evaluate an initial policy without overstating proof

**Work:**

- Select a practical learning algorithm/framework only after the environment
  contract and baselines stabilize.
- Train first on the finite micro-game, then evaluate transfer to larger scopes.
- Verify all selected actions are legal and replay failures through formal traces.
- Clearly separate empirical policy improvement from formal rule/property proof.

**Validation:**

```pwsh
cargo run -p poche-xtask -- rl train --config configs/rl/micro.toml
cargo run -p poche-xtask -- rl evaluate --checkpoint artifacts/rl/micro
cargo run -p poche-xtask -- rl replay-failures
```

**Completion criteria:** The learned policy is reproducibly evaluated against
baselines, illegal-action attempts are zero or explicit failures, and no global
optimality claim is made without separate evidence.

## Deferred follow-up — Legacy comparison without legacy ownership

This entire section is non-executable context for a later goal. It cannot block
completion of the independent formal models or shape their public contracts.

### Future: define a trace-level `v2` comparison boundary

**Work:**

- Define which formal states/actions have meaningful full-deck `v2` equivalents.
- Prefer Phon traces/golden observations over a source dependency on `v2` types.
- Resolve disagreements against rule IDs, not legacy implementation precedence.

**Validation:**

```pwsh
cargo test -p poche-compat-v2 trace_mapping
cargo run -p poche-xtask -- compare-v2 --fixtures tests/fixtures/v2
```

**Completion criteria:** Selected traces replay without making `v2` structure
part of the new public model contract.

### Future: classify and resolve legacy differential mismatches

**Work:**

- Classify mismatches as rule ambiguity, formal defect, legacy defect, scope
  mismatch, or intentional difference.
- Add a focused regression fixture for every resolution.
- Update rulebook and applicable model tracks together when intent changes.

**Validation:**

```pwsh
cargo test -p poche-compat-v2 regressions
cargo run -p poche-xtask -- compare-v2 --all-fixtures
```

**Completion criteria:** Every retained mismatch is resolved or explicitly tied
to a rule/decision and support consequence.

## Phase 9 — Pages, contributor documentation, and release evidence

### [ ] 9.1 Publish PDF and approved HTML views without Git history churn

**Work:**

- Add a GitHub Actions workflow that installs pinned Typst, compiles
  `docs/main.typ`, and deploys a landing page plus PDF through GitHub Pages.
- Trigger production deployment from the verified `model-checking` default branch
  and use the protected `github-pages` environment established in Task 1.5.
- Evaluate native Typst HTML fidelity under G13 and publish it only if accepted.
- Add README links for pretty rules and Typst source.
- Fail publication when compilation fails; never serve stale output silently.

**Validation:**

```pwsh
& $env:TYPST_BIN compile docs/main.typ "$env:TEMP\poche-rules.pdf"
git check-ignore docs/main.pdf
git ls-files "*.pdf"
gh repo view TeamDman/Poche --json defaultBranchRef
gh api repos/TeamDman/Poche/pages
```

**Completion criteria:** The public README link displays current generated rules,
source remains directly accessible, and generated document output is untracked.

### [ ] 9.2 Document modeling roles, authoring, proof strength, and future RL boundary

**Work:**

- Document how rule IDs flow into each native model and the Rust formal graph.
- Document supported/rejected formal operations, observation/scoring contracts,
  native tool discovery, and conformance commands.
- Explain exhaustive, bounded, queried, and fuzzed evidence separately, and why
  any future trained evidence will be empirical rather than a formal proof.
- Document how LLM-authored changes are reviewed and validated.

**Validation:**

```pwsh
cargo test --doc --workspace
cargo run -p poche-xtask -- doctor
cargo run -p poche-xtask -- coverage audit --all
```

**Completion criteria:** A new contributor can reproduce each advertised kind of
evidence and cannot reasonably mistake training or bounded checking for proof of
another kind.

### [ ] 9.3 Audit guidance fidelity and record first milestone evidence

**Work:**

- Verify every active U requirement remains present and traceable.
- Verify every completed task has durable completion notes and commands/results.
- Update matrices with versions, scopes, counts, hashes, limitations, and links.
- Record deferred work without marking it complete.

**Validation:**

```pwsh
cargo run -p poche-xtask -- guidance audit PLAN.md
cargo run -p poche-xtask -- coverage audit --all
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git diff --exit-code
```

**Completion criteria:** U1–U29 and every rule ID have current dispositions, all
advertised evidence is reproducible, and no nuance is known to exist only in
conversation history.

## Overall completion criteria

- [ ] Every active U requirement remains in the guidance ledger and maps to
  implementation/evidence.
- [ ] `docs/main.typ` is copied, rendered reproducibly, and published through a
  working GitHub Pages pretty-view link without tracked generated PDF/HTML.
- [ ] The pushed `model-checking` branch is the verified GitHub default/head
  branch, Pages uses GitHub Actions, and production deployment policy targets
  that branch without deleting `main` implicitly.
- [ ] Every normative Poche rule has a stable rule ID/source anchor and explicit
  Rust, Alloy, NuSMV, and Prolog applicability/evidence disposition; rules
  affecting future RL boundaries are separately flagged without adding an RL
  model track.
- [ ] Native comprehensive Alloy, NuSMV, Prolog, and Rust models execute in their
  intended engines/runtimes.
- [ ] Alloy structural/bounded consistency results name their scopes.
- [ ] NuSMV safety/deadlock/termination results and counterexamples are recorded.
- [ ] Prolog answers forward legal-action/transition queries and bounded reverse
  predecessor/action questions with documented modes/constraints.
- [ ] Rust uses strong finite/phase types where practical and exposes explicit
  state, observation, legal action, chance, transition, round-score/outcome, and
  terminal contracts.
- [ ] The named micro-scope is exhaustively explored with deterministic counts,
  safety evidence, and liveness/deadlock evidence.
- [ ] Injected defects demonstrate that each important property/query can fail
  discriminatingly rather than passing vacuously.
- [ ] Cross-model fixtures and properties agree or every discrepancy is explicit,
  classified, and tied to a rule/decision.
- [ ] Phon results carry model, rules, scope, observation, scoring, backend, and
  confidence identity.
- [ ] Future-RL compatibility is demonstrated without implementing RL: hidden
  information boundaries are explicit, chance/replay semantics are deterministic,
  and raw per-player end-of-round scores are available as the intended
  intermediary reward signal.
- [ ] RL implementation, automatic target generation, and legacy-v2 comparison
  remain explicitly deferred and do not block current completion.
- [ ] Clean-clone build, native-tool, coverage, conformance, and documentation
  commands are recorded and reproducible.

## Risk register

| Risk | Consequence | Guardrail / validation gate |
| --- | --- | --- |
| User nuance is compressed out of later plan edits | Work completes against a simpler but wrong objective | Guidance ledger, immutable U IDs, traceability audit in 1.4/9.3 |
| A rule is omitted from every computerized model | Models agree because all missed the same requirement | Stable rule IDs and per-track coverage ledger |
| Independent oracle models drift indefinitely | “Comprehensive” models describe different games | Shared fixtures, discrepancy classification, Phase 6 audit |
| A later generator replaces independent evidence | Generator and outputs share the same semantic defect | G12 defers generation from this goal; any later goal must retain handwritten oracle baselines |
| Rust executes opaque logic instead of constructing a graph | Facet/Weavy comparisons cover only part of behavior | G1; restricted graph; lowering validator; conventional-vs-strict Rust comparison |
| Phon shape is mistaken for semantic refinement | Invalid states enter the state space | G2; `FiniteDomain`/refinement layer and exhaustive round trips |
| State explosion prevents comprehensive finite checking | Verification stalls or scopes are mislabeled | Named micro-scope; state counts; tool-specific scopes; symmetry only with equivalence tests |
| NuSMV liveness uses hidden fairness/stuttering assumptions | “Always ends” is vacuous or false for the wrong reason | G6; explicit chance/player steps; SCC and NuSMV cross-check |
| Prolog reverse queries diverge or enumerate infinite terms | Predecessor/explanation goals appear unusable | Finite constraints, modes, tabling where appropriate, bounded history documented |
| Future RL observation leaks hidden cards | A learned policy could exploit impossible information | G9; preserve explicit per-agent observation boundaries now and run noninterference tests in the later RL goal |
| Future reward shaping obscures game score | A policy could optimize an accidental proxy presented as Poche success | G10/U28; preserve raw end-of-round and cumulative scores before any later projection |
| Chance/RNG is embedded in formal state or unreplayable | Formal traces and future learning episodes cannot agree | Explicit chance actions and deterministic trace replay |
| Future RL improvement is presented as formal optimality | Users overstate empirical results | U10/U26 and 9.2 document the evidence boundary; training remains outside this goal |
| Local Facet/Phon/Weavy dependencies are unreproducible | CI/fresh clones fail | G3 exact versions/revisions; no committed sibling paths |
| Native CLI output changes | Parsers silently report false success | Pin versions, preserve raw diagnostics, fail closed on unknown output |
| License/provenance is inherited accidentally | Distribution obligations are unclear | G7 before source import; clean implementation and notices |
| `v2` behavior overrides current rules | Legacy defects become normative | U14; delayed trace boundary; rule-ID mismatch classification |
| Typst Pages serves stale or unfaithful output | Pretty view disagrees with source | Pinned build, fail deployment on compile failure, G13 fidelity check |
| Pages/default-branch assumptions remain implicit | Workflow does not run, deploys the wrong branch, or repository landing/PR defaults point at legacy work | U21; G14; remote-state audit and Task 1.5 before publication |
| Confidence kinds collapse bounded, exhaustive, queried, fuzzed, and future trained evidence | Results are interpreted more strongly than warranted | Typed result confidence and acceptance/documentation matrices |
