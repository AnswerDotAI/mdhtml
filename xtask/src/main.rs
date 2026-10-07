//! `cargo wasm` builds the browser module and writes the npm package `@answerdotai/mdhtml` into `wasm/pkg/`. The package holds the module,
//! the loader `mdhtml.js`, `package.json` and `README.md`. By default it builds with the incremental `release` profile.
//! `cargo wasm --profile wasm` builds the smaller module, which is the one published to npm.
//!
//! `package.json` takes its version from the workspace. Its description, licence, repository and README come from `pyproject.toml`'s
//! `[project]` table, which PyPI also reads.
use std::{env, fs, path::Path, process::Command};

const TARGET: &str = "wasm32-unknown-unknown";
/// The npm package's name. It differs from the PyPI name, because `mdhtml` was taken on npm.
const NAME: &str = "@answerdotai/mdhtml";
/// The file that `mdhtml-wasm` builds.
const MODULE: &str = "mdhtml_wasm.wasm";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let profile = match args.as_slice() {
        [] => "release",
        [flag, name] if flag == "--profile" => name,
        _ => anyhow::bail!("usage: cargo wasm [--profile NAME]"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask is a workspace member");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let built = Command::new(cargo).current_dir(root).args(["build", "-p", "mdhtml-wasm", "--target", TARGET, "--profile", profile]).status()?;
    anyhow::ensure!(built.success(), "the wasm build failed");
    let targets = env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), Into::into);
    let pkg = root.join("wasm/pkg");
    fs::create_dir_all(&pkg)?;
    fs::copy(targets.join(TARGET).join(profile).join(MODULE), pkg.join(MODULE))?;
    fs::copy(root.join("wasm/mdhtml.js"), pkg.join("mdhtml.js"))?;
    let pyproject: toml::Table = fs::read_to_string(root.join("pyproject.toml"))?.parse()?;
    let project = &pyproject["project"];
    let repository = project["urls"]["Repository"].as_str().expect("pyproject.toml's Repository is a URL");
    let package = serde_json::json!({
        "name": NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "description": project["description"],
        "license": project["license"],
        "repository": { "type": "git", "url": format!("git+{repository}.git") },
        "type": "module",
        "main": "mdhtml.js",
        "publishConfig": { "access": "public" },
    });
    fs::write(pkg.join("package.json"), serde_json::to_string_pretty(&package)?)?;
    fs::copy(root.join(project["readme"].as_str().expect("pyproject.toml's readme is a path")), pkg.join("README.md"))?;
    Ok(())
}
