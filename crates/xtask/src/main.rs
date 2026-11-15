//! cargo xtask gen: writes the files generated from the device tables in perihub-core.
//! cargo xtask check: fails if one of them is out of date (also run by cargo test).

mod catalog;
mod docs;
mod json;
mod metainfo;
mod select;
mod site;
mod udev;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every generated file, with its new content.
fn generated() -> Vec<(&'static str, String)> {
    let devices = catalog::devices();
    let read = |path: &str| {
        std::fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}")).replace("\r\n", "\n")
    };
    vec![
        (udev::PATH, udev::render(&devices)),
        (docs::PATH, docs::render(docs::PATH, &read(docs::PATH), &devices)),
        ("README.md", docs::render("README.md", &read("README.md"), &devices)),
        (site::PATH, site::render(&read(site::PATH), &devices)),
        (metainfo::PATH, metainfo::render(&read(metainfo::PATH), &devices)),
        (json::PATH, json::render(&devices)),
    ]
}

/// The files whose content differs from what the tables give. Windows checkouts may have CRLF.
fn stale() -> Vec<&'static str> {
    generated()
        .into_iter()
        .filter(|(path, content)| {
            std::fs::read_to_string(root().join(path)).map(|old| old.replace("\r\n", "\n")).ok().as_deref()
                != Some(content)
        })
        .map(|(path, _)| path)
        .collect()
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("gen") => {
            for (path, content) in generated() {
                std::fs::write(root().join(path), content).unwrap_or_else(|e| panic!("{path}: {e}"));
            }
            ExitCode::SUCCESS
        }
        Some("check") => {
            let stale = stale();
            for path in &stale {
                eprintln!("{path} is out of date: run cargo xtask gen");
            }
            if stale.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        _ => {
            eprintln!("usage: cargo xtask gen | check");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_files_are_up_to_date() {
        let stale = super::stale();
        assert!(stale.is_empty(), "out of date, run cargo xtask gen: {stale:?}");
    }
}
