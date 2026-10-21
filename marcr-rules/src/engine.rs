//! Application of parsed rules to a record.

use marcr::{Field, Record, Subfield};

use crate::syntax::{Before, Body, Cond, Expr, Func, Idx, Ind, Operand, Output, Parsed, Piece, Source, Table, Target, Test, Value};

/// What templates and conditions can refer to.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    record: &'a Record,
    /// The source field, if the rule has one.
    field: Option<&'a Field>,
    /// The source subfield value (`{}`).
    current: Option<&'a str>,
    tables: &'a [Table],
}

/// Converts `record` with `parsed` rules.
pub(crate) fn apply(parsed: &Parsed, record: &Record) -> Record {
    let mut target = Record { leader: record.leader, fields: Vec::new() };
    let base = Ctx { record, field: None, current: None, tables: &parsed.tables };
    let produce = |rule_target: &Target, ctx: Ctx, target: &mut Record| match rule_target {
        Target::Leader(value) => {
            let leader = render(value, ctx);
            let mut bytes = [b' '; 24];
            for (b, c) in bytes.iter_mut().zip(leader.bytes()) {
                *b = c;
            }
            target.leader = bytes;
        }
        Target::Control(tag, value) => {
            let value = render(value, ctx);
            if !value.is_empty() {
                target.fields.push(Field::Control(*tag, value));
            }
        }
        Target::Data { tag, inds, body } => {
            if let Some(field) = data_field(*tag, inds.as_ref(), body, ctx) {
                target.fields.push(field);
            }
        }
    };
    for rule in parsed.rules.iter().filter(|rule| matches!(rule.source, Source::Record)) {
        if rule.conds.iter().all(|cond| test(cond, base)) {
            produce(&rule.target, base, &mut target);
        }
    }
    for field in &record.fields {
        let ctx = Ctx { field: Some(field), ..base };
        let mut converted = false;
        for rule in &parsed.rules {
            if matches!(&rule.source, Source::Tags(pats) if pats.iter().any(|pat| pat.matches(*field.tag())))
                && rule.conds.iter().all(|cond| test(cond, ctx))
            {
                produce(&rule.target, ctx, &mut target);
                converted = true;
            }
        }
        if !converted {
            for rule in parsed.rules.iter().filter(|rule| matches!(rule.source, Source::Default)) {
                if rule.conds.iter().all(|cond| test(cond, ctx)) {
                    produce(&rule.target, ctx, &mut target);
                }
            }
        }
    }
    target.fields.sort_by_key(|field| *field.tag()); // stable
    target
}

/// Builds a data field from the body; `None` if it has no subfield.
fn data_field(tag: u16, inds: Option<&[Ind; 2]>, body: &Body, ctx: Ctx) -> Option<Field> {
    // Each target subfield with the punctuation to put before it
    let mut subfields: Vec<(Subfield, &[Before])> = Vec::new();
    fn emit<'b>(outputs: &'b [Output], ctx: Ctx, source_code: Option<char>, subfields: &mut Vec<(Subfield, &'b [Before])>) {
        for output in outputs {
            match output {
                Output::Sub(code, value, before) => {
                    // A literal is produced as written, even empty
                    let literal = value.template.iter().all(|piece| matches!(piece, Piece::Text(_)));
                    let text = render(value, ctx);
                    if let Some(code) = code.or(source_code)
                        && (literal || !text.is_empty())
                    {
                        let value = text;
                        subfields.push((Subfield(code, value), before));
                    }
                }
                Output::Append(value) => {
                    if let Some((last, _)) = subfields.last_mut() {
                        last.1.push_str(&render(value, ctx));
                    }
                }
            }
        }
    }
    // Lines without a source subfield first, once
    for line in body.lines.iter().filter(|line| line.lhs.is_none()) {
        emit(&line.outputs, ctx, None, &mut subfields);
    }
    let source: &[Subfield] = match ctx.field {
        Some(Field::Standard(_, _, subfields)) => subfields,
        _ => &[],
    };
    for (i, Subfield(code, value)) in source.iter().enumerate() {
        let k = source[..=i].iter().filter(|s| s.0 == *code).count();
        let n = source.iter().filter(|s| s.0 == *code).count();
        let ctx = Ctx { current: Some(value), ..ctx };
        for line in &body.lines {
            if line.lhs.as_ref().is_some_and(|lhs| lhs.matches(*code, k, n)) {
                emit(&line.outputs, ctx, Some(*code), &mut subfields);
            }
        }
    }
    if subfields.is_empty() {
        return None;
    }
    if !body.order.is_empty() {
        // Stable: repeated codes keep their order; unlisted codes go last
        let order = &body.order;
        subfields.sort_by_key(|(Subfield(code, _), _)| order.iter().position(|c| c == code).unwrap_or(order.len()));
    }
    for i in 1..subfields.len() {
        let previous = subfields[i - 1].0.0;
        if let Some(before) = subfields[i].1.iter().find(|before| before.after.is_none_or(|after| after == previous)) {
            let punct = before.punct.clone();
            subfields[i - 1].0.1.push_str(&punct);
        }
    }
    let mut subfields: Vec<Subfield> = subfields.into_iter().map(|(subfield, _)| subfield).collect();
    if let Some((end, skip)) = &body.end
        && let Some(last) = subfields.iter_mut().rev().find(|Subfield(code, _)| !skip.contains(code))
        && !last.1.ends_with(end.as_str())
    {
        last.1.push_str(end);
    }
    let source_inds = match ctx.field {
        Some(Field::Standard(_, ind, _)) => *ind,
        _ => [' ', ' '],
    };
    let ind = match inds {
        None => source_inds,
        Some(inds) => inds.clone().map(|ind| match ind {
            Ind::Char(c) => c,
            Ind::Expr(expr) => match eval(&expr, ctx).concat().chars().next() {
                Some('#') | None => ' ',
                Some(c) => c,
            },
        }),
    };
    Some(Field::Standard(tag, ind, subfields))
}

fn render(value: &Value, ctx: Ctx) -> String {
    let text: String = value.template.iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.clone(),
            Piece::Expr(expr) => eval(expr, ctx).join(" "),
        })
        .collect();
    funcs(vec![text], &value.funcs, ctx.tables).concat()
}

fn eval(expr: &Expr, ctx: Ctx) -> Vec<String> {
    let mut values = operand(&expr.operand, ctx, false);
    if let Some((start, end)) = expr.pos {
        for value in values.iter_mut() {
            *value = value.chars().skip(start).take(end + 1 - start.min(end + 1)).collect();
        }
    }
    funcs(values, &expr.funcs, ctx.tables)
}

/// The values of `operand`. With `any`, a subfield or path without an
/// explicit rank gives all occurrences (for conditions).
fn operand(operand: &Operand, ctx: Ctx, any: bool) -> Vec<String> {
    let rank = |idx: Idx| if any && matches!(idx, Idx::Nth(1)) { Idx::All } else { idx };
    let ind = |i: usize| match ctx.field {
        Some(Field::Standard(_, ind, _)) => ind[i],
        _ => ' ',
    };
    match operand {
        Operand::Current => match (ctx.current, ctx.field) {
            (Some(value), _) => vec![value.to_string()],
            (None, Some(Field::Control(_, value))) => vec![value.clone()],
            _ => Vec::new(),
        },
        Operand::Sub(code, idx) => match ctx.field {
            Some(field) => pick(field.subfields(*code).map(str::to_string).collect(), rank(*idx)),
            None => Vec::new(),
        },
        Operand::Path { tags, field, code, sub } => {
            let fields: Vec<&Field> = ctx.record.fields.iter()
                .filter(|f| tags.iter().any(|pat| pat.matches(*f.tag())))
                .collect();
            pick(fields, rank(*field)).into_iter()
                .flat_map(|f| match code {
                    Some(code) => pick(f.subfields(*code).map(str::to_string).collect(), rank(*sub)),
                    None => vec![field_value(f)],
                })
                .collect()
        }
        Operand::Literal(s) => vec![s.clone()],
        Operand::Leader => vec![String::from_utf8_lossy(&ctx.record.leader).into_owned()],
        Operand::Tag => ctx.field.map(|f| format!("{:03}", f.tag())).into_iter().collect(),
        Operand::Ind => vec![[ind(0), ind(1)].iter().collect()],
        Operand::Ind1 => vec![ind(0).to_string()],
        Operand::Ind2 => vec![ind(1).to_string()],
    }
}

/// Occurrence `idx` of `items` (1-based, negative from the end), or all.
fn pick<T>(mut items: Vec<T>, idx: Idx) -> Vec<T> {
    match idx {
        Idx::All => items,
        Idx::Nth(n) => {
            let i = if n < 0 { items.len() as i64 + n } else { n - 1 };
            if i >= 0 && (i as usize) < items.len() { vec![items.swap_remove(i as usize)] } else { Vec::new() }
        }
    }
}

/// A control field's value, or a standard field's subfields joined by a
/// space.
fn field_value(field: &Field) -> String {
    match field {
        Field::Control(_, value) => value.clone(),
        Field::Standard(_, _, subfields) => subfields.iter().map(|s| s.1.as_str()).collect::<Vec<_>>().join(" "),
    }
}

const NSB: char = '\u{98}';
const NSE: char = '\u{9c}';

fn funcs(mut values: Vec<String>, funcs: &[Func], tables: &[Table]) -> Vec<String> {
    for func in funcs {
        values = match func {
            Func::StripNsb => values.into_iter().map(|v| v.replace([NSB, NSE], "")).collect(),
            Func::Nonfiling => values.into_iter()
                .map(|v| match (v.strip_prefix(NSB), v.find(NSE)) {
                    (Some(rest), Some(_)) => rest.chars().take_while(|c| *c != NSE).count().to_string(),
                    _ => String::from("0"),
                })
                .collect(),
            Func::Lower => values.into_iter().map(|v| v.to_lowercase()).collect(),
            Func::Upper => values.into_iter().map(|v| v.to_uppercase()).collect(),
            Func::Trim => values.into_iter().map(|v| v.trim().to_string()).collect(),
            Func::Replace(regex, with) => values.into_iter().map(|v| regex.replace_all(&v, with.as_str()).into_owned()).collect(),
            Func::Table(table, column) => values.into_iter()
                .filter_map(|v| {
                    let table = &tables[*table];
                    table.entries.get(&v).or(table.default.as_ref()).and_then(|columns| columns.get(column - 1)).cloned()
                })
                .collect(),
            Func::Tr(table) => values.into_iter()
                .map(|v| {
                    let table = &tables[*table];
                    v.chars()
                        .map(|c| match table.entries.get(c.encode_utf8(&mut [0; 4]) as &str).and_then(|values| values.first()) {
                            Some(to) => to.clone(),
                            None => c.to_string(),
                        })
                        .collect()
                })
                .collect(),
            Func::Default(default) => {
                if values.iter().all(String::is_empty) { vec![default.clone()] } else { values }
            }
            Func::Exists(yes, no) => {
                vec![if values.iter().any(|v| !v.is_empty()) { yes.clone() } else { no.clone() }]
            }
            Func::Join(separator) => {
                let values: Vec<String> = values.into_iter().filter(|v| !v.is_empty()).collect();
                if values.is_empty() { values } else { vec![values.join(separator)] }
            }
        };
    }
    values
}

fn test(cond: &Cond, ctx: Ctx) -> bool {
    let values = operand(&cond.operand, ctx, true);
    let found = match &cond.test {
        Test::Exists => values.iter().any(|v| !v.is_empty()),
        Test::Equals(expected) => values.iter().any(|v| v == expected),
        Test::Matches(regex) => values.iter().any(|v| regex.is_match(v)),
    };
    found != cond.negated
}
