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
    about = "Emit a JSON Schema and/or commented sample config for a #[derive(Config)] struct"
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
        eprintln!("pass exactly one of --schema, --sample, or --markdown; output goes to stdout, pipe it where you want it");
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
        encode_sample(&fields, &args.r#type, &args.format)?
    } else {
        configulator::__schema::markdown(
            &args.r#type,
            &fields,
            &args.flag_separator,
            &args.env_prefix,
            &args.env_separator,
        )
    };
    print!("{body}");
    Ok(())
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
            short: None,
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
    if f.secret {
        return Sv::Str("(secret)".into());
    }
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
    for f in fields {
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
    #[configulator(name = "port", default = "8080", required, description = "listen port")]
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
            "key: \"(secret)\"",
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
            "| `port` | integer | `8080` | `APP_PORT` | `--port` | listen port (required) |",
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
        for want in [
            "\"port\": 8080",
            "\"key\": \"(secret)\"",
            "\"host\": \"localhost\"",
            "\"a\",",
        ] {
            assert!(json.contains(want), "json sample missing {want}:\n{json}");
        }

        let toml = encode_sample(&fields, "SchemaCfg", "toml").unwrap();
        for want in [
            "port = 8080",
            "key = \"(secret)\"",
            "[sub]",
            "host = \"localhost\"",
        ] {
            assert!(toml.contains(want), "toml sample missing {want}:\n{toml}");
        }
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
}
