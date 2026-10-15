//! The icon names' table, made from material-symbols.codepoints at build
//! time: sorted by name, so that looking one up is a binary search, not a
//! walk down four thousand lines (src/icons.rs).

use std::fmt::Write;

fn main() {
    println!("cargo::rerun-if-changed=material-symbols.codepoints");
    let listed = std::fs::read_to_string("material-symbols.codepoints").expect("material-symbols.codepoints");
    let mut icons: Vec<(&str, u32)> = listed
        .lines()
        .filter_map(|line| {
            let (name, hex) = line.split_once(' ')?;
            Some((name, u32::from_str_radix(hex.trim(), 16).expect("a hex codepoint")))
        })
        .collect();
    icons.sort_by(|a, b| a.0.cmp(b.0));
    icons.dedup_by(|a, b| a.0 == b.0);
    let mut table = format!("static ICONS: [(&str, u32); {}] = [\n", icons.len());
    for (name, code) in icons {
        writeln!(table, "    ({name:?}, {code:#x}),").unwrap();
    }
    table.push_str("];\n");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("icons.rs");
    std::fs::write(out, table).expect("cannot write the icons' table");
}
