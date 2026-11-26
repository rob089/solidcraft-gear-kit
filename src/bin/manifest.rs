//! Print plugin.json, generated from the entry definitions in `schema.rs` (`--pretty`: indented).

fn main() {
    let m = gear_kit::schema::manifest(env!("CARGO_PKG_VERSION"));
    let text = if std::env::args().any(|a| a == "--pretty") { serde_json::to_string_pretty(&m) } else { serde_json::to_string(&m) };
    println!("{}", text.unwrap());
}
