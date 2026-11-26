//! Embed the registered catalogs without a runtime filesystem dependency.
use std::{collections::HashSet, env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("languages.json")).unwrap()).unwrap();
    let entries = manifest["languages"].as_array().unwrap();
    let mut ids = HashSet::new();
    let mut output = format!("static CATALOGS: [Catalog; {}] = [\n", entries.len());
    for entry in entries {
        let id = entry["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate language {id}");
        let name = entry["name"].as_str().unwrap();
        let aliases: Vec<_> = entry["aliases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let catalog = entry["catalog"].as_str().unwrap();
        assert!(root.join(catalog).is_file(), "missing catalog {catalog}");
        output.push_str(&format!(
            "Catalog {{ id: {id:?}, name: {name:?}, aliases: &{aliases:?}, source: include_str!({:?}), messages: OnceLock::new() }},\n",
            root.join(catalog)
        ));
    }
    output.push_str("];\n");
    let default = entries
        .iter()
        .position(|entry| entry["id"] == manifest["default"])
        .expect("registered default language");
    output.push_str(&format!(
        "const DEFAULT_LANGUAGE: Language = Language({default});\n"
    ));
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("catalogs.rs"),
        output,
    )
    .unwrap();
}
