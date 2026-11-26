//! Gear Kit: a SolidCraft plugin (`craft:plugin@0.2.0`, capabilities `commands@0.2`,
//! `dialog@0.1`, `storage@0.1` and `forms@0.1`) that builds involute gear pairs and timing belt
//! pulleys out of ordinary SolidCraft features.
//!
//! * `spur-gears`: spur, helical and herringbone gear 1 with an external gear, a ring gear or a
//!   rack, at the working centre distance and phased so the teeth interlock
//! * `bevel-gears`: straight bevel gear pairs with spherical involute teeth
//! * `worm-gears`: worm and worm wheel
//! * `timing-pulleys`: timing belt pulley pairs (GT2, HTD, T, MXL, XL), the belt setting the
//!   centre distance
//! * `edit-gears`: opens the dialog a picked gear was made with, to replace it in place
//!
//! Each side of a pair can be turned off. The dialog hook shows the pair's numbers and any input
//! problem while the user types. Each run goes in a timeline folder and is recorded in the design.
//!
//! Layers, from the host down:
//!
//! * `schema`: the entries, their fields and the manifest; `Args` reads parameters through them
//! * `entries`: per entry a plan (inputs, checks, numbers) and a build; `dialog`: the hook
//! * `edit`: the record of each run in the design, replacing a run in place, `edit-gears`
//! * `cyl`, `rack`, `bevel`, `worm`, `pulley`: one kind of gear or pulley each, its checks and its
//!   build
//! * `involute`, `pair`: tooth profiles and pair geometry, pure functions
//! * `cad`: the engine commands; `curve`, `geom`: sketch curves and 2D helpers
//!
//! Everything but the guest glue at the bottom is plain Rust, tested natively (`cargo test`).

pub mod bevel;
pub mod cad;
pub mod curve;
pub mod cyl;
pub mod dialog;
pub mod edit;
pub mod entries;
pub mod geom;
pub mod involute;
pub mod pair;
pub mod pulley;
pub mod rack;
pub mod schema;
pub mod worm;

pub use entries::{execute, Error};

#[cfg(target_arch = "wasm32")]
mod guest {
    // the bindings to the host's interface (wit/plugin.wit)
    wit_bindgen::generate!({ path: "wit", world: "command-dialog-plugin" });

    use crate::cad::Host;
    use crate::entries::Error;
    use craft::plugin::{
        commands, control, forms, log,
        types::{CommandFailure, FieldFailure, PluginError},
    };
    use serde_json::Value;

    /// A host error in words, for the plugin's own error messages.
    fn describe(e: &PluginError) -> String {
        match e {
            PluginError::PermissionDenied(s) => format!("permission denied: {s}"),
            PluginError::CommandFailed(f) => format!("{}: {}", f.command, f.reason),
            PluginError::LimitExceeded(s) => format!("limit exceeded: {s}"),
            PluginError::InvalidManifest(f) => format!("manifest {}: {}", f.field, f.reason),
            PluginError::InvalidInput(f) => format!("{}: {}", f.field, f.reason),
            PluginError::Trap(s) => s.clone(),
            PluginError::Cancelled => "cancelled".into(),
            PluginError::Unsupported(s) => format!("unsupported: {s}"),
        }
    }

    /// The SolidCraft engine, through the plugin's imports.
    struct Engine;

    impl Host for Engine {
        fn run(&mut self, command: &str, args: Value) -> Result<Value, String> {
            if control::cancelled() {
                return Err("cancelled".into());
            }
            match commands::run(command, &args.to_string()) {
                Ok(s) => Ok(serde_json::from_str(&s).unwrap_or(Value::Null)),
                Err(e) => Err(format!("{command} failed: {}", describe(&e))),
            }
        }
        fn log(&mut self, message: &str) {
            log::log(log::Level::Info, message);
        }
        fn progress(&mut self, fraction: f32, message: &str) {
            control::progress(fraction, message);
        }
        fn open_form(&mut self, entry: &str, params: &Value) -> Result<bool, String> {
            match forms::open(entry, &params.to_string()) {
                Ok(()) => Ok(true),
                // a program called: it gets no dialog
                Err(PluginError::Unsupported(_)) => Ok(false),
                Err(e) => Err(format!("opening the {entry} dialog failed: {}", describe(&e))),
            }
        }
    }

    /// The plugin's exports: the entries and the dialog hook.
    struct GearKit;

    impl exports::craft::plugin::entry::Guest for GearKit {
        fn execute(entry: String, params: String) -> Result<String, PluginError> {
            let params: Value = match serde_json::from_str(&params) {
                Ok(v) => v,
                Err(e) => {
                    return Err(PluginError::InvalidInput(FieldFailure { field: "params".into(), reason: e.to_string() }));
                }
            };
            match crate::execute(&mut Engine, &entry, params) {
                Ok(v) => Ok(v.to_string()),
                Err(Error::Input(b)) => {
                    Err(PluginError::InvalidInput(FieldFailure { reason: crate::entries::describe(&entry, &b), field: b.field }))
                }
                Err(Error::Failed(msg)) => {
                    Err(PluginError::CommandFailed(CommandFailure { command: format!("gear-kit {entry}"), reason: msg }))
                }
            }
        }
    }

    impl exports::craft::plugin::dialog_provider::Guest for GearKit {
        fn update_fields(entry: String, values: String) -> Result<String, PluginError> {
            let values: Value = serde_json::from_str(&values).unwrap_or(Value::Null);
            let reply = if entry == crate::schema::EDIT {
                crate::edit::hook(&mut Engine, values)
            } else {
                crate::dialog::update_fields(&entry, values)
            };
            Ok(reply.to_string())
        }
    }

    export!(GearKit);
}
