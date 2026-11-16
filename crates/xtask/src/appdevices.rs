//! app/src/lib/devices.generated.json: the brands and models the app lists under Settings, grouped by
//! brand, each model once (a model with two USB ids is one model). Receivers are left out: they carry
//! the mice paired to them, which are listed.

use serde::Serialize;

use crate::catalog::{Device, Kind, Level};

pub const PATH: &str = "app/src/lib/devices.generated.json";

#[derive(Serialize)]
struct Catalog<'a> {
    /// What the file is, for whoever finds it.
    about: &'static str,
    /// In the order of the driver tables.
    brands: Vec<Brand<'a>>,
}

#[derive(Serialize)]
struct Brand<'a> {
    brand: &'a str,
    models: Vec<Model<'a>>,
}

#[derive(Serialize)]
struct Model<'a> {
    name: &'a str,
    kind: Kind,
    level: Level,
}

pub fn render(devices: &[Device]) -> String {
    let mut brands: Vec<Brand> = Vec::new();
    for d in devices.iter().filter(|d| d.kind != Kind::Receiver) {
        let i = match brands.iter().position(|b| b.brand == d.brand) {
            Some(i) => i,
            None => {
                brands.push(Brand { brand: d.brand, models: Vec::new() });
                brands.len() - 1
            }
        };
        let models = &mut brands[i].models;
        if !models.iter().any(|m| m.name == d.name) {
            models.push(Model { name: &d.name, kind: d.kind, level: d.level });
        }
    }
    let catalog = Catalog {
        about: "Brands and models listed in Settings, generated from the driver tables (cargo xtask gen).",
        brands,
    };
    serde_json::to_string_pretty(&catalog).unwrap() + "\n"
}
