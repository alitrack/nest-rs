# cargo-nest

Scaffold and build **Rust DuckDB extensions** that load as [`duckdb_nest`](https://github.com/alitrack/duckdb_nest) plugins.

```bash
cargo-nest new myplugin     # scaffold a ready-to-build extension crate
cd myplugin
cargo-nest build            # cargo build --release + stamp the C-API footer
                            # -> myplugin.duckdb_extension (platform auto-detected)
```

Then in any DuckDB host (or a nest):

```python
import duckdb
con = duckdb.connect(config={"allow_unsigned_extensions": "true"})
con.execute("LOAD 'myplugin.duckdb_extension'")
print(con.execute("SELECT myplugin_double(21)").fetchone())    # (42,)
print(con.execute("SELECT myplugin_upper('hi')").fetchone())   # ('HI',)
print(con.execute("SELECT i FROM myplugin_series(5)").fetchall())
```

## Why a scaffold

A hand-rolled Rust DuckDB extension hits four non-obvious traps that cost real
time to rediscover:

1. **Which trait to implement.** The raw `VScalar` trait's `invoke` takes the
   crate-private `WritableVector` type, so it **cannot** be implemented from an
   external crate. Scalar functions must go through `VArrowScalar` instead.
   The template uses the path that actually works.
2. **Dependency resolve.** A direct `arrow = "58"` dependency can force a
   re-resolve that hits yanked transitive deps (`zip 6.0.0`). The template uses
   the `duckdb::arrow` re-export — the exact arrow version duckdb-rs was built
   against — with no direct arrow dep.
3. **Crate features.** `vscalar` / `vscalar-arrow` are not enabled by
   `vtab-arrow` alone; all four features are required and listed.
4. **The metadata footer.** A `.duckdb_extension` is a cdylib with a 534-byte
   footer (platform + C-API version + ext version) that the loader validates.
   `build` stamps it in pure Rust (no Python dependency), byte-identical to
   `duckdb_nest`'s `metadata.py` (covered by tests).

The scaffold also demonstrates a **table function** (`VTab`, public `core`
types only) and notes that the duckdb crate already **contains scalar panics
into query errors** — that is the panic guard, no extra wiring needed.

## What `build` does

1. `cargo build --release` in the crate directory.
2. Locate the built cdylib under `target/release/` (`.dll` / `.so` / `.dylib`).
3. Copy it to `<crate>.duckdb_extension` and append the footer.

Options: `--platform <p>`, `--capi <ver>` (default `v1.2.0`), `--ext-ver <ver>`.

The default C-API footer is **v1.2.0** (what `duckdb_nest` stamps) — the most
host-compatible choice: any DuckDB with C-API >= v1.2.0 can load the result.

## Publishing as a nest plugin

`cargo-nest` stops at the verified `.duckdb_extension`. The trust-chain layer —
`nest.toml` manifest, sha256 pinning, Ed25519 signatures, dependency ordering —
lives in [`duckdb_nest`](https://github.com/alitrack/duckdb_nest). Ship the
extension in a directory, add a `nest.toml`, and `SELECT * FROM nest_load(...)`
in a nest host. See the generated `README.md` for the exact steps.

## Verified

The template is byte-for-byte the shape that was verified to compile and LOAD:

- Builds with duckdb-rs `~1.10504.0`, rustc 1.97.
- Loads into DuckDB 1.5.5 on Windows: scalar (numeric + string) and table
  function all compute correctly.
- Footer bytes match `duckdb_nest/scripts/metadata.py` (unit-tested).

## Build the CLI itself

```bash
cargo install --path .        # or: cargo build --release
```

Dependency-free (std only), so installation is fast.

## License

MIT
