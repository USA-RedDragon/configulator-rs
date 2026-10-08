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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
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
pub fn json_schema(type_name: &str, fields: &[FieldInfo], allow_unknown: bool) -> String {
    let mut root = schema_object(fields, allow_unknown);
    root.push((
        "$schema".into(),
        J::Str("http://json-schema.org/draft-07/schema#".into()),
    ));
    root.push(("title".into(), J::Str(type_name.into())));
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
        ScalarHint::String => "string",
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

fn schema_default(f: &FieldInfo, default: &str) -> Option<J> {
    match &f.field_type {
        FieldType::Bool => Some(J::Raw(default.parse::<bool>().ok()?.to_string())),
        FieldType::Scalar => Some(match f.scalar {
            ScalarHint::String => J::Str(default.into()),
            ScalarHint::Bool => J::Raw(default.parse::<bool>().ok()?.to_string()),
            ScalarHint::Integer => J::Raw(default.parse::<i128>().ok()?.to_string()),
            ScalarHint::Float => J::Raw(default.parse::<f64>().ok()?.to_string()),
        }),
        FieldType::List => Some(J::Arr(
            default.split(',').map(|p| J::Str(p.into())).collect(),
        )),
        _ => None,
    }
}

/// Render a commented YAML sample: every key at its default,
/// descriptions as comments. Always YAML, whatever loader the app uses.
pub fn sample_config(type_name: &str, fields: &[FieldInfo]) -> String {
    let mut b = format!("# Sample configuration for {type_name}.\n");
    sample_fields(&mut b, fields, 0);
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
                let live = if f.optional {
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

fn sample_value(f: &FieldInfo) -> String {
    if let Some(default) = f.default_value {
        return match &f.field_type {
            FieldType::Scalar if f.scalar == ScalarHint::String => format!("{default:?}"),
            FieldType::List => format!("[{default}]"),
            _ => default.to_string(),
        };
    }
    match &f.field_type {
        FieldType::Bool => "false".to_string(),
        FieldType::List => "[]".to_string(),
        FieldType::Scalar => match f.scalar {
            ScalarHint::String => "\"\"".to_string(),
            ScalarHint::Bool => "false".to_string(),
            ScalarHint::Integer => "0".to_string(),
            ScalarHint::Float => "0.0".to_string(),
        },
        _ => "\"\"".to_string(),
    }
}

/// Render a flat Markdown reference table of every config key: file
/// path, env var, flag, type, default, and description. Collections are
/// file-only, so their env and flag cells hold an em dash character.
/// Struct-collection element fields appear as `servers[].addr` and
/// `pools.<key>.size` rows.
pub fn markdown(
    type_name: &str,
    fields: &[FieldInfo],
    flag_sep: &str,
    env_prefix: &str,
    env_sep: &str,
) -> String {
    let mut rows: Vec<[String; 6]> = Vec::new();
    markdown_fields(
        &mut rows, fields, "", env_prefix, env_sep, "", flag_sep, true, true,
    );
    render_table(type_name, rows)
}

const HEADER: [&str; 6] = [
    "Key",
    "Type",
    "Default",
    "Environment",
    "Flag",
    "Description",
];

fn render_table(type_name: &str, rows: Vec<[String; 6]>) -> String {
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
    let mut b = format!("# {type_name} configuration\n\n");
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
            format!("`--{f_flag}`")
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
            Some(d) if !d.is_empty() && !f.secret => format!("`{d}`"),
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
