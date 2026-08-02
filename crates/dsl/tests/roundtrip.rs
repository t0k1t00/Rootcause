//! Round-trip serialization tests: every [`dsl::CompiledPattern`] must
//! survive a `serde_json` encode/decode cycle unchanged. This matters
//! because a compiled pattern library is expected to be cached to disk
//! (so a future `matcher`/`cli` doesn't recompile every pattern file on
//! every run) — if compiled patterns didn't round-trip, that cache would
//! silently corrupt pattern semantics.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use dsl::CompiledPattern;

const FIXTURES: &[&str] = &["classic_reentrancy.rcdsl", "donation_attack.rcdsl"];

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
}

#[test]
fn compiled_patterns_round_trip_through_json() {
    for name in FIXTURES {
        let source = fixture(name);
        let compiled = dsl::compile_str(&source).unwrap_or_else(|e| panic!("{name}: {e:?}"));

        let json = serde_json::to_string_pretty(&compiled)
            .unwrap_or_else(|e| panic!("{name} failed to serialize: {e}"));
        let decoded: CompiledPattern = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("{name} failed to deserialize: {e}"));

        assert_eq!(compiled, decoded, "{name} did not round-trip identically");
    }
}
