/// A test build (the "Test build" workflow) says which branch and commit it is from, in PERIHUB_BUILD
/// ("test/viper-v3-pro-wireless@02fd962"): the test report starts with it, so a report can be tied to the
/// build it came from. The Flatpak build runs in a sandbox that keeps no environment, so
/// packaging/flatpak/build-local.sh also writes it to .perihub-build in the repository root. A release
/// build has neither. Only a branch-like text is kept: the value ends up in the first line of a report.
fn main() {
    println!("cargo:rerun-if-env-changed=PERIHUB_BUILD");
    let label = std::env::var("PERIHUB_BUILD").ok().or_else(|| std::fs::read_to_string("../../.perihub-build").ok());
    if let Some(label) = label.map(|l| l.trim().to_string()) {
        let plain = label.chars().all(|c| c.is_ascii_alphanumeric() || "._/@-".contains(c));
        if !label.is_empty() && label.len() <= 100 && plain {
            println!("cargo:rustc-env=PERIHUB_BUILD={label}");
        } else if !label.is_empty() {
            println!("cargo:warning=PERIHUB_BUILD is not a branch@commit text: ignored");
        }
    }
    tauri_build::build()
}
