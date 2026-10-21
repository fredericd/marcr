//! Rules file syntax tree and parser (hand-written, character based).

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

use crate::Error;

/// A tag pattern: three digits or `.` wildcards (`606`, `7..`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TagPat([Option<u8>; 3]);

impl TagPat {
    pub(crate) fn matches(&self, tag: u16) -> bool {
        let digits = [(tag / 100 % 10) as u8, (tag / 10 % 10) as u8, (tag % 10) as u8];
        self.0.iter().zip(digits).all(|(p, d)| p.is_none_or(|p| p == d))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Source {
    /// One or more tag patterns: `606`, `7..`, `300 301 302`.
    Tags(Vec<TagPat>),
    /// Once per record.
    Record,
    /// Fields that no other rule converted.
    Default,
}

#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub source: Source,
    /// `500[2..]`: occurrence range of the source field among the fields
    /// with its tag.
    pub rank: Range,
    pub conds: Vec<Cond>,
    pub target: Target,
}

#[derive(Clone, Debug)]
pub(crate) enum Target {
    Leader(Value),
    Control(u16, Value),
    Data { tag: u16, inds: Option<[Ind; 2]>, body: Body },
}

/// The block of a data field rule.
#[derive(Clone, Debug, Default)]
pub(crate) struct Body {
    pub lines: Vec<Line>,
    /// `order $a $b...`: target subfields sorted in this order.
    pub order: Vec<char>,
    /// `end "P" skip $x...`: final punctuation, added to the last
    /// subfield not skipped, unless it already ends with it.
    pub end: Option<(String, Vec<char>)>,
}

#[derive(Clone, Debug)]
pub(crate) enum Ind {
    Char(char),
    Expr(Expr),
}

/// A body line: `$c[range] -> outputs`, or `-> outputs` (no source
/// subfield: applied once per field).
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub lhs: Option<Lhs>,
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug)]
pub(crate) struct Lhs {
    /// `None` for `$*`.
    pub code: Option<char>,
    pub range: Range,
}

/// 1-based occurrence range, negative counting from the end; `None` for
/// an open end. `(1, None)` selects every occurrence.
pub(crate) type Range = (i64, Option<i64>);

/// Whether occurrence `k` (1-based) of `n` is in `range`.
pub(crate) fn in_range((start, end): Range, k: usize, n: usize) -> bool {
    let resolve = |i: i64| if i < 0 { n as i64 + 1 + i } else { i };
    let k = k as i64;
    k >= resolve(start) && end.is_none_or(|end| k <= resolve(end))
}

impl Lhs {
    /// Whether occurrence `k` (1-based) of `n` of `code` is selected.
    pub(crate) fn matches(&self, code: char, k: usize, n: usize) -> bool {
        self.code.is_none_or(|c| c == code) && in_range(self.range, k, n)
    }
}

/// `before "P" [after $c]`: punctuation added to the end of the preceding
/// target subfield (if its code is `$c`, when given).
#[derive(Clone, Debug)]
pub(crate) struct Before {
    pub punct: String,
    pub after: Option<char>,
}

#[derive(Clone, Debug)]
pub(crate) enum Output {
    /// `$c value [before "P" [after $d]]...`; `None` code for `$*` (same
    /// code as the source). The first `before` that applies is used.
    Sub(Option<char>, Value, Vec<Before>),
    /// `+= value`: appended to the last target subfield.
    Append(Value),
}

/// A template followed by functions applied to the rendered string.
#[derive(Clone, Debug)]
pub(crate) struct Value {
    pub template: Vec<Piece>,
    pub funcs: Vec<Func>,
}

#[derive(Clone, Debug)]
pub(crate) enum Piece {
    Text(String),
    Expr(Expr),
}

/// `{operand/pos|func|...}`
#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub operand: Operand,
    /// Character positions, inclusive: `/9` or `/9-12`.
    pub pos: Option<(usize, usize)>,
    pub funcs: Vec<Func>,
}

#[derive(Clone, Debug)]
pub(crate) enum Operand {
    /// `{}`: the value of the source subfield (or control field).
    Current,
    /// `{$b}`, `{$b[2]}`, `{$b*}`: subfields of the source field.
    Sub(char, Idx),
    /// `{100a}`, `{700,710}`, `{606[2]a}`, `{008}`: values in the record.
    Path { tags: Vec<TagPat>, field: Idx, code: Option<char>, sub: Idx },
    Literal(String),
    /// `{LDR}`: the leader of the record.
    Leader,
    /// `{@tag}`, `{@ind}`, `{@ind1}`, `{@ind2}`: of the source field.
    Tag,
    Ind,
    Ind1,
    Ind2,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Idx {
    /// 1-based, negative from the end.
    Nth(i64),
    All,
}

#[derive(Clone, Debug)]
pub(crate) enum Func {
    StripNsb,
    Nonfiling,
    Lower,
    Upper,
    Trim,
    Replace(Regex, String),
    /// Table index, value column (1-based).
    Table(usize, usize),
    /// Table index: each character looked up, kept if missing.
    Tr(usize),
    Default(String),
    Exists(String, String),
    Join(String),
}

/// `when [!]operand[=value|~regex]`
#[derive(Clone, Debug)]
pub(crate) struct Cond {
    pub negated: bool,
    pub operand: Operand,
    pub test: Test,
}

#[derive(Clone, Debug)]
pub(crate) enum Test {
    Exists,
    Equals(String),
    Matches(Regex),
}

/// A lookup table: key → value columns, and the columns of a missing key
/// (`* = ...`).
#[derive(Default)]
pub(crate) struct Table {
    pub entries: HashMap<String, Vec<String>>,
    pub default: Option<Vec<String>>,
}

/// The parsed content of a rules file.
pub(crate) struct Parsed {
    pub name: Option<String>,
    pub version: Option<String>,
    pub rules: Vec<Rule>,
    pub tables: Vec<Table>,
}

pub(crate) struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    file: &'a Path,
    /// Line of the first character, for templates parsed on their own.
    first_line: usize,
    table_names: Vec<String>,
}

type Result<T> = std::result::Result<T, Error>;

impl<'a> Parser<'a> {
    pub(crate) fn new(text: &str, file: &'a Path) -> Self {
        Parser { chars: text.chars().collect(), pos: 0, file, first_line: 1, table_names: Vec::new() }
    }

    fn line(&self) -> usize {
        self.first_line + self.chars[..self.pos.min(self.chars.len())].iter().filter(|c| **c == '\n').count()
    }

    pub(crate) fn error<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(Error { file: self.file.to_path_buf(), line: self.line(), message: message.into() })
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.chars.get(self.pos + i) == Some(&c))
    }

    fn eat(&mut self, s: &str) -> bool {
        let found = self.starts_with(s);
        if found {
            self.pos += s.chars().count();
        }
        found
    }

    fn expect(&mut self, s: &str) -> Result<()> {
        if self.eat(s) { Ok(()) } else { self.error(format!("expected `{s}`")) }
    }

    /// Skips blanks, newlines and comments (`#` followed by a space or
    /// the end of the line).
    pub(crate) fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.pos += 1,
                Some('#') if matches!(self.chars.get(self.pos + 1), None | Some(' ' | '\t' | '\n' | '\r')) => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.pos += 1;
                    }
                }
                _ => return,
            }
        }
    }

    /// Skips spaces and tabs only.
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.pos += 1;
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn word(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '*') {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn tag_pat(&self, word: &str) -> Option<TagPat> {
        let bytes = word.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(|b| b.is_ascii_digit() || *b == b'.') {
            return None;
        }
        Some(TagPat([0, 1, 2].map(|i| (bytes[i] != b'.').then(|| bytes[i] - b'0'))))
    }

    fn tag(&mut self) -> Result<u16> {
        let word = self.word();
        match word.parse::<u16>() {
            Ok(tag) if word.len() == 3 => Ok(tag),
            _ => self.error(format!("expected a 3-digit tag, found `{word}`")),
        }
    }

    /// A string literal, `"..."` or `'...'`. `\\`, `\"`, `\'`, `\n` and `\t`
    /// are escapes; any other backslash is kept, for regexes (`'\.$'`).
    fn string(&mut self) -> Result<String> {
        let quote = match self.peek() {
            Some(q @ ('"' | '\'')) => q,
            _ => return self.error("expected a string"),
        };
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return self.error("unterminated string"),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(s);
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some(c @ ('\\' | '"' | '\'')) => s.push(c),
                        Some(c) => {
                            s.push('\\');
                            s.push(c);
                        }
                        None => return self.error("unterminated string"),
                    }
                    self.pos += 1;
                }
                Some(c) => {
                    s.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn number(&mut self) -> Result<i64> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        match text.parse() {
            Ok(n) => Ok(n),
            Err(_) => self.error("expected a number"),
        }
    }

    /// The whole file.
    pub(crate) fn file(mut self) -> Result<Parsed> {
        let mut parsed = Parsed { name: None, version: None, rules: Vec::new(), tables: Vec::new() };
        loop {
            self.skip_ws();
            if self.at_end() {
                break;
            }
            let start = self.pos;
            let word = self.word();
            match word.as_str() {
                "rules" => {
                    self.skip_spaces();
                    parsed.name = Some(self.string()?);
                }
                "version" => {
                    self.skip_spaces();
                    parsed.version = Some(match self.peek() {
                        Some('"' | '\'') => self.string()?,
                        _ => self.word(),
                    });
                }
                "table" => {
                    self.skip_ws();
                    let name = self.word();
                    if name.is_empty() {
                        return self.error("expected a table name");
                    }
                    if self.table_names.contains(&name) {
                        return self.error(format!("table `{name}` already declared"));
                    }
                    self.skip_ws();
                    parsed.tables.push(self.table()?);
                    self.table_names.push(name);
                }
                _ => {
                    self.pos = start;
                    let rule = self.rule(&parsed.rules)?;
                    parsed.rules.push(rule);
                }
            }
        }
        Ok(parsed)
    }

    /// A table block, after its name: `{`, then one `key = value, ...`
    /// line per entry (`*` as key for missing keys), then `}`. Values are
    /// trimmed; quote one that has a comma or leading spaces.
    fn table(&mut self) -> Result<Table> {
        self.expect("{")?;
        let mut table = Table::default();
        loop {
            self.skip_ws();
            if self.eat("}") {
                return Ok(table);
            }
            if self.at_end() {
                return self.error("unterminated table, expected `}`");
            }
            let key = self.table_value(&['='])?;
            if !self.eat("=") {
                return self.error(format!("expected `=` after table key `{key}`"));
            }
            let mut values = Vec::new();
            loop {
                values.push(self.table_value(&[','])?);
                if !self.eat(",") {
                    break;
                }
            }
            self.skip_spaces();
            if !matches!(self.peek(), None | Some('\n' | '\r')) {
                return self.error("expected the end of the line after the table entry");
            }
            if key == "*" {
                table.default = Some(values);
            } else if table.entries.insert(key.clone(), values).is_some() {
                return self.error(format!("duplicate table key `{key}`"));
            }
        }
    }

    /// A table key or value: a string, or the text up to one of `stops`
    /// or the end of the line, trimmed.
    fn table_value(&mut self, stops: &[char]) -> Result<String> {
        self.skip_spaces();
        if matches!(self.peek(), Some('"' | '\'')) {
            let value = self.string()?;
            self.skip_spaces();
            return Ok(value);
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| !stops.contains(&c) && c != '\n' && c != '\r') {
            self.pos += 1;
        }
        Ok(self.chars[start..self.pos].iter().collect::<String>().trim().to_string())
    }

    /// `source [when cond]... -> target`
    fn rule(&mut self, previous: &[Rule]) -> Result<Rule> {
        let word = self.word();
        let source = match word.as_str() {
            "record" => Source::Record,
            "*" => Source::Default,
            _ => match self.tag_pat(&word) {
                Some(pat) => {
                    let mut pats = vec![pat];
                    loop {
                        let save = self.pos;
                        self.skip_spaces();
                        let word = self.word();
                        match self.tag_pat(&word) {
                            Some(pat) => pats.push(pat),
                            None => {
                                self.pos = save;
                                break;
                            }
                        }
                    }
                    Source::Tags(pats)
                }
                None => return self.error(format!("expected a rule, `rules`, `version` or `table`, found `{word}`")),
            },
        };
        let rank = if matches!(source, Source::Tags(_)) { self.range()? } else { (1, None) };
        let mut conds = Vec::new();
        loop {
            self.skip_ws();
            if self.eat("->") {
                break;
            }
            if self.word() != "when" {
                return self.error("expected `when` or `->`");
            }
            self.skip_ws();
            conds.push(self.cond()?);
        }
        self.skip_ws();
        let target = if self.eat("LDR") {
            self.skip_ws();
            Target::Leader(self.value()?)
        } else {
            let tag = self.tag()?;
            self.skip_ws();
            if tag < 10 {
                // Without a template, the source value is copied
                Target::Control(tag, self.value()?)
            } else {
                let inds = if self.eat("[") { Some(self.indicators()?) } else { None };
                self.skip_ws();
                let body = if self.eat("{") {
                    self.body()?
                } else if self.word() == "like" {
                    self.skip_ws();
                    let word = self.word();
                    let pat = self.tag_pat(&word);
                    let like = previous.iter().find(|rule| matches!(&rule.source, Source::Tags(p) if pat.as_ref().is_some_and(|pat| p.contains(pat))));
                    match like.map(|rule| &rule.target) {
                        Some(Target::Data { body, .. }) => body.clone(),
                        _ => return self.error(format!("no previous rule converting {word} to a data field")),
                    }
                } else {
                    return self.error("expected `{`, `like` or a template");
                };
                Target::Data { tag, inds, body }
            }
        };
        Ok(Rule { source, rank, conds, target })
    }

    /// The two indicators, after `[`: a character each (`#` for a blank)
    /// or a `{expression}`.
    fn indicators(&mut self) -> Result<[Ind; 2]> {
        let mut inds = Vec::new();
        while inds.len() < 2 {
            self.skip_spaces();
            match self.peek() {
                Some('{') => {
                    self.pos += 1;
                    inds.push(Ind::Expr(self.expr()?));
                    self.skip_spaces();
                    self.expect("}")?;
                }
                Some('#') => {
                    self.pos += 1;
                    inds.push(Ind::Char(' '));
                }
                Some(c) if c != ']' && !c.is_whitespace() => {
                    self.pos += 1;
                    inds.push(Ind::Char(c));
                }
                _ => return self.error("expected two indicators"),
            }
        }
        self.skip_spaces();
        self.expect("]")?;
        let [a, b]: [Ind; 2] = inds.try_into().unwrap();
        Ok([a, b])
    }

    /// Body lines, `order` and `end`, up to the closing `}`.
    fn body(&mut self) -> Result<Body> {
        let mut body = Body::default();
        loop {
            self.skip_ws();
            if self.eat(";") {
                continue;
            }
            if self.eat("}") {
                return Ok(body);
            }
            if self.at_end() {
                return self.error("unterminated block, expected `}`");
            }
            if self.keyword("order") {
                body.order = self.codes()?;
                continue;
            }
            if self.keyword("end") {
                self.skip_spaces();
                let punct = self.string()?;
                self.skip_spaces();
                let skip = if self.keyword("skip") { self.codes()? } else { Vec::new() };
                body.end = Some((punct, skip));
                continue;
            }
            let lhs = if self.eat("$") { Some(self.lhs()?) } else { None };
            self.skip_ws();
            self.expect("->")?;
            let mut outputs = Vec::new();
            loop {
                self.skip_ws();
                outputs.push(self.output()?);
                self.skip_ws();
                if !self.eat(",") {
                    break;
                }
            }
            body.lines.push(Line { lhs, outputs });
        }
    }

    /// Consumes `word` if it comes next as a whole word.
    fn keyword(&mut self, word: &str) -> bool {
        let next = self.chars.get(self.pos + word.chars().count());
        if self.starts_with(word) && !next.is_some_and(|c| c.is_alphanumeric()) {
            self.pos += word.chars().count();
            return true;
        }
        false
    }

    /// `$a $b ...` (at least one).
    fn codes(&mut self) -> Result<Vec<char>> {
        let mut codes = Vec::new();
        while { self.skip_spaces(); self.eat("$") } {
            match self.code()? {
                Some(code) => codes.push(code),
                None => return self.error("`$*` is not allowed here"),
            }
        }
        if codes.is_empty() {
            return self.error("expected subfield codes");
        }
        Ok(codes)
    }

    /// After `$`: `c`, `c[2]`, `c[2..]`, `c[1..3]`, `c[-1]`, `*`.
    fn lhs(&mut self) -> Result<Lhs> {
        let code = self.code()?;
        Ok(Lhs { code, range: self.range()? })
    }

    /// An optional `[2]`, `[2..]`, `[1..3]`, `[-1]`.
    fn range(&mut self) -> Result<Range> {
        if !self.eat("[") {
            return Ok((1, None));
        }
        let start = self.number()?;
        let range = if self.eat("..") {
            (start, if self.peek() == Some(']') { None } else { Some(self.number()?) })
        } else {
            (start, Some(start))
        };
        self.expect("]")?;
        Ok(range)
    }

    /// A subfield code, `None` for `*`.
    fn code(&mut self) -> Result<Option<char>> {
        match self.peek() {
            Some('*') => {
                self.pos += 1;
                Ok(None)
            }
            Some(c) if !c.is_whitespace() => {
                self.pos += 1;
                Ok(Some(c))
            }
            _ => self.error("expected a subfield code"),
        }
    }

    fn output(&mut self) -> Result<Output> {
        if self.eat("+=") {
            self.skip_spaces();
            return Ok(Output::Append(self.value()?));
        }
        self.expect("$")?;
        let code = self.code()?;
        self.skip_spaces();
        let value = self.value()?;
        let mut before = Vec::new();
        loop {
            let save = self.pos;
            self.skip_ws();
            if !self.keyword("before") {
                self.pos = save;
                break;
            }
            self.skip_spaces();
            let punct = self.string()?;
            let save = self.pos;
            self.skip_spaces();
            let after = if self.keyword("after") {
                self.skip_spaces();
                self.expect("$")?;
                match self.code()? {
                    Some(code) => Some(code),
                    None => return self.error("`$*` is not allowed after `after`"),
                }
            } else {
                self.pos = save;
                None
            };
            before.push(Before { punct, after });
        }
        Ok(Output::Sub(code, value, before))
    }

    /// An optional template (`"..."` or `{expr}`, `{}` if absent),
    /// followed by `| func`s.
    fn value(&mut self) -> Result<Value> {
        let template = match self.peek() {
            Some('"' | '\'') => {
                let line = self.line();
                let text = self.string()?;
                self.template(&text, line)?
            }
            Some('{') => {
                self.pos += 1;
                let expr = self.expr()?;
                self.skip_spaces();
                self.expect("}")?;
                vec![Piece::Expr(expr)]
            }
            _ => vec![Piece::Expr(Expr { operand: Operand::Current, pos: None, funcs: Vec::new() })],
        };
        let funcs = self.funcs()?;
        Ok(Value { template, funcs })
    }

    /// Splits a template string into text and `{expr}` pieces; `{{` and
    /// `}}` stand for literal braces.
    fn template(&self, text: &str, line: usize) -> Result<Vec<Piece>> {
        let chars: Vec<char> = text.chars().collect();
        let mut pieces = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '{' if chars.get(i + 1) == Some(&'{') => {
                    literal.push('{');
                    i += 2;
                }
                '}' if chars.get(i + 1) == Some(&'}') => {
                    literal.push('}');
                    i += 2;
                }
                '{' => {
                    // The matching `}`, quotes skipped
                    let mut j = i + 1;
                    let mut quote = None;
                    while j < chars.len() && (quote.is_some() || chars[j] != '}') {
                        match (quote, chars[j]) {
                            (None, q @ ('\'' | '"')) => quote = Some(q),
                            (Some(q), c) if c == q => quote = None,
                            _ => {}
                        }
                        j += 1;
                    }
                    if j == chars.len() {
                        return self.error("unterminated `{` in template");
                    }
                    if !literal.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut literal)));
                    }
                    let inner: String = chars[i + 1..j].iter().collect();
                    let mut parser = Parser::new(&inner, self.file);
                    parser.first_line = line;
                    parser.table_names = self.table_names.clone();
                    let expr = parser.expr()?;
                    parser.skip_spaces();
                    if !parser.at_end() {
                        return parser.error(format!("unexpected `{}` in `{{{inner}}}`", parser.peek().unwrap()));
                    }
                    pieces.push(Piece::Expr(expr));
                    i = j + 1;
                }
                c => {
                    literal.push(c);
                    i += 1;
                }
            }
        }
        if !literal.is_empty() {
            pieces.push(Piece::Text(literal));
        }
        Ok(pieces)
    }

    /// `operand[/pos][|func]...`, as found between braces.
    fn expr(&mut self) -> Result<Expr> {
        self.skip_spaces();
        let operand = self.operand()?;
        let pos = if self.eat("/") {
            let start = self.number()? as usize;
            let end = if self.eat("-") { self.number()? as usize } else { start };
            Some((start, end))
        } else {
            None
        };
        let funcs = self.funcs()?;
        Ok(Expr { operand, pos, funcs })
    }

    fn operand(&mut self) -> Result<Operand> {
        match self.peek() {
            None | Some('|' | '/' | '}') => Ok(Operand::Current),
            Some('$') => {
                self.pos += 1;
                match self.code()? {
                    Some(code) => Ok(Operand::Sub(code, self.idx()?)),
                    None => self.error("expected a subfield code after `$`"),
                }
            }
            Some('@') => {
                self.pos += 1;
                match self.word().as_str() {
                    "tag" => Ok(Operand::Tag),
                    "ind" => Ok(Operand::Ind),
                    "ind1" => Ok(Operand::Ind1),
                    "ind2" => Ok(Operand::Ind2),
                    other => self.error(format!("unknown `@{other}`, expected @tag, @ind, @ind1 or @ind2")),
                }
            }
            Some('"' | '\'') => Ok(Operand::Literal(self.string()?)),
            _ if self.eat("LDR") => Ok(Operand::Leader),
            _ => {
                let mut tags = Vec::new();
                loop {
                    let start = self.pos;
                    self.pos += 3.min(self.chars.len() - self.pos);
                    let word: String = self.chars[start..self.pos].iter().collect();
                    match self.tag_pat(&word) {
                        Some(pat) => tags.push(pat),
                        None => return self.error(format!("expected a tag, found `{word}`")),
                    }
                    if !self.eat(",") {
                        break;
                    }
                }
                let field = if self.peek() == Some('[') { self.idx()? } else { Idx::Nth(1) };
                let code = match self.peek() {
                    Some(c) if c.is_alphanumeric() => {
                        self.pos += 1;
                        Some(c)
                    }
                    _ => None,
                };
                let sub = self.idx()?;
                Ok(Operand::Path { tags, field, code, sub })
            }
        }
    }

    /// `[n]`, `*` or nothing (first).
    fn idx(&mut self) -> Result<Idx> {
        if self.eat("*") {
            return Ok(Idx::All);
        }
        if self.eat("[") {
            let n = self.number()?;
            self.expect("]")?;
            if n == 0 {
                return self.error("occurrences are numbered from 1");
            }
            return Ok(Idx::Nth(n));
        }
        Ok(Idx::Nth(1))
    }

    fn funcs(&mut self) -> Result<Vec<Func>> {
        let mut funcs = Vec::new();
        loop {
            let save = self.pos;
            self.skip_ws();
            if !self.eat("|") {
                self.pos = save;
                return Ok(funcs);
            }
            self.skip_spaces();
            funcs.push(self.func()?);
        }
    }

    fn func(&mut self) -> Result<Func> {
        let name = self.word();
        let mut args: Vec<String> = Vec::new();
        if self.eat("(") {
            loop {
                self.skip_spaces();
                if self.eat(")") {
                    break;
                }
                let arg = match self.peek() {
                    Some('"' | '\'') => self.string()?,
                    _ => {
                        let start = self.pos;
                        while self.peek().is_some_and(|c| !matches!(c, ',' | ')') && !c.is_whitespace()) {
                            self.pos += 1;
                        }
                        self.chars[start..self.pos].iter().collect()
                    }
                };
                args.push(arg);
                self.skip_spaces();
                if !self.eat(",") {
                    self.expect(")")?;
                    break;
                }
            }
        }
        let arity = |min: usize, max: usize| -> Result<()> {
            if args.len() < min || args.len() > max {
                return self.error(format!("{name}: expected {min} to {max} arguments, found {}", args.len()));
            }
            Ok(())
        };
        let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or_else(|| default.to_string());
        Ok(match name.as_str() {
            "strip_nsb" => { arity(0, 0)?; Func::StripNsb }
            "nonfiling" => { arity(0, 0)?; Func::Nonfiling }
            "lower" => { arity(0, 0)?; Func::Lower }
            "upper" => { arity(0, 0)?; Func::Upper }
            "trim" => { arity(0, 0)?; Func::Trim }
            "replace" => {
                arity(2, 2)?;
                match Regex::new(&args[0]) {
                    Ok(regex) => Func::Replace(regex, arg(1, "")),
                    Err(e) => return self.error(format!("replace: {e}")),
                }
            }
            "table" => {
                arity(1, 2)?;
                let Some(index) = self.table_names.iter().position(|t| *t == args[0]) else {
                    return self.error(format!("unknown table `{}` (declare it with `table` first)", args[0]));
                };
                match arg(1, "1").parse::<usize>() {
                    Ok(column) if column >= 1 => Func::Table(index, column),
                    _ => return self.error("table: the column is a number from 1"),
                }
            }
            "tr" => {
                arity(1, 1)?;
                match self.table_names.iter().position(|t| *t == args[0]) {
                    Some(index) => Func::Tr(index),
                    None => return self.error(format!("unknown table `{}` (declare it with `table` first)", args[0])),
                }
            }
            "default" => { arity(1, 1)?; Func::Default(arg(0, "")) }
            "exists" => { arity(1, 2)?; Func::Exists(arg(0, ""), arg(1, "")) }
            "join" => { arity(0, 1)?; Func::Join(arg(0, " ")) }
            _ => return self.error(format!("unknown function `{name}`")),
        })
    }

    /// After `when`: `[!]operand[=value|~regex]`.
    fn cond(&mut self) -> Result<Cond> {
        let negated = self.eat("!");
        let operand = self.operand()?;
        let bare = |parser: &mut Self| -> Result<String> {
            if matches!(parser.peek(), Some('"' | '\'')) {
                return parser.string();
            }
            let start = parser.pos;
            while parser.peek().is_some_and(|c| !c.is_whitespace()) {
                parser.pos += 1;
            }
            Ok(parser.chars[start..parser.pos].iter().collect())
        };
        let test = if self.eat("=") {
            Test::Equals(bare(self)?)
        } else if self.eat("~") {
            let regex = bare(self)?;
            match Regex::new(&regex) {
                Ok(regex) => Test::Matches(regex),
                Err(e) => return self.error(e.to_string()),
            }
        } else {
            Test::Exists
        };
        Ok(Cond { negated, operand, test })
    }
}
