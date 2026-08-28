//! The ddoc evaluation suite is exit evidence, not a ranking threshold.

use std::path::Path;

use serde_json::Value;

#[test]
fn three_named_ddoc_query_shapes_have_ground_truth() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../eval/ddocs");
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(root.join("manifest.json")).expect("ddoc eval manifest exists"),
    )
    .expect("ddoc eval manifest is JSON");
    let scenarios = manifest["scenarios"].as_array().expect("scenario list");
    assert_eq!(scenarios.len(), 3, "the three dd-named query shapes");

    let expected = [
        "which acceptance criteria are still open",
        "which tasks claim acceptance criterion ac-0001 in plan 004-ship-it",
        "which acceptance criteria touch crates/core/src/address.rs",
    ];
    for (path, expected_query) in scenarios.iter().zip(expected) {
        let relative = path.as_str().expect("scenario path");
        let scenario: Value =
            serde_json::from_slice(&std::fs::read(root.join(relative)).expect("scenario exists"))
                .expect("scenario is JSON");
        assert_eq!(scenario["ground_truth"]["search_query"], expected_query);
        assert!(
            scenario["ground_truth"]["addresses"].is_array(),
            "ground truth addresses are always explicit, including a known-empty answer"
        );
        let assertions = scenario["assertions"].as_str().expect("assertions path");
        let assertions_path = root.join(relative).parent().unwrap().join(assertions);
        let parsed: Value =
            serde_json::from_slice(&std::fs::read(assertions_path).expect("assertions exist"))
                .expect("assertions are JSON");
        assert!(
            !parsed["rows"]
                .as_array()
                .expect("assertion rows")
                .is_empty()
        );
    }

    let second_path = scenarios[1].as_str().unwrap();
    let second: Value = serde_json::from_slice(
        &std::fs::read(root.join(second_path)).expect("second scenario exists"),
    )
    .expect("second scenario is JSON");
    assert_eq!(second["lane"], "ddoc-traversal");
    assert_eq!(second["request"]["command"], "refs");
    assert_eq!(
        second["request"]["target"],
        second["ground_truth"]["traversal_target"]
    );
    assert!(
        second["ground_truth"]["resolution"]
            .as_str()
            .unwrap()
            .contains("edge evidence, not prose-similarity evidence")
    );

    let third_path = scenarios[2].as_str().unwrap();
    let third: Value = serde_json::from_slice(
        &std::fs::read(root.join(third_path)).expect("third scenario exists"),
    )
    .expect("third scenario is JSON");
    assert_eq!(third["ground_truth"]["addresses"], serde_json::json!([]));
    assert!(
        third["ground_truth"]["resolution"]
            .as_str()
            .unwrap()
            .contains("PR #12"),
        "the currently empty source-file answer must state why it is correct"
    );
}
