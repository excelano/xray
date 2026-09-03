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
fn the_verdict_is_data_not_a_sentence_to_be_parsed() {
    let clean = profile("fixtures/clean/employees.csv");
    assert_eq!(clean["verdict"]["clean"], true);
    assert_eq!(clean["verdict"]["total"], 0);
    assert_eq!(clean["verdict"]["worst"], Value::Null);

    let messy = profile("fixtures/messy/vendor_spend.csv");
    assert_eq!(messy["verdict"]["clean"], false);
    let counts = &messy["verdict"]["counts"];
    let summed: u64 = ["correctness", "type_safety", "structure"]
        .iter()
        .map(|g| counts[g].as_u64().expect("count is a number"))
        .sum();
    assert_eq!(
        summed,
        messy["verdict"]["total"].as_u64().unwrap(),
        "the per-group counts must add up to the total"
    );
    // `worst` names the most severe group that actually fired, in report order.
    let worst = messy["verdict"]["worst"].as_str().expect("a worst group");
    assert!(counts[worst].as_u64().unwrap() > 0);
}

#[test]
fn every_finding_carries_a_machine_severity() {
    let v = profile("fixtures/messy/vendor_spend.csv");
    let fs = v["findings"].as_array().expect("findings array");
    assert!(!fs.is_empty(), "fixture should produce findings");
    for f in fs {
        let group = f["group"].as_str().expect("a group");
        let severity = f["severity"].as_str().expect("a severity");
        assert!(
            matches!(group, "correctness" | "type_safety" | "structure"),
            "group must be the snake_case key, got {group:?}"
        );
        let expected = if group == "structure" { "note" } else { "warn" };
        assert_eq!(severity, expected, "severity must follow the group");
    }
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
fn an_example_holding_a_comma_is_quoted_in_the_reading() {
    // Regression: `Acme, Inc., Globex, LLC, Initech` read as five examples
    // where there were three.
    let (stdout, _) = run(&["--color", "never", "fixtures/messy/quoted_commas.csv"]);
    assert!(
        stdout.contains("\"Acme, Inc.\", \"Globex, LLC\", Initech"),
        "vendor examples are not quoted:\n{stdout}"
    );
    // A thousands comma is grouping, not a separator: money stays bare.
    assert!(
        stdout.contains("1,200.00, 12,000.00, 3,300.00"),
        "grouped numbers must not be quoted:\n{stdout}"
    );
    // The JSON view carries the raw value; quoting is a render concern.
    let v = profile("fixtures/messy/quoted_commas.csv");
    assert_eq!(column(&v, "B")["examples"][0], "Acme, Inc.");
}

#[test]
fn an_embedded_newline_does_not_break_the_reading_table() {
    // Regression: a quoted cell with a newline printed the newline raw, so one
    // column took two lines and the table lost its alignment.
    let (stdout, _) = run(&["--color", "never", "fixtures/messy/multiline.csv"]);
    assert!(
        stdout.contains("line one⏎line two, plain"),
        "newline not flattened:\n{stdout}"
    );
    let reading_lines = stdout
        .lines()
        .skip_while(|l| !l.starts_with("READING"))
        .take_while(|l| !l.is_empty())
        .count();
    assert_eq!(
        reading_lines, 4,
        "READING must be a title, a header, one line per column"
    );
}

#[test]
fn a_named_empty_column_is_not_called_a_spacer() {
    // Regression: the reading said "spacer column" for every empty column,
    // while the finding correctly kept "spacer" for the blank-header case.
    let v = profile("fixtures/messy/empty_named.csv");
    assert_eq!(column(&v, "B")["class"], "empty");
    assert!(kinds(&v).contains(&"empty_column".to_string()));
    assert!(!kinds(&v).contains(&"spacer_column".to_string()));
    let (stdout, _) = run(&["--color", "never", "fixtures/messy/empty_named.csv"]);
    assert!(
        !stdout.contains("spacer"),
        "a named column must not read as a spacer:\n{stdout}"
    );
}

#[test]
fn the_currency_finding_names_what_it_saw() {
    // Regression: every currency finding said "$ and thousands commas", even
    // for a column that never carried a $.
    fn currency_detail(path: &str) -> String {
        let v = profile(path);
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["kind"] == "currency_text")
            .unwrap_or_else(|| panic!("no currency finding in {path}"))["detail"]
            .as_str()
            .unwrap()
            .to_string()
    }
    let both = currency_detail("fixtures/messy/vendor_spend.csv");
    assert!(both.starts_with("$ and thousands commas"), "{both:?}");
    let commas_only = currency_detail("fixtures/messy/quoted_commas.csv");
    assert!(
        commas_only.starts_with("thousands commas"),
        "{commas_only:?}"
    );
    assert!(!commas_only.contains('$'), "{commas_only:?}");
}

#[test]
fn a_stray_sentinel_does_not_hide_inside_a_boolean_or_numeric_column() {
    // Regression: Y/N/NA read as a clean `bool`, and `$5` in an int column
    // vanished from the profile. Both are the minority the profile exists to
    // report, so both fold into mixed_type.
    let v = profile("fixtures/messy/contaminated.csv");
    let active = column(&v, "B");
    assert_eq!(active["class"], "bool");
    assert_eq!(active["type"], "bool · MIXED");
    assert_eq!(active["flag"], "1 non-boolean value");
    let amt = column(&v, "C");
    assert_eq!(amt["class"], "int");
    assert_eq!(amt["type"], "int · MIXED");
    assert_eq!(amt["flag"], "1 non-numeric value");
    let mixed: Vec<&str> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == "mixed_type")
        .map(|f| f["column"].as_str().unwrap())
        .collect();
    assert_eq!(mixed, ["B", "C"]);
}

#[test]
fn row_and_distinct_counts_carry_thousands_separators() {
    // The README shows `4,812 rows`; the render must agree with it.
    let mut input = String::from("id,v\n");
    for i in 0..1200 {
        input.push_str(&format!("{i},x\n"));
    }
    let (stdout, code) = run_piped(&["--color", "never"], input.as_bytes());
    assert_eq!(code, 0);
    assert!(stdout.contains("× 1,200 rows"), "{stdout}");
    assert!(stdout.contains("     1,200  0 … 1199"), "{stdout}");
    // JSON stays bare numbers.
    let (json, _) = run_piped(&["--json"], input.as_bytes());
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["film"]["rows"], 1200);
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
fn schema_smells_each_fire_once_on_the_column_they_describe() {
    // The four structure kinds that shipped without a test. Each is pinned to
    // its column so a heuristic drifting onto a neighbour fails loudly.
    let v = profile("fixtures/messy/schema_smells.csv");
    let at = |kind: &str| -> Vec<&str> {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["kind"] == kind)
            .map(|f| f["column"].as_str().unwrap())
            .collect()
    };
    // 1010 appears twice in an id-like column that is otherwise unique.
    assert_eq!(at("duplicate_key"), ["A"]);
    // Column B repeats column A's header.
    assert_eq!(at("duplicate_header"), ["B"]);
    // Every status is "open".
    assert_eq!(at("constant_column"), ["C"]);
    // One note in eleven rows.
    assert_eq!(at("sparse_column"), ["D"]);
    assert_eq!(v["verdict"]["worst"], "structure");
}

#[test]
fn distinct_counts_saturate_at_the_cardinality_cap() {
    // 10,001 distinct values: the count stops at the cap and says so, rather
    // than growing a hash set without bound on a wide, deep file.
    let mut input = String::from("id\n");
    for i in 0..10_001 {
        input.push_str(&format!("{i}\n"));
    }
    let (json, code) = run_piped(&["--json"], input.as_bytes());
    assert_eq!(code, 0);
    let v: Value = serde_json::from_str(&json).unwrap();
    let id = column(&v, "A");
    assert_eq!(id["distinct"], 10_000);
    assert_eq!(id["distinct_capped"], true);
    // A capped column cannot be called a key: it may or may not be unique.
    assert_eq!(id["candidate_key"], false);
    let (stdout, _) = run_piped(&["--color", "never"], input.as_bytes());
    assert!(stdout.contains("10,000+"), "{stdout}");
}

#[test]
fn a_missing_file_is_bad_input_and_says_which_file() {
    let out = Command::new(env!("CARGO_BIN_EXE_xray"))
        .arg("fixtures/no_such_file.csv")
        .output()
        .expect("failed to run xray");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("xray: fixtures/no_such_file.csv: "),
        "diagnostic must carry the tool prefix and the path: {stderr:?}"
    );
}

#[test]
fn an_unknown_flag_is_bad_invocation() {
    let (_, code) = run(&["--bogus", "fixtures/clean/employees.csv"]);
    assert_eq!(code, 2);
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
fn long_id_referral_does_not_talk_about_zeros() {
    // Regression: leading-zero and long-ID columns shared one "stays text"
    // line, so an 18-digit ID was told a cast would strip its zeros.
    let v = profile_with(&["--json", "--refer", "fixtures/messy/big_ids.csv"]);
    let stays = referrals(&v)
        .iter()
        .find(|r| r["trigger"].as_str().unwrap().contains("stays text"))
        .expect("no long-id referral");
    let action = stays["action"].as_str().unwrap();
    assert!(
        !action.contains("zero"),
        "long-id action talks about zeros: {action:?}"
    );
    assert!(stays["command"].is_null());
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
