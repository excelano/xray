//! End-to-end tests: run the built `xray` binary against synthetic fixtures and
//! assert on its --json output. These lock in the corpus-tuned heuristics
//! (delimiter sniff, buried-header, boolean families, near-unique keys, long
//! IDs) so a future change can't silently regress them. Fixtures are synthetic
//! by policy — real client data never enters this repo.

use std::io::Write;
use std::process::{Command, Stdio};

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

/// Run xray with `input` on stdin rather than a file argument.
fn run_piped(args: &[&str], input: &[u8]) -> (String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xray"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to run xray");
    child
        .stdin
        .take()
        .expect("no stdin")
        .write_all(input)
        .expect("failed to write to xray's stdin");
    let out = child.wait_with_output().expect("failed to wait for xray");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn profile(path: &str) -> Value {
    profile_with(&["--json", path])
}

fn profile_with(args: &[&str]) -> Value {
    let (stdout, code) = run(args);
    assert_eq!(code, 0, "xray exited {code} on {args:?}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid json for {args:?}: {e}"))
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
fn delim_override_beats_the_sniff() {
    // The sniff reads employees.csv as comma-separated; --delim says otherwise
    // and wins, collapsing every line into one field. Contrived here, but it is
    // the escape hatch for a file whose delimiter the sniff genuinely misreads.
    let v = profile_with(&["--json", "-d", ";", "fixtures/clean/employees.csv"]);
    assert_eq!(v["film"]["delimiter"], ";");
    assert_eq!(v["film"]["columns"], 1);
}

#[test]
fn tab_delimiter_takes_the_backslash_t_escape() {
    // A literal tab is awkward to type and most shells eat it, so `\t` spells it.
    // The fixture's commas sit inside values, so this also fails loudly if the
    // escape were ignored and the sniff picked comma instead.
    let v = profile_with(&["--json", "--delim", "\\t", "fixtures/clean/regions.tsv"]);
    assert_eq!(v["film"]["delimiter"], "\t");
    assert_eq!(v["film"]["columns"], 2);
    assert_eq!(column(&v, "B")["class"], "int");
}

#[test]
fn no_header_matches_header_zero() {
    let sugar = profile_with(&["--json", "--no-header", "fixtures/clean/employees.csv"]);
    let explicit = profile_with(&["--json", "--header", "0", "fixtures/clean/employees.csv"]);
    assert_eq!(sugar["film"]["header_row"], 0);
    assert_eq!(sugar, explicit, "--no-header must mean exactly --header 0");
}

#[test]
fn no_header_with_an_explicit_header_row_is_refused() {
    let (_, code) = run(&[
        "--no-header",
        "--header",
        "2",
        "fixtures/clean/employees.csv",
    ]);
    assert_ne!(
        code, 0,
        "--no-header and --header disagree; that must not pass"
    );
}

/// Profile piped bytes, asserting a clean exit and returning the parsed JSON.
fn profile_piped(args: &[&str], path: &str) -> Value {
    let input = std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let (stdout, code) = run_piped(args, &input);
    assert_eq!(code, 0, "xray exited {code} on piped {path}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid json for piped {path}: {e}"))
}

#[test]
fn piped_input_reads_the_same_as_the_file() {
    let mut piped = profile_piped(&["--json"], "fixtures/messy/vendor_spend.csv");
    let mut from_file = profile("fixtures/messy/vendor_spend.csv");
    // The only honest difference: a pipe has no filename to report.
    assert_eq!(piped["file"], "(stdin)");
    assert_eq!(from_file["file"], "vendor_spend.csv");
    piped["file"] = Value::Null;
    from_file["file"] = Value::Null;
    assert_eq!(
        piped, from_file,
        "a pipe and a file must profile identically"
    );
}

#[test]
fn dash_is_the_explicit_spelling_for_stdin() {
    let dash = profile_piped(&["--json", "-"], "fixtures/clean/employees.csv");
    let bare = profile_piped(&["--json"], "fixtures/clean/employees.csv");
    assert_eq!(dash, bare, "`-` and an omitted file both mean stdin");
}

#[test]
fn flags_still_apply_over_a_pipe() {
    // The overrides are not a file-only feature — a pipe is the case where a
    // sniff has the least to go on, so they matter more there, not less.
    let v = profile_piped(
        &["--json", "--no-header", "--delim", ","],
        "fixtures/clean/employees.csv",
    );
    assert_eq!(v["film"]["header_row"], 0);
    assert_eq!(v["film"]["delimiter"], ",");
}

#[test]
fn piped_output_has_no_ansi_escapes() {
    // Not a TTY here, so auto should emit plain text.
    let (stdout, _) = run(&["fixtures/messy/vendor_spend.csv"]);
    assert!(!stdout.contains('\u{1b}'), "piped output must be plain");
}

// ---- referral (--refer) ----

fn referrals(v: &Value) -> &Vec<Value> {
    v["referral"].as_array().expect("no referral array")
}

#[test]
fn referral_is_absent_until_asked_for() {
    let v = profile("fixtures/messy/vendor_spend.csv");
    assert!(
        v.get("referral").is_none(),
        "referral must stay opt-in; it appeared without --refer"
    );
}

#[test]
fn referral_names_the_column_it_is_about() {
    let v = profile_with(&["--json", "--refer", "fixtures/messy/vendor_spend.csv"]);
    let currency = referrals(&v)
        .iter()
        .find(|r| r["trigger"].as_str().unwrap().contains("currency text"))
        .expect("no currency referral");
    // The aggregate phrasing this replaced ("leading-zero / currency text")
    // left the reader to work out which column was meant.
    assert!(
        currency["trigger"]
            .as_str()
            .unwrap()
            .contains("FY25 Spend ($)"),
        "trigger {:?} does not name its column",
        currency["trigger"]
    );
}

#[test]
fn currency_referral_emits_a_runnable_command() {
    let path = "fixtures/messy/vendor_spend.csv";
    let v = profile_with(&["--json", "--refer", path]);
    let cmd = referrals(&v)
        .iter()
        .find(|r| r["trigger"].as_str().unwrap().contains("currency text"))
        .and_then(|r| r["command"].as_str())
        .expect("currency referral carries no command");

    // Addressed by bracketed header name, which is what survives a header
    // holding spaces, parens and a `$` — the case that would otherwise send an
    // agent back to guessing.
    assert_eq!(cmd, format!("xled '[FY25 Spend ($)] s/[$,]//g' {path}"));
    // Read, not write: xray never changes a byte and must not hand over
    // something that does it by proxy.
    assert!(
        !cmd.contains(" -i"),
        "referral command must not write in place"
    );
}

#[test]
fn referrals_without_an_unambiguous_repair_carry_no_command() {
    let v = profile_with(&["--json", "--refer", "fixtures/messy/vendor_spend.csv"]);
    let protect = referrals(&v)
        .iter()
        .find(|r| r["trigger"].as_str().unwrap().contains("stays text"))
        .expect("no leading-zero referral");
    // The correct action here is to do nothing, so there is nothing to run.
    assert!(protect["command"].is_null());
}

#[test]
fn piped_input_gets_referrals_but_no_commands() {
    let v = profile_piped(&["--json", "--refer"], "fixtures/messy/vendor_spend.csv");
    assert!(
        !referrals(&v).is_empty(),
        "stdin should still get referrals"
    );
    for r in referrals(&v) {
        assert!(
            r["command"].is_null(),
            "stdin has no file to name, so {:?} cannot carry a command",
            r["trigger"]
        );
    }
}
