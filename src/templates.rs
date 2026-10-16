//! Scaffolded-extension file templates.
//!
//! Placeholders (substituted by `new`):
//!   {{NAME}}       crate name, e.g. `myplugin`
//!   {{NAME_KRAM}}  lib name = crate name with `-` → `_` (used by `[lib] name`)
//!   {{EXT_NAME}}   the DuckDB extension name = the C-API entrypoint name.
//!                  MUST equal the `.duckdb_extension` file's basename (the
//!                  DuckDB loader matches the export symbol to the filename).
//!
//! The `lib.rs` template is byte-for-byte the shape that was verified to
//! compile and LOAD into DuckDB 1.5.5 on Windows (see nest-rs-probe).

pub const CARGO_TOML: &str = r#"[package]
name = "{{NAME}}"
version = "0.1.0"
edition = "2021"
description = "DuckDB extension: {{NAME}}"
license = "MIT"

[lib]
name = "{{NAME_KRAM}}"
crate-type = ["cdylib"]

[dependencies]
# duckdb-rs `~1.10504.0` targets C-API v1.2.0 — the most host-compatible
# version (any DuckDB with C-API >= v1.2.0 can load the result). Do NOT add a
# separate `arrow` dependency: the `duckdb` crate re-exports the exact arrow
# version it was built against via `duckdb::arrow`. A direct `arrow = "58"`
# can force a re-resolve that hits yanked transitive deps.
#
# Feature notes (all required, verified):
#   loadable-extension  -> the #[duckdb_entrypoint_c_api] macro + C ABI export
#   vtab-arrow          -> arrow<->DuckDB interop for scalar functions
#   vscalar             -> exposes the `vscalar` module (the raw VScalar trait)
#   vscalar-arrow       -> exposes VArrowScalar / ArrowFunctionSignature
duckdb = { version = "~1.10504.0", features = ["loadable-extension", "vtab-arrow", "vscalar", "vscalar-arrow"] }

[profile.release]
lto = true
strip = true
"#;

pub const LIB_RS: &str = r#"//! {{NAME}} — a DuckDB extension built as a `duckdb_nest` plugin.
//!
//! Three working examples; replace them with your own functions.
//!
//! IMPORTANT — which trait to implement:
//!   * SCALAR functions: implement `VArrowScalar` (below). This is the path
//!     that works from an *external* crate. The raw `VScalar` trait's
//!     `invoke` takes the crate-private `WritableVector` type, so it CANNOT be
//!     implemented outside the `duckdb` crate itself.
//!   * TABLE functions: implement `VTab` (below) using only public `core`
//!     types.
//!
//! A Rust panic inside a registered scalar is converted by the duckdb crate
//! into a query error (it does not abort the host) — that is the built-in
//! panic guard. Keep `type State` invariants intact if you mutate it.

use std::sync::Arc;

use duckdb::{
    arrow::{
        array::{Array, Int64Array, RecordBatch, StringArray},
        datatypes::DataType,
    },
    core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId},
    duckdb_entrypoint_c_api,
    vscalar::{ArrowFunctionSignature, VArrowScalar},
    vtab::{BindInfo, InitInfo, TableFunctionInfo, VTab},
    Connection,
};

// ───────────────────────────── scalar examples ─────────────────────────────

/// `{{EXT_NAME}}_double(x BIGINT) -> BIGINT` = x * 2.
struct DoubleFn;

impl VArrowScalar for DoubleFn {
    type State = ();

    fn invoke(
        _: &Self::State,
        input: RecordBatch,
    ) -> Result<Arc<dyn Array>, Box<dyn std::error::Error>> {
        let col = input
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| "arg0 must be BIGINT".to_string())?;
        let out: Vec<i64> = col.iter().map(|v| v.unwrap_or(0) * 2).collect();
        Ok(Arc::new(Int64Array::from(out)))
    }

    fn signatures() -> Vec<ArrowFunctionSignature> {
        vec![ArrowFunctionSignature::exact(
            vec![DataType::Int64],
            DataType::Int64,
        )]
    }
}

/// `{{EXT_NAME}}_upper(s VARCHAR) -> VARCHAR`.
struct UpperFn;

impl VArrowScalar for UpperFn {
    type State = ();

    fn invoke(
        _: &Self::State,
        input: RecordBatch,
    ) -> Result<Arc<dyn Array>, Box<dyn std::error::Error>> {
        let col = input
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| "arg0 must be VARCHAR".to_string())?;
        let out: Vec<String> =
            col.iter().map(|v| v.unwrap_or("").to_uppercase()).collect();
        Ok(Arc::new(StringArray::from(out)))
    }

    fn signatures() -> Vec<ArrowFunctionSignature> {
        vec![ArrowFunctionSignature::exact(
            vec![DataType::Utf8],
            DataType::Utf8,
        )]
    }
}

// ─────────────────────────── table function example ─────────────────────────

/// `{{EXT_NAME}}_series(n BIGINT) -> (i BIGINT)`: emits rows 0..n-1.
struct SeriesInit {
    done: std::sync::atomic::AtomicBool,
}
struct SeriesBind {
    n: i64,
}

impl VTab for SeriesInit {
    type InitData = SeriesInit;
    type BindData = SeriesBind;

    fn bind(bind: &BindInfo) -> Result<Self::BindData, Box<dyn std::error::Error>> {
        // Declare the output schema. Column names/types go here.
        bind.add_result_column("i", LogicalTypeHandle::from(LogicalTypeId::Bigint));
        // Read bind parameters. Note: Value has typed getters (to_int64,
        // to_double, to_bool, ...) — not `try_into`.
        let n: i64 = bind.get_parameter(0).to_int64();
        Ok(SeriesBind { n })
    }

    fn init(_: &InitInfo) -> Result<Self::InitData, Box<dyn std::error::Error>> {
        Ok(SeriesInit {
            done: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn func(
        func: &TableFunctionInfo<Self>,
        output: &mut DataChunkHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let init_data = func.get_init_data();
        let bind_data = func.get_bind_data();

        // `func` is called repeatedly until you set_len(0) to signal done.
        // Here we emit everything in one chunk (DuckDB caps the chunk size, so
        // for large n you'd loop over multiple chunks and track progress in
        // InitData instead of a single `done` flag).
        if init_data
            .done
            .swap(true, std::sync::atomic::Ordering::Relaxed)
            || bind_data.n <= 0
        {
            output.set_len(0);
            return Ok(());
        }
        let mut col = output.flat_vector(0);
        let out = unsafe { col.as_mut_slice::<i64>() };
        for (row, i) in out.iter_mut().take(bind_data.n as usize).enumerate() {
            *i = row as i64;
        }
        output.set_len(bind_data.n as usize);
        Ok(())
    }

    fn parameters() -> Option<Vec<LogicalTypeHandle>> {
        Some(vec![LogicalTypeHandle::from(LogicalTypeId::Bigint)])
    }
}

// ───────────────────────────── entrypoint ───────────────────────────────────

/// `ext_name` must equal the `.duckdb_extension` basename, and
/// `min_duckdb_version` is the C-API floor the host must meet.
#[duckdb_entrypoint_c_api(ext_name = "{{EXT_NAME}}", min_duckdb_version = "v1.2.0")]
pub fn extension_entrypoint(con: Connection) -> Result<(), Box<dyn std::error::Error>> {
    // Register every function the extension provides. A panicking scalar is
    // contained by the duckdb crate into a per-query error (no host abort).
    con.register_scalar_function::<DoubleFn>("{{EXT_NAME}}_double")?;
    con.register_scalar_function::<UpperFn>("{{EXT_NAME}}_upper")?;
    con.register_table_function::<SeriesInit>("{{EXT_NAME}}_series")?;
    Ok(())
}
"#;
