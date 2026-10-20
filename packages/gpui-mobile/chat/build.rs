//! Lifts named items, byte for byte, out of desktop files that mix portable drawing helpers with
//! native-only code, the same way `apps/gpui-web/build.rs` does (the `extract` function below is
//! that file's, unchanged). `extracted-items.txt` is the list, and with the symlinks under `app/`
//! it is the inventory of what the phone's chat takes from the desktop crate.
use std::{collections::BTreeMap, env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let desktop = Path::new(&manifest).join("../../../apps/desktop/src");
    let list_path = Path::new(&manifest).join("extracted-items.txt");
    println!("cargo:rerun-if-changed={}", list_path.display());

    // `<output module> <desktop file> <item> <item> ...` per line.
    let mut outputs: BTreeMap<String, String> = BTreeMap::new();
    for line in fs::read_to_string(&list_path).unwrap().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let output = words.next().unwrap().to_string();
        let file = desktop.join(words.next().unwrap());
        println!("cargo:rerun-if-changed={}", file.display());
        let source =
            fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let lines: Vec<&str> = source.lines().collect();
        let out = outputs.entry(output).or_default();
        for item in words {
            out.push_str(&extract(&lines, item, &file));
            out.push('\n');
        }
    }
    let out_dir = env::var("OUT_DIR").unwrap();
    for (name, body) in outputs {
        fs::write(Path::new(&out_dir).join(format!("{name}.rs")), body).unwrap();
    }
}

/// Relies on rustfmt's shape: an item starts at column 0 and its block closes with a bare `}` at column 0. `Type::method` names one method of an `impl Type` block (one indent deeper) and comes out wrapped in its own `impl Type`.
fn extract(lines: &[&str], item: &str, file: &Path) -> String {
    if let Some((owner, method)) = item.split_once("::") {
        let indented: Vec<&str> = lines
            .iter()
            .map(|line| {
                line.strip_prefix("    ")
                    .unwrap_or(if line.is_empty() { "" } else { "\u{1}" })
            })
            .collect();
        let body = extract(&indented, method, file);
        return format!("impl {owner} {{\n{body}}}\n");
    }
    // `impl=Type` names the inherent impl block of a type, which shares its name with the type itself.
    let (heads, item): (&[&str], &str) = match item.strip_prefix("impl=") {
        Some(owner) => (&["impl"], owner),
        None => (&["fn", "const", "static", "struct", "enum", "type"], item),
    };
    let start = lines
        .iter()
        .position(|line| {
            let rest = line
                .strip_prefix("pub(crate) ")
                .or_else(|| line.strip_prefix("pub(super) "))
                .or_else(|| line.strip_prefix("pub "))
                .unwrap_or(line);
            heads.iter().any(|head| {
                rest.strip_prefix(head)
                    .and_then(|r| r.strip_prefix(' '))
                    .and_then(|r| r.strip_prefix(item))
                    .is_some_and(|r| !r.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
            })
        })
        .unwrap_or_else(|| panic!("item `{item}` not found in {}", file.display()));
    let mut first = start;
    while first > 0 {
        let above = lines[first - 1].trim_start();
        if above.starts_with("///") || above.starts_with("#[") {
            first -= 1;
        } else {
            break;
        }
    }
    let mut end = start;
    loop {
        let line = lines[end];
        let single_line = end == start && (line.ends_with(';') || line.ends_with('}'));
        let is_value = ["const ", "static ", "type "].iter().any(|head| {
            lines[start]
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub ")
                .starts_with(head)
        });
        let closes = end > start
            && (line.starts_with('}')
                || line == "];"
                || line == ");"
                || (is_value && line.ends_with(';') && !line.starts_with("        ")));
        if single_line || closes {
            break;
        }
        end += 1;
    }
    let mut text = lines[first..=end].join("\n");
    text.push('\n');
    text
}
