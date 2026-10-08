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
    group(clap::ArgGroup::new("output_file").args(["markdown_file", "sample_file"])),
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
    #[arg(long, default_value = "__")]
    env_separator: String,

    /// Flag separator used in the Markdown table
    #[arg(long, default_value = ".")]
    flag_separator: String,

    /// With --markdown, write the table between the
    /// <!-- configulator:begin --> and <!-- configulator:end --> markers in
    /// this file instead of stdout
    #[arg(long, requires = "markdown")]
    markdown_file: Option<PathBuf>,

    /// With --sample, write the sample to this file instead of stdout
    #[arg(long, requires = "sample")]
    sample_file: Option<PathBuf>,

    /// With --markdown-file or --sample-file, change nothing and exit 1 if
    /// the file is out of date
    #[arg(long, requires = "output_file")]
    check: bool,

    /// Crate directory to scan for .rs files (target/ is skipped)
    #[arg(long, default_value = ".")]
    dir: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let selected = [args.schema, args.sample, args.markdown]
        .iter()
        .filter(|b| **b)
        .count();
    if selected != 1 {
        eprintln!("pass exactly one of --schema, --sample, or --markdown");
        return ExitCode::FAILURE;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    let structs = collect_structs(&args.dir)?;
    if structs.is_empty() {
        return Err(format!("no structs found under {}", args.dir.display()));
    }
    let fields = build_fields(&args.r#type, &structs, &mut Vec::new())?;

    if !args.sample && args.format != "yaml" {
        return Err("--format only applies to --sample".to_string());
    }
    let body = if args.schema {
        configulator::__schema::json_schema(
            &args.r#type,
            &fields,
            allows_unknown(&structs[&args.r#type]),
        )
    } else if args.sample {
        let sample = encode_sample(&fields, &args.r#type, &args.format)?;
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
                    "{0} is out of date; run configulator --sample --sample-file {0}",
                    path.display()
                ));
            }
            std::fs::write(path, &sample).map_err(|e| format!("{}: {e}", path.display()))?;
            eprintln!("configulator: updated {}", path.display());
            return Ok(());
        }
        sample
    } else {
        let table = configulator::__schema::markdown(
            &args.r#type,
            &fields,
            &args.flag_separator,
            &args.env_prefix,
            &args.env_separator,
        );
        if let Some(path) = &args.markdown_file {
            let table = table.split_once("\n\n").map_or(table.as_str(), |(_, t)| t);
            if update_markdown_file(path, table, args.check)? {
                eprintln!("configulator: updated {}", path.display());
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

/// Write `table` into `path` between the markers. With `check` set, write
/// nothing and fail if the file would change. Returns whether it changed.
fn update_markdown_file(path: &Path, table: &str, check: bool) -> Result<bool, String> {
    let doc = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let out = splice_markdown(&doc, table).map_err(|e| format!("{}: {e}", path.display()))?;
    if out == doc {
        return Ok(false);
    }
    if check {
        return Err(format!(
            "{0} is out of date; run configulator --markdown --markdown-file {0}",
            path.display()
        ));
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
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
                out.default = Some(get(&meta)?);
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
        let attrs = parse_attrs(&field.attrs)?;
        let ident = field.ident.as_ref().unwrap().unraw().to_string();
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

// YAML is written by hand so descriptions can go in comments.

/// Order-preserving sample value tree, serializable by any serde encoder.
enum Sv {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Seq(Vec<Sv>),
    Map(Vec<(String, Sv)>),
}

impl serde::Serialize for Sv {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Sv::Str(s) => ser.serialize_str(s),
            Sv::Int(v) => ser.serialize_i64(*v),
            Sv::Float(v) => ser.serialize_f64(*v),
            Sv::Bool(v) => ser.serialize_bool(*v),
            Sv::Seq(items) => {
                let mut seq = ser.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Sv::Map(pairs) => {
                let mut map = ser.serialize_map(Some(pairs.len()))?;
                for (k, v) in pairs {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

fn leaf_value(f: &FieldInfo) -> Sv {
    match (f.scalar, f.default_value) {
        (ScalarHint::Bool, d) => Sv::Bool(d.and_then(|d| d.parse().ok()).unwrap_or(false)),
        (ScalarHint::Integer, d) => Sv::Int(d.and_then(|d| d.parse().ok()).unwrap_or(0)),
        (ScalarHint::Float, d) => Sv::Float(d.and_then(|d| d.parse().ok()).unwrap_or(0.0)),
        (ScalarHint::String, d) => Sv::Str(d.unwrap_or("").to_string()),
        (_, d) => Sv::Str(d.unwrap_or("").to_string()),
    }
}

fn sample_tree(fields: &[FieldInfo]) -> Sv {
    let mut pairs = Vec::new();
    for f in fields.iter().filter(|f| !f.secret) {
        let value = match &f.field_type {
            FieldType::Struct(sub) => sample_tree(sub),
            FieldType::StructList(_) => Sv::Seq(Vec::new()),
            FieldType::StructMap(_) | FieldType::Map => Sv::Map(Vec::new()),
            FieldType::List => match f.default_value {
                Some(d) if !f.secret => Sv::Seq(
                    d.split(',')
                        .map(|part| {
                            let elem = FieldInfo {
                                default_value: Some(Box::leak(part.to_string().into_boxed_str())),
                                secret: false,
                                ..f.clone()
                            };
                            leaf_value(&elem)
                        })
                        .collect(),
                ),
                _ => Sv::Seq(Vec::new()),
            },
            FieldType::Bool | FieldType::Scalar => leaf_value(f),
            _ => leaf_value(f),
        };
        pairs.push((f.config_name.to_string(), value));
    }
    Sv::Map(pairs)
}

fn encode_sample(fields: &[FieldInfo], type_name: &str, format: &str) -> Result<String, String> {
    match format {
        "yaml" => Ok(configulator::__schema::sample_config(type_name, fields)),
        "json" => {
            let mut out =
                serde_json::to_string_pretty(&sample_tree(fields)).map_err(|e| e.to_string())?;
            out.push('\n');
            Ok(out)
        }
        "toml" => toml::to_string_pretty(&sample_tree(fields)).map_err(|e| e.to_string()),
        other => Err(format!(
            "unknown --format {other:?}: expected yaml, json, or toml"
        )),
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

        let schema = configulator::__schema::json_schema("SchemaCfg", &fields, false);
        for want in [
            "\"required\"",
            "\"port\"",
            "\"listen port\"",
            "\"additionalProperties\": false",
            "\"default\": 8080",
        ] {
            assert!(schema.contains(want), "schema missing {want}:\n{schema}");
        }

        let sample = configulator::__schema::sample_config("SchemaCfg", &fields);
        for want in [
            "port: 8080",
            "# key: \"(secret)\"",
            "# bind host",
            "host: \"localhost\"",
            "tags: [a,b]",
        ] {
            assert!(sample.contains(want), "sample missing {want}:\n{sample}");
        }
    }

    #[test]
    fn markdown_table() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let fields = build_fields("SchemaCfg", &structs, &mut Vec::new()).unwrap();
        let md = configulator::__schema::markdown("SchemaCfg", &fields, ".", "APP_", "_");
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

        let json = encode_sample(&fields, "SchemaCfg", "json").unwrap();
        for want in ["\"port\": 8080", "\"host\": \"localhost\"", "\"a\","] {
            assert!(json.contains(want), "json sample missing {want}:\n{json}");
        }
        assert!(!json.contains("\"key\""), "{json}");

        let toml = encode_sample(&fields, "SchemaCfg", "toml").unwrap();
        for want in ["port = 8080", "[sub]", "host = \"localhost\""] {
            assert!(toml.contains(want), "toml sample missing {want}:\n{toml}");
        }
        assert!(!toml.contains("key ="), "{toml}");
        assert!(
            toml.find("[sub]").unwrap() > toml.find("port =").unwrap(),
            "toml scalars must precede tables:\n{toml}"
        );

        assert!(encode_sample(&fields, "SchemaCfg", "ini").is_err());
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

        run(&args(&[])).unwrap();
        let written = std::fs::read_to_string(&readme).unwrap();
        assert!(!written.contains("# SchemaCfg configuration"), "{written}");
        assert!(written.contains("`APP__SUB__HOST`"), "{written}");
        assert!(
            written.ends_with("<!-- configulator:end -->\n"),
            "{written}"
        );

        run(&args(&["--check"])).unwrap();
        assert_eq!(std::fs::read_to_string(&readme).unwrap(), written);
    }

    #[test]
    fn markdown_file_flag_dependencies() {
        assert!(Args::try_parse_from([
            "configulator",
            "-t",
            "X",
            "--schema",
            "--markdown-file",
            "R.md"
        ])
        .is_err());
        assert!(
            Args::try_parse_from(["configulator", "-t", "X", "--markdown", "--check"]).is_err()
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
        assert!(Args::try_parse_from([
            "configulator",
            "-t",
            "X",
            "--markdown",
            "--sample-file",
            "c.yaml"
        ])
        .is_err());
        assert!(Args::try_parse_from(["configulator", "-t", "X", "--sample", "--check"]).is_err());
        assert!(Args::try_parse_from([
            "configulator",
            "-t",
            "X",
            "--sample",
            "--sample-file",
            "c.yaml",
            "--check"
        ])
        .is_ok());
    }

    #[test]
    fn markdown_reads_short_from_source() {
        let dir = fixture();
        let structs = collect_structs(dir.path()).unwrap();
        let fields = build_fields("SchemaCfg", &structs, &mut Vec::new()).unwrap();
        let md = configulator::__schema::markdown("SchemaCfg", &fields, ".", "", "__");
        assert!(md.contains("`-p`, `--port`"), "{md}");
    }
}
