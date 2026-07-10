//! End-to-end tests: run the built `xray` binary against synthetic fixtures and
//! assert on its --json output. These lock in the corpus-tuned heuristics
//! (delimiter sniff, buried-header, boolean families, near-unique keys, long
//! IDs) so a future change can't silently regress them. Fixtures are synthetic
//! by policy — real client data never enters this repo.

use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_xray"))
        .args(args)
        .output()
        .expect("failed to run xray");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn profile(path: &str) -> Value {
    let (stdout, code) = run(&["--json", path]);
    assert_eq!(code, 0, "xray exited {code} on {path}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid json for {path}: {e}"))
}

fn kinds(v: &Value) -> Vec<String> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap().to_string())
        .collect()
}

fn column<'a>(v: &'a Value, letter: &str) -> &'a Value {
    v["reading"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["letter"] == letter)
        .unwrap_or_else(|| panic!("no column {letter}"))
}

#[test]
fn messy_file_reports_the_expected_hazards() {
    let v = profile("fixtures/messy/vendor_spend.csv");
    assert_eq!(v["film"]["header_row"], 1);
    let k = kinds(&v);
    for expected in [
        "leading_zero",
        "currency_text",
        "mixed_type",
        "mixed_bool",
        "total_row",
        "ragged_row",
        "spacer_column",
    ] {
        assert!(
            k.contains(&expected.to_string()),
            "missing finding: {expected}"
        );
    }
}

#[test]
fn clean_file_gets_a_clean_bill() {
    let v = profile("fixtures/clean/employees.csv");
    assert_eq!(v["film"]["header_row"], 1);
    assert!(
        kinds(&v).is_empty(),
        "clean file should have no findings: {:?}",
        kinds(&v)
    );
}

#[test]
fn buried_header_is_detected() {
    let v = profile("fixtures/messy/risk_log.csv");
    assert_eq!(v["film"]["header_row"], 6);
    assert_eq!(v["film"]["preamble"], 5);
    assert!(kinds(&v).contains(&"buried_header".to_string()));
}

#[test]
fn quoted_commas_do_not_fool_the_delimiter() {
    // Regression: commas inside quoted fields once made this sniff as semicolon.
    let v = profile("fixtures/messy/quoted_commas.csv");
    assert_eq!(v["film"]["delimiter"], ",");
    assert_eq!(v["film"]["columns"], 4);
    assert_eq!(v["film"]["header_row"], 1);
}

#[test]
fn plain_yes_no_is_not_mixed_bool() {
    // Regression: Y and N are the two values of one family, not "mixed forms".
    let v = profile("fixtures/messy/flags.csv");
    assert!(
        !kinds(&v).contains(&"mixed_bool".to_string()),
        "Y/N wrongly flagged"
    );
    assert!(
        !kinds(&v).contains(&"duplicate_key".to_string()),
        "'paid' wrongly flagged"
    );
}

#[test]
fn long_ids_stay_text_and_do_not_corrupt_stats() {
    // Regression: 18-digit ids must not pass through f64.
    let v = profile("fixtures/messy/big_ids.csv");
    let msg_id = column(&v, "A");
    assert_eq!(msg_id["class"], "long_id");
    assert!(
        msg_id["min"].is_null(),
        "long id must not have a numeric min"
    );
    assert!(kinds(&v).contains(&"long_id".to_string()));
    // The plain integer column is unaffected.
    assert_eq!(column(&v, "B")["class"], "int");
}

#[test]
fn header_past_end_is_an_error_not_a_wrong_answer() {
    let (_, code) = run(&["--header", "99", "fixtures/clean/employees.csv"]);
    assert_ne!(code, 0, "--header past the last row should fail");
}

#[test]
fn piped_output_has_no_ansi_escapes() {
    // Not a TTY here, so auto should emit plain text.
    let (stdout, _) = run(&["fixtures/messy/vendor_spend.csv"]);
    assert!(!stdout.contains('\u{1b}'), "piped output must be plain");
}
