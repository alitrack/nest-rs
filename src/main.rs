//! cargo-nest: scaffold and build Rust DuckDB extensions for use as
//! `duckdb_nest` plugins.
//!
//!   cargo-nest new <name> [--dir <out>]   scaffold a new extension crate
//!   cargo-nest build [crate-dir]          cargo build --release, then append
//!                                         the DuckDB metadata footer to produce
//!                                         <name>.duckdb_extension

mod footer;
mod templates;

use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

const DEFAULT_CAPI: &str = footer::DEFAULT_CAPI_VERSION;

/// A known-good dependency lock for a scaffolded extension. `libduckdb-sys`
/// (pulled in by `duckdb`) requires `zip ^6`, and 6.0.0 is the only 6.x — it is
/// yanked, so a *fresh* resolve fails. Shipping this lock pins the working set
/// (including the yanked-but-needed zip 6.0.0) so `cargo build` succeeds out of
/// the box. The root package entry carries a placeholder that `new` substitutes
/// with the scaffolded crate name; cargo self-heals the lock on first build.
const DEPENDENCY_LOCK: &str = include_str!("../assets/dep.lock");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("new") => cmd_new(&args[1..]),
        Some("build") => cmd_build(&args[1..]),
        Some("-h") | Some("--help") | Some("help") => {
            print_help();
            0
        }
        Some(other) => {
            eprintln!("cargo-nest: unknown command '{other}'\n");
            print_help();
            2
        }
        None => {
            print_help();
            2
        }
    };
    std::process::exit(code);
}

fn print_help() {
    println!(
        "cargo-nest — Rust DuckDB extension scaffold for duckdb_nest plugins

USAGE:
    cargo-nest new <name> [--dir <out>]
    cargo-nest build [crate-dir] [--platform <p>] [--capi <ver>] [--ext-ver <ver>]

COMMANDS:
    new     Scaffold a ready-to-build extension crate with verified scalar
            (VArrowScalar) and table (VTab) examples.
    build   Run `cargo build --release` in the crate, then append the DuckDB
            metadata footer to produce <name>.duckdb_extension (platform is
            auto-detected).

OPTIONS (build):
    --platform <p>     windows_amd64 | linux_amd64 | linux_arm64 | osx_arm64
                       (default: auto-detected from the host)
    --capi <ver>       C-API version footer (default: v1.2.0)
    --ext-ver <ver>    extension version field (default: 0.1.0)

EXAMPLES:
    cargo-nest new myplugin
    cd myplugin && cargo-nest build
    # then load:
    #   import duckdb
    #   con = duckdb.connect(config={{\"allow_unsigned_extensions\": \"true\"}})
    #   con.execute(\"LOAD 'myplugin.duckdb_extension'\")
    #   con.execute(\"SELECT myplugin_upper('hi')\").fetchone()
"
    );
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    s.trim_matches('_').to_string()
}

fn cmd_new(args: &[String]) -> i32 {
    let mut name: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" | "-d" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("cargo-nest: --dir needs a value");
                    return 2;
                }
                out = Some(PathBuf::from(&args[i]));
            }
            other if other.starts_with('-') => {
                eprintln!("cargo-nest: unknown option for `new`: {other}");
                return 2;
            }
            other => {
                if name.is_some() {
                    eprintln!("cargo-nest: unexpected extra argument '{other}'");
                    return 2;
                }
                name = Some(other.to_string());
            }
        }
        i += 1;
    }
    let name = match name {
        Some(n) => n,
        None => {
            eprintln!("cargo-nest: missing <name>\n\nRun `cargo-nest new <name>`");
            return 2;
        }
    };
    let slug = slug(&name);
    if slug.is_empty() {
        eprintln!("cargo-nest: name '{name}' has no usable characters");
        return 2;
    }
    if slug.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        eprintln!("cargo-nest: crate name '{slug}' cannot start with a digit");
        return 2;
    }

    let root = match out {
        Some(d) => d.join(&slug),
        None => PathBuf::from(&slug),
    };
    if root.exists() {
        eprintln!("cargo-nest: '{}' already exists — refusing to overwrite", root.display());
        return 1;
    }

    let lib_name = slug.replace('-', "_");
    let cargo_toml = templates::CARGO_TOML
        .replace("{{NAME}}", &slug)
        .replace("{{NAME_KRAM}}", &lib_name);
    let lib_rs = templates::LIB_RS
        .replace("{{NAME}}", &slug)
        .replace("{{EXT_NAME}}", &slug);
    // Known-good lock: substitute the root package name so `cargo build`
    // resolves to the exact (yank-safe) dependency set on first run.
    let cargo_lock = DEPENDENCY_LOCK.replace("name = \"__NESTRS_TEMPLATE__\"", &format!("name = \"{slug}\""));

    let readme_md = readme(&slug);
    let files = [
        ("Cargo.toml", cargo_toml.as_str()),
        ("Cargo.lock", cargo_lock.as_str()),
        ("src/lib.rs", lib_rs.as_str()),
        (".gitignore", "/target\n*.duckdb_extension\n"),
        ("README.md", readme_md.as_str()),
        ("LICENSE", "MIT License\n\nCopyright (c) 2026\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.\n"),
    ];

    for (rel, content) in files {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Err(e) = std::fs::write(&p, content) {
            eprintln!("cargo-nest: failed to write {}: {e}", p.display());
            return 1;
        }
    }

    println!("Scaffolded {slug} at {}:", root.display());
    println!("  Cargo.toml");
    println!("  src/lib.rs        (2 scalar examples + 1 table-function example)");
    println!("  README.md");
    println!();
    println!("Next:");
    println!("  cd {slug}");
    println!("  cargo-nest build");
    println!("  # produces {slug}.duckdb_extension (footer {DEFAULT_CAPI})");
    0
}

fn readme(name: &str) -> String {
    format!(
        "# {name}\n\nA Rust DuckDB extension scaffolded by [`cargo-nest`](https://crates.io/crates/cargo-nest), intended to load as a [`duckdb_nest`](https://github.com/alitrack/duckdb_nest) plugin.\n\n## Build\n\n```bash\ncargo-nest build          # or: cargo build --release + footer stamping\n```\n\nProduces `{name}.duckdb_extension` (C-API {DEFAULT_CAPI}, platform auto-detected).\n\n## Verify (quick)\n\n```python\nimport duckdb\ncon = duckdb.connect(config={{\"allow_unsigned_extensions\": \"true\"}})\ncon.execute(\"LOAD '{name}.duckdb_extension'\")\nprint(con.execute(\"SELECT {name}_double(21)\").fetchone())   # (42,)\nprint(con.execute(\"SELECT {name}_upper('hi')\").fetchone())  # ('HI',)\nprint(con.execute(\"SELECT i FROM {name}_series(5)\").fetchall())  # [(0,)...(4,)]\n```\n\n## Publish as a nest plugin\n\nShip `{name}.duckdb_extension` in a directory, optionally with a `nest.toml` manifest:\n\n```toml\n[[plugin]]\nname = \"{name}\"\nfile = \"{name}.duckdb_extension\"\nsha256 = \"<64 hex>\"      # pin the exact bytes\n# signature = \"<128 hex>\" # optional Ed25519, verified against the nest's key\n```\n\nGenerate the pin/sig with the `duckdb_nest` tools:\n\n```bash\npython3 scripts/nest_sign.py genkeys --dir ./keys   # once\npython3 scripts/nest_sign.py sign {name}.duckdb_extension --privkey keys/nest_ed25519_private.key\n```\n\nThen `SELECT * FROM nest_load('/path/to/plugin-dir')` in a nest host.\n\n## Notes\n\n- Scalar functions here use `VArrowScalar` (works from external crates). The\n  raw `VScalar` trait needs the crate-private `WritableVector` type and cannot\n  be implemented outside the `duckdb` crate.\n- Table functions use `VTab` with public `core` types only.\n- A Rust panic in a registered scalar is contained into a query error by the\n  duckdb crate (the host does not abort).\n",
    )
}

fn auto_platform() -> String {
    #[cfg(target_os = "windows")]
    {
        "windows_amd64".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64")
            || std::env::consts::ARCH == "aarch64"
        {
            if std::env::consts::OS == "macos" {
                "osx_arm64".to_string()
            } else {
                "linux_arm64".to_string()
            }
        } else if std::env::consts::OS == "macos" {
            "osx_amd64".to_string()
        } else {
            "linux_amd64".to_string()
        }
    }
}

fn cmd_build(args: &[String]) -> i32 {
    let mut dir: PathBuf = PathBuf::from(".");
    let mut platform: Option<String> = None;
    let mut capi = DEFAULT_CAPI.to_string();
    let mut ext_ver = "0.1.0".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--platform" | "-p" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("cargo-nest: --platform needs a value");
                    return 2;
                }
                platform = Some(args[i].clone());
            }
            "--capi" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("cargo-nest: --capi needs a value");
                    return 2;
                }
                capi = args[i].clone();
            }
            "--ext-ver" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("cargo-nest: --ext-ver needs a value");
                    return 2;
                }
                ext_ver = args[i].clone();
            }
            other if other.starts_with('-') => {
                eprintln!("cargo-nest: unknown option for `build`: {other}");
                return 2;
            }
            other => {
                if dir != PathBuf::from(".") {
                    eprintln!("cargo-nest: unexpected extra argument '{other}'");
                    return 2;
                }
                dir = PathBuf::from(other);
            }
        }
        i += 1;
    }
    let platform = platform.unwrap_or_else(auto_platform);

    // Read crate name from Cargo.toml.
    let cargo_toml = dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        eprintln!("cargo-nest: no Cargo.toml in {} — is this a crate dir?", dir.display());
        return 1;
    }
    let meta = match std::fs::read_to_string(&cargo_toml) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cargo-nest: cannot read Cargo.toml: {e}");
            return 1;
        }
    };
    let crate_name = match meta
        .lines()
        .find(|l| l.trim_start().starts_with("name"))
        .and_then(|l| l.split_once('=').map(|(_, v)| v.trim()))
    {
        Some(n) => n.trim_matches('"').trim_matches('\'').to_string(),
        None => {
            eprintln!("cargo-nest: could not find package `name` in Cargo.toml");
            return 1;
        }
    };

    // 1) cargo build --release
    println!("cargo build --release ({crate_name}) …");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("cargo-nest: cargo build failed ({s})");
            return 1;
        }
        Err(e) => {
            eprintln!("cargo-nest: could not run cargo: {e}");
            return 1;
        }
    }

    // 2) locate the built cdylib
    let lib_name = crate_name.replace('-', "_");
    // Windows cdylibs are `name.dll` (no `lib` prefix); macOS/Linux are
    // `libname.dylib` / `libname.so`. Try every combination.
    let mut candidates: Vec<String> = Vec::new();
    for pfx in ["lib", ""] {
        for ext in [".dll", ".so", ".dylib"] {
            candidates.push(format!("{pfx}{lib_name}{ext}"));
        }
    }
    let lib_path = match candidates
        .iter()
        .find_map(|name| {
            let p = dir.join(format!("target/release/{name}"));
            p.is_file().then_some(p)
        }) {
        Some(p) => p,
        None => {
            eprintln!(
                "cargo-nest: built cdylib not found (looked for {lib_name}.{{dll,so,dylib}} under target/release)."
            );
            return 1;
        }
    };

    // 3) copy + append footer -> <crate>.duckdb_extension next to the crate
    let out_path = dir.join(format!("{crate_name}.duckdb_extension"));
    let _ = std::fs::remove_file(&out_path);
    if let Err(e) = std::fs::copy(&lib_path, &out_path) {
        eprintln!("cargo-nest: cannot copy {}: {e}", lib_path.display());
        return 1;
    }
    match footer::append_footer(&out_path, &platform, &capi, &ext_ver) {
        Ok(sz) => {
            println!("Wrote {}", out_path.display());
            println!("  platform = {platform}");
            println!("  C-API    = {capi}");
            println!("  version  = {ext_ver}");
            println!("  size     = {sz} bytes");
            println!();
            println!("Load & test:");
            println!(
                "  python -c \"import duckdb; c=duckdb.connect(config={{'allow_unsigned_extensions':'true'}}); c.execute(\\\"LOAD '{}\\\"\"); print(c.execute(\\\"SELECT {}_double(21)\\\").fetchone())\"",
                out_path.display(), crate_name
            );
            0
        }
        Err(e) => {
            eprintln!("cargo-nest: footer append failed: {e}");
            1
        }
    }
}
