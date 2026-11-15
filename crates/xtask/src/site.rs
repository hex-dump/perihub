//! site/index.html: the counts and model lists of the device catalogue, and the names the search
//! looks for (`data-models` of a row with `data-catalog="SELECTOR"`).

use crate::catalog::Device;
use crate::select::{self, block, fill};

pub const PATH: &str = "site/index.html";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Rewrites `data-models` after every `data-catalog`.
fn search_names(html: &str, devices: &[Device]) -> String {
    const CATALOG: &str = "data-catalog=\"";
    const MODELS: &str = "\" data-models=\"";
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find(CATALOG) {
        let start = i + CATALOG.len();
        let selector_end = start
            + rest[start..]
                .find(MODELS)
                .unwrap_or_else(|| panic!("{PATH}: data-catalog must be followed by data-models"));
        let models_start = selector_end + MODELS.len();
        let models_end = models_start + rest[models_start..].find('"').unwrap();
        let selector = &rest[start..selector_end];
        let names: Vec<String> = select::names(devices, selector).into_iter().map(escape).collect();
        out.push_str(&rest[..models_start]);
        out.push_str(&names.join("|"));
        rest = &rest[models_end..];
    }
    out + rest
}

pub fn render(old: &str, devices: &[Device]) -> String {
    let html = fill(old, PATH, |kind, selector, indent| match kind {
        "count" => select::names(devices, selector).len().to_string(),
        "list" => {
            block(select::names(devices, selector).into_iter().map(|n| format!("<li>{}</li>", escape(n))), indent)
        }
        _ => panic!("{PATH}: unknown generated block {kind:?}"),
    });
    search_names(&html, devices)
}
