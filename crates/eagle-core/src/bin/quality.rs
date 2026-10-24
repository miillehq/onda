//! Reproducible content-safe deterministic quality and latency harness.
use eagle_core::{contracts::*, dictionary::DictionaryIndex, text::validate_enhancement};
use serde::Deserialize;
use std::{path::Path, time::Instant};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    raw: String,
    expected_local: String,
    enhanced: String,
    expect_enhanced_valid: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/quality.json");
    let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(path)?)?;
    let entries = vec![DictionaryEntry {
        id: "fixture".into(),
        kind: DictionaryKind::Replacement,
        phrase: "post grass".into(),
        replacement: Some("Postgres".into()),
        enabled: true,
        starred: true,
        source: EntrySource::Manual,
    }];
    let index = DictionaryIndex::build(&entries, &[])?;
    for case in cases {
        let output = index.normalize(&case.raw)?;
        if output != case.expected_local
            || validate_enhancement(&output, &case.enhanced, &[], true).is_ok()
                != case.expect_enhanced_valid
        {
            return Err(format!("Quality case failed: {}", case.name).into());
        }
    }
    let scale_entries: Vec<_> = (0..10_000)
        .map(|index| DictionaryEntry {
            id: index.to_string(),
            kind: DictionaryKind::Vocabulary,
            phrase: format!("ProjectTerm{index}"),
            replacement: None,
            enabled: true,
            starred: false,
            source: EntrySource::Manual,
        })
        .collect();
    let started = Instant::now();
    let scale = DictionaryIndex::build(&scale_entries, &[])?;
    let index_build_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(scale.candidates("Ask about ProjectTerm3842 today.", ""));
    }
    let retrieval_ms = started.elapsed().as_secs_f64();
    println!(
        "{}",
        serde_json::json!({"quality":"passed","dictionaryEntries":10000,"indexBuildMs":index_build_ms,"meanCandidateRetrievalMs":retrieval_ms,"iterations":1000})
    );
    Ok(())
}
