//! Expected cross-section areas for the integration tests, from the plugin's own profile code:
//!
//!     cargo run -q --example expect -- '{"kind": "external", "z": 20, "m": 2, "x": 0.5}'
//!
//! `kind` is external, internal (with `rim`, the outer diameter) or rack (with `height`); the
//! other keys default to a standard 20° gear of module 2. `kind` pulley takes `profile`, `z` and
//! `clearance` and gives the toothed cross-section (flanges are discs, easily added), or with
//! `what` od or pd its outside or pitch diameter. Prints the area in mm².

use gear_kit::involute::{section_area_external, section_area_internal, Tooth};
use gear_kit::rack::{section_area, RackSpec};
use serde_json::Value;

fn main() {
    let v: Value = serde_json::from_str(&std::env::args().nth(1).expect("json")).unwrap();
    let f = |k: &str, d: f64| v.get(k).and_then(Value::as_f64).unwrap_or(d);
    let kind = v["kind"].as_str().unwrap();
    if kind == "pulley" {
        // `what`: area (default), od (outside diameter) or pd (pitch diameter)
        let p = gear_kit::pulley::profile(v["profile"].as_str().unwrap()).expect("profile");
        let z = f("z", 20.0) as u32;
        let x = match v.get("what").and_then(Value::as_str).unwrap_or("area") {
            "od" => p.outside_diameter(z),
            "pd" => p.pitch_diameter(z),
            _ => gear_kit::pulley::section_area(p, z, f("clearance", 0.0)).unwrap(),
        };
        println!("{x}");
        return;
    }
    let t = Tooth {
        z: f("z", 20.0) as u32,
        mn: f("m", 2.0),
        alpha_n: f("alpha", 20.0).to_radians(),
        beta: f("beta", 0.0).to_radians(),
        x: f("x", 0.0),
        ha: f("ha", 1.0),
        hf: f("hf", 1.25),
        rho: f("rho", if kind == "internal" { 0.25 } else { 0.38 }),
        backlash: f("backlash", 0.0),
    };
    let a = match kind {
        "external" => section_area_external(&t),
        "internal" => section_area_internal(&t, f("rim", 0.0) / 2.0),
        "rack" => section_area(&RackSpec {
            teeth: t.z,
            mn: t.mn,
            alpha_n: t.alpha_n,
            beta: t.beta,
            hand: 1.0,
            width: 1.0,
            height: f("height", 12.0),
            ha: t.ha,
            hf: t.hf,
            rho: t.rho,
            backlash: t.backlash,
            name: String::new(),
        }),
        _ => panic!("kind"),
    };
    println!("{}", a.unwrap());
}
