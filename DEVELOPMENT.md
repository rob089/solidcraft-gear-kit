# Development

Needs Rust with the `wasm32-unknown-unknown` target and [`wasm-tools`](https://github.com/bytecodealliance/wasm-tools). If Homebrew's Rust shadows rustup's, prefix commands with `PATH="$(dirname "$(rustup which rustc)"):$PATH"`.

```
./build.sh                       # dist/gear-kit/ and dist/gear-kit-<version>.zip
cargo test --lib                 # geometry, schema, dialog, edit flow
tests/integration.py [filter]    # every entry on the real host (needs solidcraft-cli)
tools/run.py ENTRY 'JSON' --png out.png
tools/icons/make_icons.py icons  # regenerate the icons
tools/release.sh                 # GitHub release of the version in Cargo.toml
```

`plugin.json` is generated from `src/schema.rs`; `build.sh` also writes the copy at the repository root, which plugin lists read (a test fails when it is out of date). Set `SOLIDCRAFT_CLI` if `solidcraft-cli` is not in `/Applications/SolidCraft.app/Contents/MacOS/`.

## Layout

| Path | Contents |
|---|---|
| `src/schema.rs` | Entries, fields, manifest |
| `src/entries.rs` | Per entry: plan (checks, numbers) and build |
| `src/dialog.rs` | Dialog hook: result row and tooltips |
| `src/edit.rs` | Record in the design, replace in place, Edit Gears |
| `src/cyl.rs`, `rack.rs`, `bevel.rs`, `worm.rs`, `pulley.rs` | One kind of part each |
| `src/involute.rs`, `pair.rs` | Tooth profiles, pair geometry |
| `src/cad.rs`, `curve.rs`, `geom.rs` | Engine commands, sketch curves |
| `tests/integration.py` | Volume, mesh and edit checks on the real host |
| `wit/plugin.wit` | SolidCraft's `craft:plugin@0.2.0` (Apache-2.0) |

## Notes

- The manifest asks for SolidCraft 0.50.0 (`min_host_version`): it draws fields in declared order without marking them all required.
- A failing engine command ends the whole plugin run, so nothing may try a command to see if it works.
- The dialog hook has its own budget (10⁷ fuel, 250 ms) and also runs before every run: keep plan checks cheap.
- The result row shows about 26 characters and no tooltip: it takes whole parts up to that, problems get a short form (`Bad::short`), and everything is also in a field's tooltip.
- Pulley grooves follow ISO 5294 (Table 2), DIN 7721-2 (Tables 1, 4) and ISO 13050 (Tables 14, 16). GT2 uses Gates' published belt tooth.
