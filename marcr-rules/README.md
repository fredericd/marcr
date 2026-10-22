# marcr-rules

A rule language to convert MARC records from one format to another,
built on [marcr](https://crates.io/crates/marcr). It comes with rules
converting UNIMARC to MARC21, written after the conversion of the French
union catalogue Sudoc (Abes).

A rules file is self-contained: header, tables and rules in a single
text file, easy to share, read and compare between versions.

## Command-line usage

```sh
marcr-rules list                                        # built-in rules
marcr-rules convert -r unimarc-to-marc21 -s marcxml notices.mrc > notices.xml
marcr-rules show unimarc-to-marc21 > my.rules           # start your own rules
marcr-rules convert -r my.rules -s iso2709 -o out.mrc notices.mrc
marcr-rules eval -r my.rules corpus/unimarc.mrc corpus/marc21.mrc --show 245,300
```

`-r` takes a rules file, or the name of built-in rules. `convert` takes
the same `-d`, `-s`, `-o` and `-j` options as `marcr` and runs on all
cores. `eval` converts each record of the first file and compares it, tag
by tag, with the record of the second file that has the same 001; `--show`
prints the differences for some tags.

## Library usage

```rust
use marcr_rules::Rules;

let rules = Rules::builtin("unimarc-to-marc21").unwrap(); // or Rules::from_file(path)?
let marc21 = rules.apply(&unimarc_record);
```

`Rules::apply` takes a record and returns a new one: use it as the
`process` closure of `marcr::parallel::convert` to convert on all cores.

## The language

### Structure

```
# A comment: `#` followed by a space, up to the end of the line.

rules "UNIMARC to MARC21 (Sudoc)"     # header: name and version
version 0.1

table roles {                          # tables, before the rules using them
  070 = aut., Auteur
  651 = pbd., Directeur de la publication
}

200 -> 245 [10] { ... }                # rules
```

### Tables

```
table langs {
  fre = French, "fr, FR"      # key = value, value...
  *   = Unknown               # value for a missing key (optional)
}
```

One entry per line. Keys and values are trimmed; quote one that contains
a comma. `{|table(langs)}` gives the first value column of the key,
`{|table(langs, 2)}` the second. A missing key gives the `*` entry, or no
value (the subfield is then not produced).

### Rules

```
SOURCE [when CONDITION]... -> TARGET
```

The source is one or more tag patterns (`606`, `7..`, `300 301 302`),
`record` (applied once per record) or `*` (fields that no other rule
converted). A rank selects source fields by their occurrence among the
fields with the same tag: `500[1] -> 240`, `500[2..] -> 730`. Every rule whose
source and conditions match a field applies: a field can produce several
fields (`410 -> 760` and `410 -> 830`). The fields of the converted
record are sorted by tag; fields that no rule converts are dropped.

The target is one of:

```
LDR "template"                  # the leader
008 "template"                  # a control field (tag < 10)
001                             # a control field, copying the value
245 [IND] { BODY }              # a data field
701 -> 700 [IND] like 700       # the body of the first rule from 700
```

Indicators are two characters, `#` for a blank, or `{expressions}`:
`[#7]`, `[{@ind2}#]`, `[{700,710|exists(1,0)} {$a|nonfiling}]`. Without
indicators, the source field's are kept.

### Body

```
200 -> 245 [10] {
  $a[1]   -> $a | strip_nsb                 # first $a
  $a[2..] -> $b before " ;"                 # next $a, preceded by " ;"
  $e      -> $b before " :"
  $f[1]   -> $c before " /"
  $f[2..] -> += " ; {}"                     # appended to the previous subfield
  $3      -> $0 "(IdRef){}", $1 "http://www.idref.fr/{}/id"   # two subfields
  -> $2 "rameau"                            # once per field, not per subfield
  $*      -> $*                             # any subfield, copied
  order $0 $1 $a $b $c                      # target subfield order
  end "." skip $0 $1 $2                     # final punctuation
}
```

Each source subfield is taken in order, and every line selecting it
applies, so repeated subfields are converted one by one in their order.
`$a` selects every `$a`; `$a[2]` the second, `$a[2..]` from the second,
`$a[1..3]`, `$a[-1]` the last. Lines without a source (`-> ...`) apply
once, before the others. Lines are separated by line breaks or `;`.

Outputs, separated by commas:

- `$c VALUE`: a subfield `$c`. Without a value, the source value is
  copied. A subfield whose value is empty is not produced, unless the
  value is a literal (`-> $b ""`); a field without subfields is not
  produced.
- `+= VALUE`: appended to the last subfield produced, in the source
  order: `$r -> += ",", $m` puts a comma after the subfield preceding
  `$r`, even if `order` moves it.
- `before "P"`, after a `$c` output: when the subfield is not the first
  of the field, `P` is appended to the subfield preceding it.
  `before "P" after $b` only applies if that subfield is a `$b`; several
  `before` can follow each other, the first that applies is used.

Block statements:

- `order $a $b ...`: target subfields are sorted in this order (repeated
  codes keep theirs; codes not listed go last), before punctuation.
- `end "P" [skip $x ...]`: `P` is appended to the last subfield not
  skipped, unless it already ends with `P`.

### Values and templates

A value is a template string, `"..."` or `'...'` (`\\`, `\"`, `\'`,
`\n` and `\t` are escapes, other backslashes are kept, as in regexes:
`'\.$'`), where `{expression}`
is replaced by its result (`{{` and `}}` for literal braces), or a single
`{expression}`. Functions after the value apply to the whole result:
`$a -> $a | replace('-', '')`. Texts are compared and produced as they
are, without Unicode normalization: write literals and table values in
the form of the records (decomposed, NFD, for the Sudoc).

Expressions: `operand[/positions][|function]...`

| Operand | Value |
|---|---|
| `{}` | the source subfield value (or control field value) |
| `{$b}`, `{$b[2]}`, `{$b[-1]}`, `{$b*}` | the first, second, last, all `$b` of the source field |
| `{100a}`, `{606[2]a}`, `{700,710}`, `{008}` | values elsewhere in the record (first field and subfield by default; a field without code gives its subfields joined by spaces) |
| `{LDR}` | the leader |
| `{@tag}`, `{@ind}`, `{@ind1}`, `{@ind2}` | tag and indicators of the source field |
| `{'text'}` | a literal |

`/9` and `/9-12` take character positions (from 0, inclusive):
`{100a/9-12}`, `{LDR/6}`, `{/0-15}`. Several values (`{$b*}`) are joined
by a space unless `join` is used.

| Function | Result |
|---|---|
| `strip_nsb` | removes the non-sorting marks NSB/NSE (U+0098, U+009C) |
| `nonfiling` | the number of characters between NSB and NSE at the start (0 if none) |
| `lower`, `upper`, `trim` | case, surrounding spaces |
| `replace(REGEX, TEXT)` | replaces matches (`$1` for groups) |
| `table(NAME[, COLUMN])` | looks the value up in a table |
| `tr(NAME)` | looks each character up in a table (first column), keeping those missing |
| `default(TEXT)` | `TEXT` if there is no value |
| `exists(YES[, NO])` | `YES` if there is a value, else `NO` |
| `join([SEPARATOR])` | joins several values (space by default) |

### Conditions

```
606 when $2=rameau -> 650 [#7] { ... }
606 when $2~^(lc|LC)$ -> 650 [#0] { ... }
record when !856 -> ...
```

`when [!]OPERAND[=VALUE|~REGEX]`: the operand is a subfield of the source
field (`$2`), an indicator (`@ind2`) or a record path (`700a`); without
a rank, any occurrence matches. Without a test, the operand must have a
value. `!` negates. Several `when` must all hold.

## Corpus

`corpus/` holds the PPNs of Sudoc records, fetched in UNIMARC and in
MARC21 from the Abes public Z39.50 server, to write and evaluate the
rules: 2,340 records in `ppn.txt` to work on, and 400 others in
`ppn-test.txt`, kept aside to check that the rules hold on records not
looked at while writing them.

```sh
corpus/fetch.sh                      # unimarc.mrc, marc21.mrc
corpus/fetch.sh ppn-test.txt test    # unimarc-test.mrc, marc21-test.mrc
corpus/sample.sh 50 k m -x corpus/ppn.txt >> corpus/ppn.txt   # 50 more maps and scores
```

`sample.sh` picks random PPNs by type of document from the Sudoc SRU
server. The records themselves are not part of the repository nor of
the published crate.
