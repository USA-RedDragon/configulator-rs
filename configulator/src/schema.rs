//! JSON Schema and commented-sample-config rendering, built from the
//! derive-generated `FieldInfo` tree. The Rust counterpart of the Go
//! generator's `-schema` and `-sample` flags; output formats match.

use crate::field_info::{FieldInfo, FieldType, ScalarHint};

// Hand-rolled with sorted keys so output matches the Go generator byte for byte.

enum J {
    Str(String),
    /// Numbers and booleans, already rendered.
    Raw(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Spell `v` the way Go's `encoding/json` does: plain decimal, or
/// exponent form like `1e+21` and `1e-7` outside `[1e-6, 1e21)`. `None`
/// for NaN and infinities, which JSON cannot hold.
fn json_number(v: f64) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    let a = v.abs();
    if a != 0.0 && !(1e-6..1e21).contains(&a) {
        let e = format!("{v:e}");
        return Some(match e.split_once("e-") {
            Some(_) => e,
            None => e.replacen('e', "e+", 1),
        });
    }
    Some(v.to_string())
}

/// Whether Go's `strconv.IsPrint` holds for `c`: letters, marks, numbers,
/// punctuation, symbols, and the ASCII space.
fn is_go_print(c: char) -> bool {
    use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};
    c == ' '
        || matches!(
            c.general_category_group(),
            GeneralCategoryGroup::Letter
                | GeneralCategoryGroup::Mark
                | GeneralCategoryGroup::Number
                | GeneralCategoryGroup::Punctuation
                | GeneralCategoryGroup::Symbol
        )
}

/// Quote `s` like Go's `strconv.Quote`: a double-quoted string with Go
/// escapes (`\t`, `\x7f`, `\U000e0001`). Valid YAML too.
pub fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if is_go_print(c) => out.push(c),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push('"');
    out
}

/// Wrap `s` in a Markdown code span, with a double-backtick fence when `s`
/// holds a backtick.
fn code_span(s: &str) -> String {
    if s.contains('`') {
        format!("`` {s} ``")
    } else {
        format!("`{s}`")
    }
}

fn write_json(v: &J, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    let pad_in = "  ".repeat(indent + 1);
    match v {
        J::Str(s) => out.push_str(&format!("\"{}\"", esc(s))),
        J::Raw(s) => out.push_str(s),
        J::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad_in);
                write_json(item, indent + 1, out);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad);
            out.push(']');
        }
        J::Obj(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut sorted: Vec<&(String, J)> = pairs.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            out.push_str("{\n");
            for (i, (k, val)) in sorted.iter().enumerate() {
                out.push_str(&format!("{pad_in}\"{}\": ", esc(k)));
                write_json(val, indent + 1, out);
                if i + 1 < sorted.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&pad);
            out.push('}');
        }
    }
}

/// Render a JSON Schema (draft-07 subset) for a config type: types,
/// descriptions, defaults, required, and `additionalProperties: false`
/// unless the struct has `allow_unknown_fields`.
pub fn json_schema(fields: &[FieldInfo], allow_unknown: bool) -> String {
    let mut root = schema_object(&normalize_bools(fields), allow_unknown);
    root.push((
        "$schema".into(),
        J::Str("http://json-schema.org/draft-07/schema#".into()),
    ));
    root.push(("title".into(), J::Str("Configuration".into())));
    let mut out = String::new();
    write_json(&J::Obj(root), 0, &mut out);
    out.push('\n');
    out
}

fn schema_object(fields: &[FieldInfo], allow_unknown: bool) -> Vec<(String, J)> {
    let mut props = Vec::new();
    let mut required = Vec::new();
    for f in fields {
        props.push((f.config_name.to_string(), J::Obj(schema_field(f))));
        if f.required {
            required.push(J::Str(f.config_name.to_string()));
        }
    }
    let mut obj = vec![
        ("type".to_string(), J::Str("object".into())),
        ("properties".to_string(), J::Obj(props)),
    ];
    if !allow_unknown {
        obj.push(("additionalProperties".to_string(), J::Raw("false".into())));
    }
    if !required.is_empty() {
        obj.push(("required".into(), J::Arr(required)));
    }
    obj
}

fn scalar_type(hint: ScalarHint) -> &'static str {
    match hint {
        ScalarHint::String | ScalarHint::Duration => "string",
        ScalarHint::Integer => "integer",
        ScalarHint::Float => "number",
        ScalarHint::Bool => "boolean",
    }
}

fn schema_field(f: &FieldInfo) -> Vec<(String, J)> {
    let mut s = Vec::new();
    if let Some(desc) = f.description {
        s.push(("description".into(), J::Str(desc.into())));
    }
    match &f.field_type {
        FieldType::Bool => s.push(("type".into(), J::Str("boolean".into()))),
        FieldType::Scalar => s.push(("type".into(), J::Str(scalar_type(f.scalar).into()))),
        FieldType::Struct(sub) => s.extend(schema_object(sub, f.allow_unknown_fields)),
        FieldType::List => {
            s.push(("type".into(), J::Str("array".into())));
            s.push((
                "items".into(),
                J::Obj(vec![("type".into(), J::Str(scalar_type(f.scalar).into()))]),
            ));
        }
        FieldType::StructList(sub) => {
            s.push(("type".into(), J::Str("array".into())));
            s.push((
                "items".into(),
                J::Obj(schema_object(sub, f.allow_unknown_fields)),
            ));
        }
        FieldType::Map => {
            s.push(("type".into(), J::Str("object".into())));
            s.push((
                "additionalProperties".into(),
                J::Obj(vec![("type".into(), J::Str(scalar_type(f.scalar).into()))]),
            ));
        }
        FieldType::StructMap(sub) => {
            s.push(("type".into(), J::Str("object".into())));
            s.push((
                "additionalProperties".into(),
                J::Obj(schema_object(sub, f.allow_unknown_fields)),
            ));
        }
    }
    if let Some(default) = f.default_value.filter(|_| !f.secret) {
        if let Some(d) = schema_default(f, default) {
            s.push(("default".into(), d));
        }
    }
    s
}

/// Parse `s` like Go's `strconv.ParseBool`.
fn parse_go_bool(s: &str) -> Option<bool> {
    match s {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

/// `fields` with every bool default spelled `true` or `false`, the way the
/// Go generator normalizes `default:"1"` and friends. List defaults keep
/// their spelling.
fn normalize_bools(fields: &[FieldInfo]) -> Vec<FieldInfo> {
    fields
        .iter()
        .map(|f| {
            let mut f = f.clone();
            match &mut f.field_type {
                FieldType::Bool => {
                    if let Some(b) = f.default_value.and_then(parse_go_bool) {
                        f.default_value = Some(if b { "true" } else { "false" });
                    }
                }
                FieldType::Struct(sub) | FieldType::StructList(sub) | FieldType::StructMap(sub) => {
                    *sub = normalize_bools(sub);
                }
                _ => {}
            }
            f
        })
        .collect()
}

/// Spell a float default the way YAML reads NaN and infinities.
fn yaml_float(default: &str) -> String {
    match default.parse::<f64>() {
        Ok(v) if v.is_nan() => ".nan".into(),
        Ok(v) if v == f64::INFINITY => ".inf".into(),
        Ok(v) if v == f64::NEG_INFINITY => "-.inf".into(),
        _ => default.into(),
    }
}

/// `default` as a JSON value, or `None` when it doesn't parse or holds a
/// NaN or infinity, which JSON can't. A list is `None` when any element
/// is.
fn schema_default(f: &FieldInfo, default: &str) -> Option<J> {
    match &f.field_type {
        FieldType::Bool => Some(J::Raw(parse_go_bool(default)?.to_string())),
        FieldType::Scalar => Some(match f.scalar {
            ScalarHint::String | ScalarHint::Duration => J::Str(default.into()),
            ScalarHint::Bool => J::Raw(parse_go_bool(default)?.to_string()),
            ScalarHint::Integer => J::Raw(match default.parse::<i128>() {
                Ok(v) => v.to_string(),
                Err(_) => default.parse::<u128>().ok()?.to_string(),
            }),
            ScalarHint::Float => J::Raw(json_number(default.parse::<f64>().ok()?)?),
        }),
        FieldType::List => default
            .split(',')
            .map(|item| {
                schema_default(
                    &FieldInfo {
                        field_type: FieldType::Scalar,
                        ..f.clone()
                    },
                    item,
                )
            })
            .collect::<Option<Vec<J>>>()
            .map(J::Arr),
        _ => None,
    }
}

/// Render a commented YAML sample: every key at its default,
/// descriptions as comments. Always YAML, whatever loader the app uses.
pub fn sample_config(fields: &[FieldInfo]) -> String {
    let mut b = "# Sample configuration\n".to_string();
    sample_fields(&mut b, &normalize_bools(fields), 0);
    b
}

fn sample_fields(b: &mut String, fields: &[FieldInfo], depth: usize) {
    let ind = "  ".repeat(depth);
    for f in fields {
        if let Some(desc) = f.description {
            b.push_str(&format!("{ind}# {desc}\n"));
        }
        let tag = f.config_name;
        let commented_example = match &f.field_type {
            FieldType::Struct(sub) if !f.optional => {
                b.push_str(&format!("{ind}{tag}:\n"));
                sample_fields(b, sub, depth + 1);
                false
            }
            FieldType::Struct(_)
            | FieldType::StructList(_)
            | FieldType::StructMap(_)
            | FieldType::Map => true,
            _ => {
                let val = if f.secret {
                    "\"(secret)\"".to_string()
                } else {
                    sample_value(f)
                };
                let live = if f.secret {
                    false
                } else if f.optional {
                    f.default_value.is_some()
                } else {
                    f.default_value.is_some() || matches!(f.field_type, FieldType::Bool)
                };
                let hash = if live { "" } else { "# " };
                b.push_str(&format!("{ind}{hash}{tag}: {val}\n"));
                false
            }
        };
        if commented_example {
            for line in example_field(f) {
                b.push_str(&format!("{ind}# {line}\n"));
            }
        }
    }
}

/// Uncommented YAML lines for `f` with one example element in every
/// collection, at any depth.
fn example_field(f: &FieldInfo) -> Vec<String> {
    let tag = f.config_name;
    match &f.field_type {
        FieldType::Struct(sub) => {
            let mut lines = vec![format!("{tag}:")];
            lines.extend(indent(example_fields(sub), "  "));
            lines
        }
        FieldType::StructList(sub) => {
            let item = example_fields(sub);
            let Some((first, rest)) = item.split_first() else {
                return vec![format!("{tag}: []")];
            };
            let mut lines = vec![format!("{tag}:"), format!("  - {first}")];
            lines.extend(indent(rest.to_vec(), "    "));
            lines
        }
        FieldType::StructMap(sub) => {
            let mut lines = vec![format!("{tag}:"), "  example:".to_string()];
            lines.extend(indent(example_fields(sub), "    "));
            lines
        }
        FieldType::Map => {
            let elem = FieldInfo {
                default_value: None,
                field_type: FieldType::Scalar,
                ..f.clone()
            };
            vec![
                format!("{tag}:"),
                format!("  example: {}", sample_value(&elem)),
            ]
        }
        _ if f.secret => vec![format!("{tag}: \"(secret)\"")],
        _ => vec![format!("{tag}: {}", sample_value(f))],
    }
}

fn example_fields(fields: &[FieldInfo]) -> Vec<String> {
    fields.iter().flat_map(example_field).collect()
}

fn indent(lines: Vec<String>, prefix: &str) -> Vec<String> {
    lines.into_iter().map(|l| format!("{prefix}{l}")).collect()
}

fn quotes(hint: ScalarHint) -> bool {
    matches!(hint, ScalarHint::String | ScalarHint::Duration)
}

fn sample_value(f: &FieldInfo) -> String {
    if let Some(default) = f.default_value {
        return match &f.field_type {
            FieldType::Scalar if quotes(f.scalar) => go_quote(default),
            FieldType::Scalar if f.scalar == ScalarHint::Float => yaml_float(default),
            FieldType::List => {
                let items: Vec<String> = default
                    .split(',')
                    .map(|item| match f.scalar {
                        h if quotes(h) => go_quote(item),
                        ScalarHint::Float => yaml_float(item),
                        _ => item.to_string(),
                    })
                    .collect();
                format!("[{}]", items.join(", "))
            }
            _ => default.to_string(),
        };
    }
    match &f.field_type {
        FieldType::Bool => "false".to_string(),
        FieldType::List => "[]".to_string(),
        FieldType::Scalar => match f.scalar {
            ScalarHint::Duration => "\"0s\"".to_string(),
            ScalarHint::Bool => "false".to_string(),
            ScalarHint::Integer => "0".to_string(),
            ScalarHint::Float => "0.0".to_string(),
            _ => "\"\"".to_string(),
        },
        _ => "\"\"".to_string(),
    }
}

/// Render a flat Markdown reference table of every config key: file
/// path, env var, flag, type, default, and description. Collections are
/// file-only, so their env and flag cells hold an em dash character.
/// Struct-collection element fields appear as `servers[].addr` and
/// `pools.<key>.size` rows.
pub fn markdown(fields: &[FieldInfo], flag_sep: &str, env_prefix: &str, env_sep: &str) -> String {
    let mut rows: Vec<[String; 6]> = Vec::new();
    markdown_fields(
        &mut rows,
        &normalize_bools(fields),
        "",
        env_prefix,
        env_sep,
        "",
        flag_sep,
        true,
        true,
    );
    render_table(rows)
}

const HEADER: [&str; 6] = [
    "Key",
    "Type",
    "Default",
    "Environment",
    "Flag",
    "Description",
];

fn render_table(mut rows: Vec<[String; 6]>) -> String {
    for cell in rows.iter_mut().flatten() {
        *cell = cell.replace('|', "\\|");
    }
    let mut widths: Vec<usize> = HEADER.iter().map(|h| h.chars().count()).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let pad = |cell: &str, w: usize| {
        let mut out = cell.to_string();
        out.extend(std::iter::repeat_n(' ', w - cell.chars().count()));
        out
    };
    let mut b = "# Configuration\n\n".to_string();
    let header: Vec<String> = HEADER
        .iter()
        .zip(&widths)
        .map(|(h, w)| pad(h, *w))
        .collect();
    b.push_str(&format!("| {} |\n", header.join(" | ")));
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    b.push_str(&format!("|-{}-|\n", rule.join("-|-")));
    for row in rows {
        let cells: Vec<String> = row.iter().zip(&widths).map(|(c, w)| pad(c, *w)).collect();
        b.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    b
}

#[allow(clippy::too_many_arguments)]
fn markdown_fields(
    b: &mut Vec<[String; 6]>,
    fields: &[FieldInfo],
    path: &str,
    env: &str,
    env_sep: &str,
    flag: &str,
    flag_sep: &str,
    // false inside collections or under a field with env = "-" / flag = "-"
    env_ok: bool,
    flag_ok: bool,
) {
    for f in fields {
        let key = if path.is_empty() {
            f.config_name.to_string()
        } else {
            format!("{path}.{}", f.config_name)
        };
        let f_env = format!("{env}{}", f.env_segment);
        let f_flag = if flag.is_empty() {
            f.flag_segment.to_string()
        } else {
            format!("{flag}{flag_sep}{}", f.flag_segment)
        };
        let env_cell = if env_ok && !f.skip_env {
            format!("`{f_env}`")
        } else {
            "—".to_string()
        };
        let flag_cell = if flag_ok && !f.skip_cli {
            match f.short {
                Some(c) => format!("`-{c}`, `--{f_flag}`"),
                None => format!("`--{f_flag}`"),
            }
        } else {
            "—".to_string()
        };
        let desc = |f: &FieldInfo| {
            let mut d = f.description.unwrap_or("").to_string();
            if f.required {
                d = if d.is_empty() {
                    "required".into()
                } else {
                    format!("{d} (required)")
                };
            }
            if f.secret {
                d = if d.is_empty() {
                    "secret".into()
                } else {
                    format!("{d} (secret)")
                };
            }
            d
        };
        let default_cell = |f: &FieldInfo| match f.default_value {
            Some(d) if !d.is_empty() && !f.secret => code_span(d),
            _ => String::new(),
        };
        match &f.field_type {
            FieldType::Struct(sub) => {
                markdown_fields(
                    b,
                    sub,
                    &key,
                    &format!("{f_env}{env_sep}"),
                    env_sep,
                    &f_flag,
                    flag_sep,
                    env_ok && !f.skip_env,
                    flag_ok && !f.skip_cli,
                );
            }
            FieldType::StructList(sub) => {
                b.push([
                    format!("`{key}`"),
                    "list of objects".into(),
                    String::new(),
                    "—".into(),
                    "—".into(),
                    desc(f),
                ]);
                markdown_fields(
                    b,
                    sub,
                    &format!("{key}[]"),
                    "",
                    env_sep,
                    "",
                    flag_sep,
                    false,
                    false,
                );
            }
            FieldType::StructMap(sub) => {
                b.push([
                    format!("`{key}`"),
                    "map of objects".into(),
                    String::new(),
                    "—".into(),
                    "—".into(),
                    desc(f),
                ]);
                markdown_fields(
                    b,
                    sub,
                    &format!("{key}.<key>"),
                    "",
                    env_sep,
                    "",
                    flag_sep,
                    false,
                    false,
                );
            }
            FieldType::Map => {
                b.push([
                    format!("`{key}`"),
                    format!("map of {}", scalar_type(f.scalar)),
                    String::new(),
                    "—".into(),
                    "—".into(),
                    desc(f),
                ]);
            }
            FieldType::List => {
                b.push([
                    format!("`{key}`"),
                    format!("list of {}", scalar_type(f.scalar)),
                    default_cell(f),
                    env_cell,
                    flag_cell,
                    desc(f),
                ]);
            }
            FieldType::Bool | FieldType::Scalar => {
                let ty = match &f.field_type {
                    FieldType::Bool => "boolean",
                    _ => scalar_type(f.scalar),
                };
                b.push([
                    format!("`{key}`"),
                    ty.to_string(),
                    default_cell(f),
                    env_cell,
                    flag_cell,
                    desc(f),
                ]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values come from Go's `strconv.Quote`.
    #[test]
    fn go_quote_matches_go() {
        for (input, want) in [
            ("say \"hi\"", "\"say \\\"hi\\\"\""),
            ("back\\slash", "\"back\\\\slash\""),
            ("tab\u{9}here", "\"tab\\there\""),
            ("nl\u{a}x", "\"nl\\nx\""),
            ("cr\u{d}x", "\"cr\\rx\""),
            (
                "\u{0}\u{1}\u{7}\u{8}\u{b}\u{c}\u{1b}\u{1f}",
                "\"\\x00\\x01\\a\\b\\v\\f\\x1b\\x1f\"",
            ),
            ("del\u{7f}", "\"del\\x7f\""),
            ("caf\u{e9}", "\"caf\u{e9}\""),
            ("\u{1f600}", "\"\u{1f600}\""),
            ("\u{a0}", "\"\\u00a0\""),
            ("\u{ad}", "\"\\u00ad\""),
            ("\u{200b}", "\"\\u200b\""),
            ("\u{200c}", "\"\\u200c\""),
            ("\u{2028}", "\"\\u2028\""),
            ("\u{3000}", "\"\\u3000\""),
            ("\u{e000}", "\"\\ue000\""),
            ("\u{f0000}", "\"\\U000f0000\""),
            ("\u{feff}", "\"\\ufeff\""),
            ("\u{fffe}", "\"\\ufffe\""),
            ("\u{378}", "\"\\u0378\""),
            ("\u{e0001}", "\"\\U000e0001\""),
            ("\u{85}", "\"\\u0085\""),
            ("\u{61c}", "\"\\u061c\""),
            ("<&>", "\"<&>\""),
            ("\u{1680}", "\"\\u1680\""),
            ("\u{300}x", "\"\u{300}x\""),
            ("x\u{300}", "\"x\u{300}\""),
            ("\u{1f1e6}", "\"\u{1f1e6}\""),
            ("\u{1f3fb}", "\"\u{1f3fb}\""),
            ("\u{e0020}", "\"\\U000e0020\""),
            ("\u{10ffff}", "\"\\U0010ffff\""),
        ] {
            assert_eq!(go_quote(input), want, "{input:?}");
        }
    }

    /// Expected values follow the Go generator's `yamlFloat`.
    #[test]
    fn yaml_floats_match_go() {
        for (input, want) in [
            ("NaN", ".nan"),
            ("nan", ".nan"),
            ("inf", ".inf"),
            ("+Inf", ".inf"),
            ("Infinity", ".inf"),
            ("-inf", "-.inf"),
            ("1e21", "1e21"),
            ("-0", "-0"),
            ("x", "x"),
        ] {
            assert_eq!(yaml_float(input), want, "{input:?}");
        }
    }

    #[test]
    fn bool_defaults_parse_like_go() {
        for s in ["1", "t", "T", "TRUE", "true", "True"] {
            assert_eq!(parse_go_bool(s), Some(true), "{s:?}");
        }
        for s in ["0", "f", "F", "FALSE", "false", "False"] {
            assert_eq!(parse_go_bool(s), Some(false), "{s:?}");
        }
        for s in ["yes", "tRUE", "", " true"] {
            assert_eq!(parse_go_bool(s), None, "{s:?}");
        }
    }

    /// Expected values come from Go's `encoding/json/v2`.
    #[test]
    fn json_strings_match_go() {
        for (input, want) in [
            ("say \"hi\"", "\"say \\\"hi\\\"\""),
            ("back\\slash", "\"back\\\\slash\""),
            ("tab\u{9}here", "\"tab\\there\""),
            ("nl\u{a}x", "\"nl\\nx\""),
            ("cr\u{d}x", "\"cr\\rx\""),
            (
                "\u{0}\u{1}\u{7}\u{8}\u{b}\u{c}\u{1b}\u{1f}",
                "\"\\u0000\\u0001\\u0007\\b\\u000b\\f\\u001b\\u001f\"",
            ),
            ("del\u{7f}", "\"del\u{7f}\""),
            ("caf\u{e9}", "\"caf\u{e9}\""),
            ("\u{1f600}", "\"\u{1f600}\""),
            ("\u{a0}", "\"\u{a0}\""),
            ("\u{ad}", "\"\u{ad}\""),
            ("\u{200b}", "\"\u{200b}\""),
            ("\u{200c}", "\"\u{200c}\""),
            ("\u{2028}", "\"\u{2028}\""),
            ("\u{3000}", "\"\u{3000}\""),
            ("\u{e000}", "\"\u{e000}\""),
            ("\u{f0000}", "\"\u{f0000}\""),
            ("\u{feff}", "\"\u{feff}\""),
            ("\u{fffe}", "\"\u{fffe}\""),
            ("\u{378}", "\"\u{378}\""),
            ("\u{e0001}", "\"\u{e0001}\""),
            ("\u{85}", "\"\u{85}\""),
            ("\u{61c}", "\"\u{61c}\""),
            ("<&>", "\"<&>\""),
            ("\u{1680}", "\"\u{1680}\""),
            ("\u{300}x", "\"\u{300}x\""),
            ("x\u{300}", "\"x\u{300}\""),
            ("\u{1f1e6}", "\"\u{1f1e6}\""),
            ("\u{1f3fb}", "\"\u{1f3fb}\""),
            ("\u{e0020}", "\"\u{e0020}\""),
            ("\u{10ffff}", "\"\u{10ffff}\""),
        ] {
            assert_eq!(format!("\"{}\"", esc(input)), want, "{input:?}");
        }
    }

    /// Expected values come from Go's `encoding/json/v2`.
    #[test]
    #[allow(clippy::excessive_precision)]
    fn json_numbers_match_go() {
        for (input, want) in [
            (0.0_f64, "0"),
            (1.0_f64, "1"),
            (1.5_f64, "1.5"),
            (-2.25_f64, "-2.25"),
            (1000.0_f64, "1000"),
            (1e+20_f64, "100000000000000000000"),
            (1e+21_f64, "1e+21"),
            (1e-06_f64, "0.000001"),
            (1e-07_f64, "1e-7"),
            (1.234567e+06_f64, "1234567"),
            (1.23456789012e+11_f64, "123456789012"),
            (0.1_f64, "0.1"),
            (1e+300_f64, "1e+300"),
            (-1e+21_f64, "-1e+21"),
            (5e-324_f64, "5e-324"),
            (1.7976931348623157e+308_f64, "1.7976931348623157e+308"),
            (1e+15_f64, "1000000000000000"),
            (1e+16_f64, "10000000000000000"),
            (1e-05_f64, "0.00001"),
            (1.2345e-05_f64, "0.000012345"),
            (0.10000000149011612_f64, "0.10000000149011612"),
            (1.5e-07_f64, "1.5e-7"),
            (1.23456789e+28_f64, "1.23456789e+28"),
        ] {
            assert_eq!(json_number(input).as_deref(), Some(want), "{input:?}");
        }
        assert_eq!(json_number(f64::NAN), None);
        assert_eq!(json_number(f64::INFINITY), None);
    }
}
