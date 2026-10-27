use std::path::Path;

use marcr::{Field, Record, Subfield};

use crate::Rules;

fn rules(rules: &str) -> Rules {
    Rules::parse(rules, Path::new("test.rules")).unwrap_or_else(|e| panic!("{e}"))
}

fn record(fields: Vec<Vec<&str>>) -> Record {
    let mut record = Record::default();
    record.insert(fields);
    record
}

/// The fields of `record` as `TAG IN $aA $bB` lines (`#` for blanks).
fn lines(record: &Record) -> Vec<String> {
    record.fields.iter()
        .map(|field| match field {
            Field::Control(tag, value) => format!("{tag:03} {value}"),
            Field::Standard(tag, ind, subfields) => {
                let ind: String = ind.iter().map(|c| if *c == ' ' { '#' } else { *c }).collect();
                let subfields: Vec<String> = subfields.iter().map(|Subfield(c, v)| format!("${c}{v}")).collect();
                format!("{tag:03} {ind} {}", subfields.join(" "))
            }
        })
        .collect()
}

fn error(text: &str) -> String {
    match Rules::parse(text, Path::new("t.rules")) {
        Ok(_) => panic!("{text:?} should not compile"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn subfields_are_converted_in_source_order() {
    let rules = rules("001 -> 001\n214 -> 264 [#1] { $a -> $a ; $c -> $b ; $d -> $c }");
    let source = record(vec![
        vec!["001", "123"],
        vec!["214", " 0", "a", "New York", "c", "Swiss", "a", "Zürich", "c", "Cabaret", "z", "dropped"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), ["001 123", "264 #1 $aNew York $bSwiss $aZürich $bCabaret"]);
}

#[test]
fn templates_and_functions() {
    let rules = rules(
        r#"
        table roles {
          070 = aut, Auteur
          651 = pbd, Directeur
        }
        700 -> 100 [1#] {
          $3 -> $0 "(IdRef){}", $1 "http://www.idref.fr/{}/id"
          $a -> $a "{}, {$b}"
          $f -> $d "({})."
          $4 -> $4 {|table(roles)}, $e {|table(roles, 2)}
        }
        010 -> 020 { $a -> $a | replace('-', '') ; $b -> += " ({|replace('\.$', '')})" }
        "#,
    );
    let source = record(vec![
        vec!["010", "  ", "a", "978-2-07", "b", "br."],
        vec!["700", " 1", "3", "026764644", "a", "Camus", "b", "Albert", "f", "1913-1960", "4", "070", "4", "999"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), [
        "020 ## $a978207 (br)",
        "100 1# $0(IdRef)026764644 $1http://www.idref.fr/026764644/id $aCamus, Albert $d(1913-1960). $4aut $eAuteur",
    ]);
}

#[test]
fn ranks_and_append() {
    let rules = rules("200 -> 245 { $a -> $a ; $d[1] -> $b ; $d[2..] -> += ' = {}' ; $g -> += ' ; {}' ; $e[-1] -> $p }");
    let source = record(vec![vec!["200", "1 ", "a", "Dada", "d", "Wiederkehr", "d", "Retour", "e", "x", "e", "y", "g", "preface"]]);
    assert_eq!(lines(&rules.apply(&source)), ["245 1# $aDada $bWiederkehr = Retour $py ; preface"]);
}

#[test]
fn conditions_indicators_and_record_paths() {
    let rules = rules(
        r#"
        606 when $2=rameau -> 650 [#7] { $a -> $a ; $2 -> $2 "ram" }
        606 when $2~^lc$ -> 650 [#0] { $a -> $a }
        200 -> 245 [{700,710|exists(1,0)} {$a|nonfiling}] { $a -> $a | strip_nsb }
        record -> 008 "{100a/2-7}|{101a}"
        record when !856 -> 040 [##] { -> $a "ABES" }
        "#,
    );
    let source = record(vec![
        vec!["100", "  ", "a", "20131114h20131942"],
        vec!["101", "0 ", "a", "fre"],
        vec!["200", "1 ", "a", "\u{98}L'\u{9c}étranger"],
        vec!["606", "  ", "a", "Roman", "2", "rameau"],
        vec!["606", "  ", "a", "Novel", "2", "lc"],
        vec!["606", "  ", "a", "Other", "2", "mesh"],
        vec!["710", "02", "a", "Gallimard"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), [
        "008 131114|fre",
        "040 ## $aABES",
        "245 12 $aL'étranger",
        "650 #7 $aRoman $2ram",
        "650 #0 $aNovel",
    ]);
}

#[test]
fn default_rule_and_like() {
    let rules = rules(
        r#"
        700 -> 100 [1#] { $a -> $a }
        701 702 -> 700 like 700
        * -> 886 [2#] { -> $2 "unimarc" ; -> $a "{@tag}" ; -> $b "{@ind}" ; -> $c "" ; $* -> $* }
        "#,
    );
    let source = record(vec![
        vec!["181", " 1", "a", "i#", "b", "xxxe##"],
        vec!["700", " 1", "a", "Camus"],
        vec!["701", " 1", "a", "Sartre"],
        vec!["702", " 1", "a", "Beauvoir"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), [
        "100 1# $aCamus",
        "700 #1 $aSartre",
        "700 #1 $aBeauvoir",
        "886 2# $2unimarc $a181 $b 1 $c $ai# $bxxxe##",
    ]);
}

#[test]
fn source_field_ranks() {
    let rules = rules(
        r#"
        500[1] when 700 -> 240 [10] { $a -> $a }
        500[1] when !700 -> 130 [0#] like 500
        500[2..] -> 730 [0#] { $a -> $a ; $r -> += ",", $m ; $s -> += ".", $n ; order $a $m $n }
        "#,
    );
    let source = record(vec![
        vec!["500", "  ", "a", "Sonates", "s", "K 1", "r", "Clavier"],
        vec!["500", "  ", "a", "Stabat Mater"],
        vec!["500", "  ", "a", "Sonates", "s", "K 90", "r", "Clavier"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), [
        "130 0# $aSonates",
        "730 0# $aStabat Mater",
        "730 0# $aSonates. $mClavier $nK 90,",
    ]);
}

#[test]
fn punctuation_and_order() {
    let rules = rules(
        r#"
        200 -> 245 [10] {
          $a[1] -> $a
          $a[2..] -> $b before " ;"
          $e -> $b before " :"
          $f -> $c before " /"
          end "."
        }
        215 -> 300 [##] {
          $a -> $a
          $c -> $b before " :"
          $d -> $c before ". ;" after $b before " ;"
        }
        700 -> 100 [1#] {
          $a -> $a
          $4 -> $4, $e "role {}" before "." after $e
          order $a $4 $e
          end "." skip $4 $e
        }
        "#,
    );
    let source = record(vec![
        vec!["200", "1 ", "a", "Henriade", "a", "Essai", "e", "suivi", "f", "Voltaire."],
        vec!["215", "  ", "a", "1 vol.", "c", "ill.", "d", "18 cm"],
        vec!["215", "  ", "a", "2 vol.", "d", "21 cm"],
        vec!["700", " 1", "a", "Camus", "4", "070", "4", "651"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), [
        "100 1# $aCamus. $4070 $4651 $erole 070. $erole 651",
        "245 10 $aHenriade ; $bEssai : $bsuivi / $cVoltaire.",
        "300 ## $a1 vol. : $bill.. ; $c18 cm",
        "300 ## $a2 vol. ; $c21 cm",
    ]);
}

#[test]
fn tables_header_and_defaults() {
    let rules = rules(
        r##"
        rules "Test rules"
        version 1.2
        # comment
        table langs {
          fre = French, "fr, FR"
          * = Unknown
        }
        table blanks {
          y = " "
          " " = "#"
        }
        table codes {
          scr = hrv
          * = {}
        }
        101 -> 041 [##] { $a -> $a {|table(langs)}, $b {|table(langs, 2)} }
        105 -> 008 "{$a|tr(blanks)}"
        102 -> 044 [##] { $a -> $a {|table(codes)} }
        "##,
    );
    assert_eq!((rules.name(), rules.version()), (Some("Test rules"), Some("1.2")));
    let source = record(vec![vec!["101", "0 ", "a", "fre", "a", "xxx"], vec!["102", "  ", "a", "scr", "a", "fre"], vec!["105", "  ", "a", "ay y"]]);
    assert_eq!(lines(&rules.apply(&source)), ["008 a # ", "041 ## $aFrench $bfr, FR $aUnknown", "044 ## $ahrv $afre"]);
}

#[test]
fn builtin_rules_compile() {
    for (name, _) in crate::BUILTIN {
        assert!(Rules::builtin(name).is_some(), "{name}");
    }
    assert!(Rules::builtin("nope").is_none());
}

#[test]
fn errors_give_the_line() {
    assert_eq!(error("001 -> 001\n200 -> 245 { $a -> $a | nope }"), "t.rules:2: unknown function `nope`");
    assert_eq!(error("\n\nfoo -> 245"), "t.rules:3: expected a rule, `rules`, `version` or `table`, found `foo`");
    assert!(error("200 -> 245 { $a -> $a").contains("unterminated block"));
    assert!(error("200 -> 245 { $a -> $a '{$b' }").contains("unterminated `{`"));
    assert!(error("200 -> 245 { $a -> $a {|table(x)} }").contains("unknown table `x`"));
    assert!(error("table t {\n a = 1\n a = 2\n}").contains("duplicate table key `a`"));
    assert!(error("table t {\n a 1\n}").contains("expected `=`"));
    assert!(error("200 -> 245 { $a -> $a ; order }").contains("expected subfield codes"));
    assert!(error("701 -> 700 like 700").contains("no previous rule"));
}

#[test]
fn order_with_the_other_codes_in_place() {
    let rules = rules("650 -> 606 { -> $2 'lcsh' ; $* -> $* ; order $a $* $2 }");
    let source = record(vec![vec!["650", " 0", "a", "Insects", "z", "Egypt", "v", "Periodicals"]]);
    assert_eq!(lines(&rules.apply(&source)), ["606 #0 $aInsects $zEgypt $vPeriodicals $2lcsh"]);
}

#[test]
fn linked_fields_and_common_lines() {
    let rules = rules(
        r#"common { $6 -> $6 | replace('^\d{3}-(\d\d).*$', 'a$1'), $7 | replace('^[^/]*$', '') | replace('^.*/\(2.*$', 'ha') }
        245 -> 200 [1#] { $a -> $a ; order $a }
        880 -> linked
        "#,
    );
    let source = record(vec![
        vec!["245", "10", "6", "880-01", "a", "Hisṭoryah"],
        vec!["880", "10", "6", "245-01/(2/r", "a", "היסטוריה"],
    ]);
    assert_eq!(lines(&rules.apply(&source)), ["200 1# $6a01 $aHisṭoryah", "200 1# $6a01 $7ha $aהיסטוריה"]);
    let (_, report) = rules.apply_with_report(&source);
    assert!(report.unconverted_fields.is_empty() && report.unconverted_subfields.is_empty());
}

#[test]
fn conditions_on_the_source_value() {
    let rules = rules("007 when ~^s -> 126 [##] { -> $a '{/1}' }\n007 when !~^s -> 135 [##] { -> $a '{/1}' }");
    let source = record(vec![vec!["007", "sd fsngnnmmned"], vec!["007", "cr |||||||||||"]]);
    assert_eq!(lines(&rules.apply(&source)), ["126 ## $ad", "135 ## $ar"]);
}

#[test]
fn unicode_escapes() {
    let rules = rules(r#"245 -> 200 { $a -> $a "{@ind2}{}" | replace('^4(.{4})', '\u{98}$1\u{9c}') | replace('^0', '') }"#);
    let source = record(vec![vec!["245", "14", "a", "The title"], vec!["245", "10", "a", "Title"]]);
    assert_eq!(lines(&rules.apply(&source)), ["200 14 $a\u{98}The \u{9c}title", "200 10 $aTitle"]);
    assert!(error("200 -> 245 { $a -> $a '\\u{zz}' }").contains("invalid escape"));
}

#[test]
fn report_lists_what_is_left_out() {
    let rules = rules(
        r#"table langs {
          fre = French
        }
        record -> 008 "{100a/9-12}"
        101 -> 041 { $a -> $a {|table(langs)} }
        200 -> 245 { $a -> $a "{}{$e}" }
        "#,
    );
    let source = record(vec![
        vec!["100", "  ", "a", "20240101d2024"],
        vec!["101", "0 ", "a", "fre", "a", "xxx", "c", "ger"],
        vec!["200", "1 ", "a", "Titre", "e", "sous-titre", "f", "Auteur"],
        vec!["330", "  ", "a", "Résumé"],
    ]);
    let (converted, report) = rules.apply_with_report(&source);
    assert_eq!(converted, rules.apply(&source));
    // 100 is read by an expression, 200$e by a template
    assert_eq!(report.unconverted_fields, [330]);
    assert_eq!(report.unconverted_subfields, [(101, 'c'), (200, 'f')]);
    assert_eq!(report.table_misses, [(String::from("langs"), String::from("xxx"))]);
}
