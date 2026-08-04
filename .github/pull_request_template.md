## Change and intent

Describe the change and the user/rule intent it preserves.

Affected rule IDs and source anchors:

-

## Evidence claim

Name the exact scope and confidence kind (exhaustive, bounded, symbolic,
queried, sampled, or conformance). List controlled defects/counterexamples where
applicable.

## Model impact

- [ ] Conventional Rust reviewed or explicitly not applicable
- [ ] Strict Rust/Weavy reviewed or explicitly not applicable
- [ ] Alloy reviewed or explicitly not applicable
- [ ] NuSMV reviewed or explicitly not applicable
- [ ] Scryer Prolog reviewed or explicitly not applicable
- [ ] Phon/Facet evidence identity reviewed or explicitly not applicable
- [ ] Observation, chance, scoring, and future-RL boundaries reviewed

## LLM assistance

State whether an LLM authored or materially suggested rules, code, models,
fixtures, tests, or prose. Identify those areas and how the complete diff,
native outputs, scopes, provenance, and independent-oracle boundary were
reviewed.

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo run -p poche-xtask -- coverage audit --all`
- [ ] Affected native oracle and pairwise conformance commands
- [ ] `cargo run -p poche-xtask -- compare all --scope micro` when accepted claims changed
- [ ] Documentation, versions, scopes, counts, limitations, and hashes updated
- [ ] Generated/native evidence remains ignored; no machine-local paths added
