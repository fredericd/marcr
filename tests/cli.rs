use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn cmd() -> Command {
    Command::cargo_bin("marcr").unwrap()
}

const SAMPLE: &str = "tests/fixtures/sample.xml";

#[test]
fn converts_marcxml_file_to_text() {
    cmd()
        .args(["-d", "marcxml", "-s", "text", SAMPLE])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn stdin_input_matches_file_input() {
    // Regression test: main.rs used to read the *output* format for stdin
    // instead of the *input* format, so -d was silently ignored on stdin.
    let file_output = cmd()
        .args(["-d", "marcxml", "-s", "text", SAMPLE])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let input = fs::read(SAMPLE).unwrap();
    let stdin_output = cmd()
        .args(["-d", "marcxml", "-s", "text"])
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(file_output, stdin_output);
}

#[test]
fn roundtrip_marcxml_to_iso2709_to_marcxml_preserves_record_count() {
    let tmp = tempfile::tempdir().unwrap();
    let iso_path = tmp.path().join("out.mrc");

    cmd()
        .args(["-d", "marcxml", "-s", "iso2709", "-o"])
        .arg(&iso_path)
        .arg(SAMPLE)
        .assert()
        .success();

    let xml_output = cmd()
        .args(["-d", "iso2709", "-s", "marcxml"])
        .arg(&iso_path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let xml_output = String::from_utf8(xml_output).unwrap();

    let original = fs::read_to_string(SAMPLE).unwrap();
    let expected_records = original.matches("<record>").count();
    let actual_records = xml_output.matches("<record").count();
    assert_eq!(expected_records, actual_records);
}

#[test]
fn writes_output_to_file_when_o_flag_given() {
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("out.txt");

    cmd()
        .args(["-d", "marcxml", "-s", "text", "-o"])
        .arg(&out_path)
        .arg(SAMPLE)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let contents = fs::read_to_string(&out_path).unwrap();
    assert!(!contents.is_empty());
}

#[test]
fn concatenates_multiple_input_files() {
    let single = cmd()
        .args(["-d", "marcxml", "-s", "marcxml", SAMPLE])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let single = String::from_utf8(single).unwrap();
    let single_records = single.matches("<record").count();

    let doubled = cmd()
        .args(["-d", "marcxml", "-s", "marcxml", SAMPLE, SAMPLE])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doubled = String::from_utf8(doubled).unwrap();
    let doubled_records = doubled.matches("<record").count();

    assert_eq!(doubled_records, single_records * 2);
}

#[test]
fn errors_on_missing_input_file() {
    cmd()
        .args(["-d", "marcxml", "-s", "text", "tests/fixtures/does_not_exist.xml"])
        .assert()
        .failure();
}

#[test]
fn skips_malformed_records_and_keeps_going() {
    // good, malformed (invalid tag in the directory), good
    let good = "00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}";
    let bad = good.replacen("4500001", "45000X1", 1);
    let input = format!("{good}{bad}{good}");

    let output = cmd()
        .args(["-d", "iso2709", "-s", "marcxml"])
        .write_stdin(input)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("<stdin>: skipped record #2"))
        .stderr(predicate::str::contains("2 records written, 1 skipped"))
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("<record>").count(), 2);
    // The output is complete: the collection is closed.
    assert!(output.trim_end().ends_with("</collection>"));
}

#[test]
fn skips_records_too_long_for_iso2709_output() {
    let record = |value: &str| format!(
        "<record><leader>00000nam a2200000   4500</leader>\
         <datafield tag=\"300\" ind1=\" \" ind2=\" \"><subfield code=\"a\">{value}</subfield></datafield>\
         </record>"
    );
    let input = format!("<collection>{}{}{}</collection>", record("ok"), record(&"x".repeat(10_000)), record("ok"));

    let output = cmd()
        .args(["-d", "marcxml", "-s", "iso2709"])
        .write_stdin(input)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("skipped record #2 on output"))
        .get_output()
        .stdout
        .clone();
    // Two records, each terminated by RT (0x1d)
    assert_eq!(output.iter().filter(|&&b| b == 0x1d).count(), 2);
}

#[test]
fn empty_input_gives_valid_marcxml() {
    cmd()
        .args(["-d", "iso2709", "-s", "marcxml"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("<collection>"))
        .stdout(predicate::str::contains("</collection>"));
}
