//! Prints a JSON Schema, sample config, or Markdown table for a
//! `#[derive(Config)]` struct, like the Go generator's `-schema`/`-sample`/
//! `-markdown` flags.
//!
//! Reads the crate's source with syn instead of compiling it. Rendering
//! uses the runtime crate's emitters, so output matches
//! `Configulator::json_schema()` and friends.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use configulator::{FieldInfo, FieldType, ScalarHint};
use syn::ext::IdentExt;

#[derive(Parser)]
#[command(
    name = "configulator",
    about = "Print a JSON Schema, sample config, or Markdown table for a #[derive(Config)] struct"
)]
struct Args {
    /// Config root type name (e.g. AppConfig)
    #[arg(long, short = 't')]
    r#type: String,

    /// Print a JSON Schema to stdout
    #[arg(long)]
    schema: bool,

    /// Print a sample config to stdout (see --format)
    #[arg(long)]
    sample: bool,

    /// Sample format: yaml (commented), json, or toml
    #[arg(long, default_value = "yaml")]
    format: String,

    /// Print a Markdown reference table of every key to stdout
    #[arg(long)]
    markdown: bool,

    /// Env var prefix used in the Markdown table (verbatim, e.g. "MYAPP_")
    #[arg(long, default_value = "")]
    env_prefix: String,

    /// Env var separator used in the Markdown table
    #[arg(long, default_value = "_")]
    env_separator: String,

    /// Flag separator used in the Markdown table
    #[arg(long, default_value = ".")]
    flag_separator: String,

    /// With --markdown, write the table between the
    /// <!-- configulator:begin --> and <!-- configulator:end --> markers in
    /// this file instead of stdout
    #[arg(long)]
    markdown_file: Option<PathBuf>,

    /// With --sample, write the sample to this file instead of stdout
    #[arg(long)]
    sample_file: Option<PathBuf>,

    /// With --markdown-file or --sample-file, change nothing and exit 1 if
    /// the file is out of date
    #[arg(long)]
    check: bool,

    /// Crate directory to scan for .rs files (target/ is skipped)
    #[arg(long, default_value = ".")]
    dir: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if let Err(e) = check_usage(&args) {
        eprintln!("configulator: {e}");
        return ExitCode::from(2);
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("configulator: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Reject flag combinations that can't work, before scanning any source.
/// The caller exits 2 on an error, like the Go generator.
fn check_usage(args: &Args) -> Result<(), String> {
    if args.markdown_file.is_some() && !args.markdown {
        return Err("--markdown-file needs --markdown".into());
    }
    if args.sample_file.is_some() && !args.sample {
        return Err("--sample-file needs --sample".into());
    }
    if args.check && args.markdown_file.is_none() && args.sample_file.is_none() {
        return Err("--check needs --markdown-file or --sample-file".into());
    }
    match [args.schema, args.sample, args.markdown]
        .iter()
        .filter(|b| **b)
        .count()
    {
        0 => return Err("pass one of --schema, --sample, or --markdown".into()),
        1 => {}
        _ => {
            return Err(
                "pass at most one of --schema, --sample, --markdown; output goes to stdout, pipe it where you want it"
                    .into(),
            )
        }
    }
    if args.format != "yaml" && !args.sample {
        return Err("--format only applies to --sample".into());
    }
    Ok(())
}

/// Quote `s` for a POSIX shell when it needs it.
fn shell_quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./:=,@+%".contains(c));
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// The command that rewrites the out-of-date file: the same type, dir,
/// mode and every non-default option, so it reproduces the same output.
fn rerun_command(args: &Args, path: &Path) -> String {
    let mut cmd = vec![
        "configulator".to_string(),
        "--type".into(),
        shell_quote(&args.r#type),
    ];
    if args.dir != Path::new(".") {
        cmd.push("--dir".into());
        cmd.push(shell_quote(&args.dir.to_string_lossy()));
    }
    let mut opt = |name: &str, value: &str, default: &str| {
        if value != default {
            cmd.push(name.into());
            cmd.push(shell_quote(value));
        }
    };
    if args.markdown {
        opt("--env-prefix", &args.env_prefix, "");
        opt("--env-separator", &args.env_separator, "_");
        opt("--flag-separator", &args.flag_separator, ".");
        cmd.push("--markdown".into());
        cmd.push("--markdown-file".into());
    } else {
        opt("--format", &args.format, "yaml");
        cmd.push("--sample".into());
        cmd.push("--sample-file".into());
    }
    cmd.push(shell_quote(&path.to_string_lossy()));
    cmd.join(" ")
}

fn run(args: &Args) -> Result<(), String> {
    let structs = collect_structs(&args.dir)?;
    if structs.is_empty() {
        return Err(format!("no structs found under {}", args.dir.display()));
    }
    let fields = build_fields(&args.r#type, &structs, &mut Vec::new())?;

    let body = if args.schema {
        configulator::__schema::json_schema(&fields, allows_unknown(&structs[&args.r#type]))
    } else if args.sample {
        let sample = encode_sample(&fields, &args.format)?;
        if let Some(path) = &args.sample_file {
            let old = match std::fs::read_to_string(path) {
                Ok(s) => Some(s),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("{}: {e}", path.display())),
            };
            if old.as_deref() == Some(sample.as_str()) {
                return Ok(());
            }
            if args.check {
                return Err(format!(
                    "{} is out of date; run {}",
                    path.display(),
                    rerun_command(args, path)
                ));
            }
            std::fs::write(path, &sample).map_err(|e| format!("{}: {e}", path.display()))?;
            eprintln!("configulator: updated {}", path.display());
            return Ok(());
        }
        sample
    } else {
        let table = configulator::__schema::markdown(
            &fields,
            &args.flag_separator,
            &args.env_prefix,
            &args.env_separator,
        );
        if let Some(path) = &args.markdown_file {
            let table = table.split_once("\n\n").map_or(table.as_str(), |(_, t)| t);
            match update_markdown_file(path, table, args.check)? {
                Update::Unchanged => {}
                Update::Written => eprintln!("configulator: updated {}", path.display()),
                Update::Stale => {
                    return Err(format!(
                        "{} is out of date; run {}",
                        path.display(),
                        rerun_command(args, path)
                    ))
                }
            }
            return Ok(());
        }
        table
    };
    print!("{body}");
    Ok(())
}

const MARKDOWN_BEGIN: &str = "<!-- configulator:begin -->";
const MARKDOWN_END: &str = "<!-- configulator:end -->";

/// Replace everything between the begin and end markers in `doc` with
/// `table`, keeping the markers.
fn splice_markdown(doc: &str, table: &str) -> Result<String, String> {
    let (Some(begin), Some(end)) = (doc.find(MARKDOWN_BEGIN), doc.find(MARKDOWN_END)) else {
        return Err(format!(
            "needs a {MARKDOWN_BEGIN} line followed by a {MARKDOWN_END} line"
        ));
    };
    if end < begin {
        return Err(format!(
            "needs a {MARKDOWN_BEGIN} line followed by a {MARKDOWN_END} line"
        ));
    }
    if doc[end + MARKDOWN_END.len()..].contains(MARKDOWN_BEGIN) {
        return Err(format!("has more than one {MARKDOWN_BEGIN} marker"));
    }
    Ok(format!(
        "{}\n\n{}\n\n{}",
        &doc[..begin + MARKDOWN_BEGIN.len()],
        table.trim_end_matches('\n'),
        &doc[end..]
    ))
}

/// What [`update_markdown_file`] did.
#[derive(Debug, PartialEq)]
enum Update {
    Unchanged,
    Written,
    /// The file is out of date and `check` kept it as is.
    Stale,
}

/// Write `table` into `path` between the markers. With `check` set, write
/// nothing and report a file that would change as stale.
fn update_markdown_file(path: &Path, table: &str, check: bool) -> Result<Update, String> {
    let doc = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let out = splice_markdown(&doc, table).map_err(|e| format!("{}: {e}", path.display()))?;
    if out == doc {
        return Ok(Update::Unchanged);
    }
    if check {
        return Ok(Update::Stale);
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Update::Written)
}

fn collect_structs(dir: &Path) -> Result<HashMap<String, syn::ItemStruct>, String> {
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    let mut out = HashMap::new();
    for file in files {
        let src = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        collect_items(&parsed.items, &mut out);
    }
    Ok(out)
}

fn collect_items(items: &[syn::Item], out: &mut HashMap<String, syn::ItemStruct>) {
    for item in items {
        match item {
            syn::Item::Struct(s) => {
                out.entry(s.ident.to_string()).or_insert_with(|| s.clone());
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    collect_items(items, out);
                }
            }
            _ => {}
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk(&path, out)?;
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[derive(Default)]
struct Attrs {
    name: Option<String>,
    default: Option<String>,
    description: Option<String>,
    nested: bool,
    secret: bool,
    required: bool,
    short: Option<char>,
    env: Option<String>,
    flag: Option<String>,
}

fn parse_attrs(attrs: &[syn::Attribute]) -> Result<Attrs, String> {
    let mut out = Attrs::default();
    for attr in attrs {
        if !attr.path().is_ident("configulator") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            let get = |meta: &syn::meta::ParseNestedMeta| -> syn::Result<String> {
                let lit: syn::LitStr = meta.value()?.parse()?;
                Ok(lit.value())
            };
            if meta.path.is_ident("name") {
                out.name = Some(get(&meta)?);
            } else if meta.path.is_ident("default") {
                let default = get(&meta)?;
                if default.is_empty() {
                    return Err(meta.error("empty default; remove it"));
                }
                out.default = Some(default);
            } else if meta.path.is_ident("description") {
                out.description = Some(get(&meta)?);
            } else if meta.path.is_ident("nested") {
                out.nested = true;
            } else if meta.path.is_ident("secret") {
                out.secret = true;
            } else if meta.path.is_ident("required") {
                out.required = true;
            } else if meta.path.is_ident("short") {
                out.short = Some(meta.value()?.parse::<syn::LitChar>()?.value());
            } else if meta.path.is_ident("env") {
                out.env = Some(get(&meta)?);
            } else if meta.path.is_ident("flag") {
                out.flag = Some(get(&meta)?);
            } else {
                let _ = meta.value().and_then(|v| v.parse::<syn::LitStr>());
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn allows_unknown(item: &syn::ItemStruct) -> bool {
    let mut allow = false;
    for attr in &item.attrs {
        if attr.path().is_ident("configulator") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("allow_unknown_fields") {
                    allow = true;
                } else if meta.input.peek(syn::Token![=]) {
                    meta.value()?.parse::<syn::Expr>()?;
                }
                Ok(())
            });
        }
    }
    allow
}

// Leaking is fine: the process exits right after printing.

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn last_ident(ty: &syn::Type) -> Option<&syn::PathSegment> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last(),
        _ => None,
    }
}

fn type_arg(seg: &syn::PathSegment, n: usize) -> Option<&syn::Type> {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        args.args
            .iter()
            .filter_map(|a| match a {
                syn::GenericArgument::Type(t) => Some(t),
                _ => None,
            })
            .nth(n)
    } else {
        None
    }
}

fn scalar_hint(ty: &syn::Type) -> ScalarHint {
    if let Some(seg) = last_ident(ty) {
        match seg.ident.to_string().as_str() {
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => return ScalarHint::Integer,
            "f32" | "f64" => return ScalarHint::Float,
            "bool" => return ScalarHint::Bool,
            "Duration" => return ScalarHint::Duration,
            _ => {}
        }
    }
    ScalarHint::String
}

fn build_fields(
    type_name: &str,
    structs: &HashMap<String, syn::ItemStruct>,
    stack: &mut Vec<String>,
) -> Result<Vec<FieldInfo>, String> {
    let item = structs
        .get(type_name)
        .ok_or_else(|| format!("type {type_name} not found in the scanned tree"))?;
    if stack.contains(&type_name.to_string()) {
        return Err(format!("recursive config type {type_name}"));
    }
    stack.push(type_name.to_string());

    let syn::Fields::Named(named) = &item.fields else {
        return Err(format!("{type_name} does not have named fields"));
    };

    let mut out = Vec::new();
    for field in &named.named {
        let ident = field.ident.as_ref().unwrap().unraw().to_string();
        let attrs = parse_attrs(&field.attrs).map_err(|e| format!("{type_name}.{ident}: {e}"))?;
        let config_name = attrs.name.clone().unwrap_or_else(|| ident.clone());

        let mut ty = &field.ty;
        let mut optional = false;
        if let Some(seg) = last_ident(ty) {
            if seg.ident == "Option" {
                if let Some(inner) = type_arg(seg, 0) {
                    ty = inner;
                    optional = true;
                }
            }
        }

        let mut allow_unknown_fields = false;
        let mut nested_fields =
            |t: &syn::Type, stack: &mut Vec<String>| -> Result<Vec<FieldInfo>, String> {
                let name = last_ident(t)
                    .map(|s| s.ident.to_string())
                    .ok_or_else(|| format!("{type_name}.{ident}: cannot resolve nested type"))?;
                if let Some(item) = structs.get(&name) {
                    allow_unknown_fields = allows_unknown(item);
                }
                build_fields(&name, structs, stack)
            };

        let (field_type, scalar) = if let Some(seg) = last_ident(ty) {
            let id = seg.ident.to_string();
            match id.as_str() {
                "bool" => (FieldType::Bool, ScalarHint::Bool),
                "Vec" => {
                    let elem = type_arg(seg, 0)
                        .ok_or_else(|| format!("{type_name}.{ident}: Vec without type argument"))?;
                    if attrs.nested {
                        (
                            FieldType::StructList(nested_fields(elem, stack)?),
                            ScalarHint::String,
                        )
                    } else {
                        (FieldType::List, scalar_hint(elem))
                    }
                }
                "HashMap" | "BTreeMap" => {
                    let val = type_arg(seg, 1)
                        .ok_or_else(|| format!("{type_name}.{ident}: map without value type"))?;
                    if attrs.nested {
                        (
                            FieldType::StructMap(nested_fields(val, stack)?),
                            ScalarHint::String,
                        )
                    } else {
                        (FieldType::Map, scalar_hint(val))
                    }
                }
                _ if attrs.nested => (
                    FieldType::Struct(nested_fields(ty, stack)?),
                    ScalarHint::String,
                ),
                _ => (FieldType::Scalar, scalar_hint(ty)),
            }
        } else if attrs.nested {
            (
                FieldType::Struct(nested_fields(ty, stack)?),
                ScalarHint::String,
            )
        } else {
            (FieldType::Scalar, scalar_hint(ty))
        };

        let env_segment = match attrs.env.as_deref() {
            Some(e) if e != "-" => e.to_string(),
            _ => config_name.to_uppercase().replace('-', "_"),
        };
        let flag_segment = match attrs.flag.as_deref() {
            Some(f) if f != "-" => f.to_string(),
            _ => config_name.clone(),
        };
        out.push(FieldInfo {
            field_name: leak(ident),
            config_name: leak(config_name.clone()),
            env_segment: leak(env_segment),
            flag_segment: leak(flag_segment),
            skip_env: attrs.env.as_deref() == Some("-"),
            skip_cli: attrs.flag.as_deref() == Some("-"),
            short: attrs.short,
            secret: attrs.secret,
            required: attrs.required,
            default_value: attrs.default.map(leak),
            description: attrs.description.map(leak),
            scalar,
            optional,
            allow_unknown_fields,
            field_type,
        });
    }
    stack.pop();
    Ok(out)
}

/// Order-preserving sample value tree for the JSON and TOML samples. The
/// YAML sample is rendered by the runtime crate so it can carry comments.
#[derive(Debug, PartialEq)]
enum Sv {
    Str(String),
    /// An integer, already spelled in decimal, so `u64` values above
    /// `i64::MAX` survive.
    Int(String),
    Float(f64),
    Bool(bool),
    Seq(Vec<Sv>),
    Map(Vec<(String, Sv)>),
}

fn leaf_value(scalar: ScalarHint, default: Option<&str>) -> Sv {
    match scalar {
        ScalarHint::Bool => Sv::Bool(default.and_then(|d| d.parse().ok()).unwrap_or(false)),
        ScalarHint::Integer => Sv::Int(
            default
                .and_then(|d| {
                    d.parse::<i128>()
                        .map(|v| v.to_string())
                        .or_else(|_| d.parse::<u128>().map(|v| v.to_string()))
                        .ok()
                })
                .unwrap_or_else(|| "0".to_string()),
        ),
        ScalarHint::Float => Sv::Float(default.and_then(|d| d.parse().ok()).unwrap_or(0.0)),
        ScalarHint::Duration => Sv::Str(default.unwrap_or("0s").to_string()),
        _ => Sv::Str(default.unwrap_or("").to_string()),
    }
}

fn sample_tree(fields: &[FieldInfo]) -> Vec<(String, Sv)> {
    let mut pairs = Vec::new();
    for f in fields.iter().filter(|f| !f.secret) {
        let value = match &f.field_type {
            FieldType::Struct(sub) => Sv::Map(sample_tree(sub)),
            FieldType::StructList(_) => Sv::Seq(Vec::new()),
            FieldType::StructMap(_) | FieldType::Map => Sv::Map(Vec::new()),
            FieldType::List => Sv::Seq(match f.default_value {
                Some(d) if !d.is_empty() => d
                    .split(',')
                    .map(|part| leaf_value(f.scalar, Some(part)))
                    .collect(),
                _ => Vec::new(),
            }),
            FieldType::Bool => leaf_value(ScalarHint::Bool, f.default_value),
            _ => leaf_value(f.scalar, f.default_value),
        };
        pairs.push((f.config_name.to_string(), value));
    }
    pairs
}

fn encode_sample(fields: &[FieldInfo], format: &str) -> Result<String, String> {
    match format {
        "yaml" => Ok(configulator::__schema::sample_config(fields)),
        "json" => {
            let mut out = String::new();
            write_json(&mut out, &Sv::Map(sample_tree(fields)), 0)?;
            out.push('\n');
            Ok(out)
        }
        "toml" => {
            let mut out = String::new();
            write_toml_table(&mut out, &sample_tree(fields), &[]);
            Ok(out)
        }
        other => Err(format!(
            "unknown --format {other:?}: expected yaml, json, or toml"
        )),
    }
}

/// Write `v` as JSON indented by two spaces, like Go's `jsontext` encoder.
fn write_json(out: &mut String, v: &Sv, depth: usize) -> Result<(), String> {
    match v {
        Sv::Str(s) => out.push_str(&json_quote(s)),
        Sv::Int(s) => out.push_str(s),
        Sv::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Sv::Float(f) if !f.is_finite() => {
            return Err(format!(
                "sample value {} has no JSON spelling",
                go_float_name(*f)
            ))
        }
        Sv::Float(f) => out.push_str(&go_float(*f)),
        Sv::Seq(items) if items.is_empty() => out.push_str("[]"),
        Sv::Map(pairs) if pairs.is_empty() => out.push_str("{}"),
        Sv::Seq(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&"  ".repeat(depth + 1));
                write_json(out, item, depth + 1)?;
            }
            out.push('\n');
            out.push_str(&"  ".repeat(depth));
            out.push(']');
        }
        Sv::Map(pairs) => {
            out.push('{');
            for (i, (k, val)) in pairs.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&"  ".repeat(depth + 1));
                out.push_str(&json_quote(k));
                out.push_str(": ");
                write_json(out, val, depth + 1)?;
            }
            out.push('\n');
            out.push_str(&"  ".repeat(depth));
            out.push('}');
        }
    }
    Ok(())
}

/// Write scalar keys first, then `[table]` sections, the way the Go
/// generator does. TOML requires that order.
fn write_toml_table(out: &mut String, pairs: &[(String, Sv)], path: &[String]) {
    let mut tables = Vec::new();
    for (k, v) in pairs {
        match v {
            Sv::Map(sub) => tables.push((k, sub)),
            Sv::Seq(items) => {
                let items: Vec<String> = items.iter().map(toml_scalar).collect();
                out.push_str(&format!("{} = [{}]\n", toml_key(k), items.join(", ")));
            }
            v => out.push_str(&format!("{} = {}\n", toml_key(k), toml_scalar(v))),
        }
    }
    for (k, sub) in tables {
        let mut full = path.to_vec();
        full.push(toml_key(k));
        out.push_str(&format!("\n[{}]\n", full.join(".")));
        write_toml_table(out, sub, &full);
    }
}

/// A bare TOML key when `k` allows it, else a quoted one. The Go generator
/// always writes keys bare.
fn toml_key(k: &str) -> String {
    let bare = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        k.to_string()
    } else {
        go_quote(k)
    }
}

fn toml_scalar(v: &Sv) -> String {
    match v {
        Sv::Str(s) => go_quote(s),
        Sv::Int(s) => s.clone(),
        Sv::Bool(b) => b.to_string(),
        Sv::Float(f) if f.is_nan() => "nan".to_string(),
        Sv::Float(f) if f.is_infinite() => if *f > 0.0 { "inf" } else { "-inf" }.to_string(),
        Sv::Float(f) => go_float(*f),
        Sv::Seq(_) | Sv::Map(_) => "\"\"".to_string(),
    }
}

/// Spell `v` like the Go generator's sample floats: always with a `.` or an
/// exponent so TOML reads a float, e.g. `1000.0`, `1e21`, `1e-07`.
fn go_float(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && !(1e-5..1e16).contains(&a) {
        let e = format!("{v:e}");
        let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
        let (sign, digits) = match exp.strip_prefix('-') {
            Some(d) => ("-", d),
            None => ("", exp),
        };
        return format!("{mantissa}e{sign}{digits:0>2}");
    }
    let s = v.to_string();
    if s.contains('.') {
        s
    } else {
        s + ".0"
    }
}

/// How Go's `%v` prints a NaN or infinite float.
fn go_float_name(v: f64) -> &'static str {
    if v.is_nan() {
        "NaN"
    } else if v > 0.0 {
        "+Inf"
    } else {
        "-Inf"
    }
}

/// Quote `s` as a JSON string the way Go's `jsontext` does: short escapes
/// for `\b \f \n \r \t`, `\u00XX` for other control characters, and
/// everything else as is.
fn json_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Quote `s` like Go's `strconv.Quote`, as the Go generator does for TOML
/// strings. Mirrors the runtime crate's YAML sample quoting.
fn go_quote(s: &str) -> String {
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

/// Whether Go's `strconv.IsPrint` holds for `c`. Rust's `escape_debug`
/// escapes the same classes, plus grapheme extenders at the start of a
/// string only, so `c` goes second.
fn is_go_print(c: char) -> bool {
    match c {
        ' '..='~' => true,
        '\0'..='\u{7f}' => false,
        _ => {
            let s = format!("a{c}");
            s.escape_debug().eq(s.chars())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let mut f = std::fs::File::create(dir.path().join("src/main.rs")).unwrap();
        write!(
            f,
            r##"
use configulator::{{Config, Validate}};

#[derive(Config, Debug)]
struct SchemaCfg {{
    #[configulator(name = "port", default = "8080", required, short = 'p', description = "listen port")]
    port: u16,

    #[configulator(name = "key", secret)]
    key: String,

    #[configulator(name = "sub", nested)]
    sub: SchemaSub,

    #[configulator(name = "tags", default = "a,b")]
    tags: Vec<String>,
}}

#[derive(Config, Debug)]
struct SchemaSub {{
    #[configulator(name = "host", default = "localhost", description = "bind host")]
    host: String,
}}
"##
        )
        .unwrap();
        dir
    }

    #[test]
    fn schema_and_sample_match_the_generator_contract() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let fields = build_fields("SchemaCfg", &structs, &mut Vec::new()).unwrap();

        let schema = configulator::__schema::json_schema(&fields, false);
        for want in [
            "\"required\"",
            "\"port\"",
            "\"listen port\"",
            "\"additionalProperties\": false",
            "\"default\": 8080",
        ] {
            assert!(schema.contains(want), "schema missing {want}:\n{schema}");
        }

        let sample = configulator::__schema::sample_config(&fields);
        for want in [
            "port: 8080",
            "# key: \"(secret)\"",
            "# bind host",
            "host: \"localhost\"",
            "tags: [\"a\", \"b\"]",
        ] {
            assert!(sample.contains(want), "sample missing {want}:\n{sample}");
        }
    }

    #[test]
    fn markdown_table() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let fields = build_fields("SchemaCfg", &structs, &mut Vec::new()).unwrap();
        let md = configulator::__schema::markdown(&fields, ".", "APP_", "_");
        let squeezed: String = md
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        let md = squeezed;
        for want in [
            "| Key | Type | Default | Environment | Flag | Description |",
            "| `port` | integer | `8080` | `APP_PORT` | `-p`, `--port` | listen port (required) |",
            "| `key` | string | | `APP_KEY` | `--key` | secret |",
            "| `sub.host` | string | `localhost` | `APP_SUB_HOST` | `--sub.host` | bind host |",
            "| `tags` | list of string | `a,b` | `APP_TAGS` | `--tags` | |",
        ] {
            assert!(md.contains(want), "markdown missing {want:?}:\n{md}");
        }
    }

    #[test]
    fn sample_formats() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let fields = build_fields("SchemaCfg", &structs, &mut Vec::new()).unwrap();

        let json = encode_sample(&fields, "json").unwrap();
        for want in ["\"port\": 8080", "\"host\": \"localhost\"", "\"a\","] {
            assert!(json.contains(want), "json sample missing {want}:\n{json}");
        }
        assert!(!json.contains("\"key\""), "{json}");

        let toml = encode_sample(&fields, "toml").unwrap();
        for want in ["port = 8080", "[sub]", "host = \"localhost\""] {
            assert!(toml.contains(want), "toml sample missing {want}:\n{toml}");
        }
        assert!(!toml.contains("key ="), "{toml}");
        assert!(
            toml.find("[sub]").unwrap() > toml.find("port =").unwrap(),
            "toml scalars must precede tables:\n{toml}"
        );

        assert!(encode_sample(&fields, "ini").is_err());

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["sub"]["host"], "localhost");
        let parsed: toml::Table = toml::from_str(&toml).unwrap();
        assert_eq!(parsed["port"].as_integer(), Some(8080));
        assert_eq!(parsed["tags"].as_array().unwrap().len(), 2);
    }

    fn go_fixture() -> Vec<FieldInfo> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            r#"
#[derive(Config)]
struct Cfg {
    #[configulator(name = "name", default = "say \"hi\"", description = "a | b")]
    name: String,
    #[configulator(name = "big", default = "18446744073709551615")]
    big: u64,
    #[configulator(name = "whole", default = "1000")]
    whole: f64,
    #[configulator(name = "huge", default = "1e21")]
    huge: f64,
    #[configulator(name = "tiny", default = "0.0000001")]
    tiny: f64,
    #[configulator(name = "wait")]
    wait: configulator::Duration,
    #[configulator(name = "tags", default = "a,b")]
    tags: Vec<String>,
    #[configulator(name = "labels")]
    labels: HashMap<String, String>,
    #[configulator(name = "token", secret)]
    token: String,
    #[configulator(name = "http", nested)]
    http: Http,
}

#[derive(Config)]
struct Http {
    #[configulator(name = "host", default = "localhost")]
    host: String,
    #[configulator(name = "tls", nested)]
    tls: Tls,
}

#[derive(Config)]
struct Tls {
    #[configulator(name = "on", default = "true")]
    on: bool,
}
"#,
        )
        .unwrap();
        let structs = collect_structs(dir.path()).unwrap();
        build_fields("Cfg", &structs, &mut Vec::new()).unwrap()
    }

    /// The expected outputs are the Go generator's for the same config.
    #[test]
    fn samples_match_go() {
        let fields = go_fixture();
        assert_eq!(
            encode_sample(&fields, "json").unwrap(),
            r#"{
  "name": "say \"hi\"",
  "big": 18446744073709551615,
  "whole": 1000.0,
  "huge": 1e21,
  "tiny": 1e-07,
  "wait": "0s",
  "tags": [
    "a",
    "b"
  ],
  "labels": {},
  "http": {
    "host": "localhost",
    "tls": {
      "on": true
    }
  }
}
"#
        );
        assert_eq!(
            encode_sample(&fields, "toml").unwrap(),
            r#"name = "say \"hi\""
big = 18446744073709551615
whole = 1000.0
huge = 1e21
tiny = 1e-07
wait = "0s"
tags = ["a", "b"]

[labels]

[http]
host = "localhost"

[http.tls]
on = true
"#
        );
        assert_eq!(
            configulator::__schema::markdown(&fields, ".", "", "_"),
            "# Configuration

| Key           | Type           | Default                | Environment   | Flag            | Description |
|---------------|----------------|------------------------|---------------|-----------------|-------------|
| `name`        | string         | `say \"hi\"`             | `NAME`        | `--name`        | a \\| b      |
| `big`         | integer        | `18446744073709551615` | `BIG`         | `--big`         |             |
| `whole`       | number         | `1000`                 | `WHOLE`       | `--whole`       |             |
| `huge`        | number         | `1e21`                 | `HUGE`        | `--huge`        |             |
| `tiny`        | number         | `0.0000001`            | `TINY`        | `--tiny`        |             |
| `wait`        | string         |                        | `WAIT`        | `--wait`        |             |
| `tags`        | list of string | `a,b`                  | `TAGS`        | `--tags`        |             |
| `labels`      | map of string  |                        | \u{2014}             | \u{2014}               |             |
| `token`       | string         |                        | `TOKEN`       | `--token`       | secret      |
| `http.host`   | string         | `localhost`            | `HTTP_HOST`   | `--http.host`   |             |
| `http.tls.on` | boolean        | `true`                 | `HTTP_TLS_ON` | `--http.tls.on` |             |
"
        );
    }

    #[test]
    fn go_float_spelling() {
        for (v, want) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1000.0, "1000.0"),
            (2.5, "2.5"),
            (1e21, "1e21"),
            (1e16, "1e16"),
            (1e15, "1000000000000000.0"),
            (1e-5, "0.00001"),
            (1e-7, "1e-07"),
            (-1.23e-6, "-1.23e-06"),
            (1.5e300, "1.5e300"),
        ] {
            assert_eq!(go_float(v), want, "{v:e}");
        }
        assert_eq!(toml_scalar(&Sv::Float(f64::NAN)), "nan", "TOML spells NaN");
        let err = encode_sample(
            &[FieldInfo {
                default_value: Some("NaN"),
                ..go_fixture()[2].clone()
            }],
            "json",
        )
        .unwrap_err();
        assert_eq!(err, "sample value NaN has no JSON spelling");
    }

    #[test]
    fn toml_keys_are_quoted_only_when_needed() {
        assert_eq!(toml_key("min-version_2"), "min-version_2");
        assert_eq!(toml_key("a.b c"), "\"a.b c\"");
    }

    #[test]
    fn unknown_type_errors() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let err = build_fields("Nope", &structs, &mut Vec::new()).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn splice_keeps_markers_and_replaces_between() {
        let doc = "# App\n\n<!-- configulator:begin -->\nold\n<!-- configulator:end -->\n\nmore\n";
        let out = splice_markdown(doc, "| a |\n").unwrap();
        assert_eq!(
            out,
            "# App\n\n<!-- configulator:begin -->\n\n| a |\n\n<!-- configulator:end -->\n\nmore\n"
        );
        assert_eq!(splice_markdown(&out, "| a |\n").unwrap(), out);
    }

    #[test]
    fn splice_rejects_bad_markers() {
        assert!(splice_markdown("no markers", "x").is_err());
        assert!(
            splice_markdown("<!-- configulator:end --> <!-- configulator:begin -->", "x").is_err()
        );
        let two = "<!-- configulator:begin --><!-- configulator:end --><!-- configulator:begin --><!-- configulator:end -->";
        assert!(splice_markdown(two, "x")
            .unwrap_err()
            .contains("more than one"));
    }

    #[test]
    fn markdown_file_writes_then_check_passes() {
        let dir = fixture();
        let readme = dir.path().join("README.md");
        std::fs::write(
            &readme,
            "## Configuration\n\n<!-- configulator:begin -->\n<!-- configulator:end -->\n",
        )
        .unwrap();
        let args = |extra: &[&str]| {
            let mut a = vec![
                "configulator",
                "--type",
                "SchemaCfg",
                "--markdown",
                "--env-prefix",
                "APP__",
                "--dir",
                dir.path().to_str().unwrap(),
                "--markdown-file",
                readme.to_str().unwrap(),
            ];
            a.extend_from_slice(extra);
            Args::parse_from(a)
        };

        let err = run(&args(&["--check"])).unwrap_err();
        assert!(err.contains("is out of date"), "{err}");
        let want = format!(
            "; run configulator --type SchemaCfg --dir {} --env-prefix APP__ --markdown --markdown-file {}",
            dir.path().display(),
            readme.display()
        );
        assert!(err.ends_with(&want), "{err}");

        run(&args(&[])).unwrap();
        let written = std::fs::read_to_string(&readme).unwrap();
        assert_eq!(written.matches("Configuration").count(), 1, "{written}");
        assert!(written.contains("`APP__SUB_HOST`"), "{written}");
        assert!(
            written.ends_with("<!-- configulator:end -->\n"),
            "{written}"
        );

        run(&args(&["--check"])).unwrap();
        assert_eq!(std::fs::read_to_string(&readme).unwrap(), written);
    }

    fn usage(args: &[&str]) -> Result<(), String> {
        let mut a = vec!["configulator", "-t", "X"];
        a.extend_from_slice(args);
        check_usage(&Args::try_parse_from(a).unwrap())
    }

    #[test]
    fn markdown_file_flag_dependencies() {
        assert_eq!(
            usage(&["--schema", "--markdown-file", "R.md"]).unwrap_err(),
            "--markdown-file needs --markdown"
        );
        assert_eq!(
            usage(&["--markdown", "--check"]).unwrap_err(),
            "--check needs --markdown-file or --sample-file"
        );
        assert!(usage(&["--markdown", "--markdown-file", "R.md", "--check"]).is_ok());
    }

    #[test]
    fn mode_and_format_usage() {
        assert_eq!(
            usage(&[]).unwrap_err(),
            "pass one of --schema, --sample, or --markdown"
        );
        assert!(usage(&["--schema", "--sample"])
            .unwrap_err()
            .starts_with("pass at most one of --schema, --sample, --markdown"));
        assert_eq!(
            usage(&["--schema", "--format", "json"]).unwrap_err(),
            "--format only applies to --sample"
        );
        assert!(usage(&["--sample", "--format", "json"]).is_ok());
    }

    #[test]
    fn rerun_command_repeats_every_option() {
        let args = Args::try_parse_from([
            "configulator",
            "-t",
            "Cfg",
            "--markdown",
            "--markdown-file",
            "docs/my README.md",
            "--env-prefix",
            "APP_",
            "--env-separator",
            "__",
            "--flag-separator",
            "-",
            "--dir",
            "crates/app",
        ])
        .unwrap();
        assert_eq!(
            rerun_command(&args, args.markdown_file.as_ref().unwrap()),
            "configulator --type Cfg --dir crates/app --env-prefix APP_ --env-separator __ --flag-separator - --markdown --markdown-file 'docs/my README.md'"
        );
        let args = Args::try_parse_from([
            "configulator",
            "-t",
            "Cfg",
            "--sample",
            "--format",
            "toml",
            "--sample-file",
            "it's.toml",
        ])
        .unwrap();
        assert_eq!(
            rerun_command(&args, args.sample_file.as_ref().unwrap()),
            r"configulator --type Cfg --format toml --sample --sample-file 'it'\''s.toml'"
        );
    }

    #[test]
    fn sample_file_creates_then_check_passes() {
        let dir = fixture();
        let sample = dir.path().join("config.example.yaml");
        let args = |extra: &[&str]| {
            let mut a = vec![
                "configulator",
                "--type",
                "SchemaCfg",
                "--sample",
                "--dir",
                dir.path().to_str().unwrap(),
                "--sample-file",
                sample.to_str().unwrap(),
            ];
            a.extend_from_slice(extra);
            Args::parse_from(a)
        };

        let err = run(&args(&["--check"])).unwrap_err();
        assert!(err.contains("is out of date"), "{err}");
        assert!(!sample.exists());

        run(&args(&[])).unwrap();
        let written = std::fs::read_to_string(&sample).unwrap();
        assert!(written.contains("port: 8080"), "{written}");

        run(&args(&["--check"])).unwrap();
        std::fs::write(&sample, "stale: true\n").unwrap();
        assert!(run(&args(&["--check"])).is_err());
        run(&args(&[])).unwrap();
        assert_eq!(std::fs::read_to_string(&sample).unwrap(), written);
    }

    #[test]
    fn sample_file_flag_dependencies() {
        assert_eq!(
            usage(&["--markdown", "--sample-file", "c.yaml"]).unwrap_err(),
            "--sample-file needs --sample"
        );
        assert!(usage(&["--sample", "--check"]).is_err());
        assert!(usage(&["--sample", "--sample-file", "c.yaml", "--check"]).is_ok());
    }

    #[test]
    fn markdown_reads_short_from_source() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let fields = build_fields("SchemaCfg", &structs, &mut Vec::new()).unwrap();
        let md = configulator::__schema::markdown(&fields, ".", "", "__");
        assert!(md.contains("`-p`, `--port`"), "{md}");
    }
}
