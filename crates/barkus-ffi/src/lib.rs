#![allow(private_interfaces)]

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use barkus_core::generate::{decode, generate};
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use barkus_core::tape::DecisionTape;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use barkus_sql::context::SqlContext;
use barkus_sql::dialect::{GenericDialect, PostgresDialect, SqliteDialect, TrinoDialect};
use barkus_sql::SqlGenerator;

struct Handle {
    ir: GrammarIr,
    profile: Profile,
    rng: SmallRng,
}

struct SqlHandle {
    gen: SqlGenerator,
    rng: SmallRng,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: &str) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = CString::new(msg).ok();
    });
}

/// Copy `src` into a caller-provided FFI buffer. Returns 0 on success, -1 on error.
///
/// # Safety
/// `dst` must have capacity `*dst_len`. `dst_len` must be non-null.
unsafe fn write_to_buffer(src: &[u8], dst: *mut u8, dst_len: *mut usize, label: &str) -> i32 {
    let cap = *dst_len;
    if src.len() > cap {
        set_last_error(&format!("{label}: output buffer too small"));
        return -1;
    }
    ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
    *dst_len = src.len();
    0
}

fn make_rng(seed: u64) -> SmallRng {
    if seed == 0 {
        rand::make_rng()
    } else {
        SmallRng::seed_from_u64(seed)
    }
}

/// Compile an EBNF grammar source and return an opaque handle.
/// Returns null on error (call `barkus_last_error` for details).
///
/// # Safety
/// `source` must point to `source_len` valid bytes (or be null when `source_len` is 0).
#[no_mangle]
pub unsafe extern "C" fn barkus_compile(
    source: *const u8,
    source_len: usize,
    seed: u64,
    max_depth: u32,
) -> *mut Handle {
    if source.is_null() && source_len > 0 {
        set_last_error("compile error: null source pointer with non-zero length");
        return ptr::null_mut();
    }

    let src = if source_len == 0 {
        ""
    } else {
        let bytes = slice::from_raw_parts(source, source_len);
        match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => {
                set_last_error(&format!("compile error: invalid UTF-8: {e}"));
                return ptr::null_mut();
            }
        }
    };

    let ir = match barkus_ebnf::compile(src) {
        Ok(ir) => ir,
        Err(e) => {
            set_last_error(&format!("compile error: {e}"));
            return ptr::null_mut();
        }
    };

    let mut builder = Profile::builder();
    if max_depth > 0 {
        builder = builder.max_depth(max_depth);
    }
    let profile = builder.build();

    let rng = make_rng(seed);

    let handle = Box::new(Handle { ir, profile, rng });
    Box::into_raw(handle)
}

/// Generate one sample, writing output bytes into `output_buf`.
/// On entry, `*output_len` is the buffer capacity.
/// On success, `*output_len` is set to the actual length and returns 0.
/// On failure, returns -1 (call `barkus_last_error`).
///
/// # Safety
/// `handle` must be a valid pointer from `barkus_compile`. `output_buf` must
/// have capacity `*output_len`. `output_len` must be non-null.
#[no_mangle]
pub unsafe extern "C" fn barkus_generate(
    handle: *mut Handle,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => {
            set_last_error("generate error: null handle");
            return -1;
        }
    };

    let (ast, _tape, _map) = match generate(&h.ir, &h.profile, &mut h.rng) {
        Ok(result) => result,
        Err(e) => {
            set_last_error(&format!("generate error: {e}"));
            return -1;
        }
    };

    let serialized = ast.serialize();
    write_to_buffer(&serialized, output_buf, output_len, "generate error")
}

/// Generate one sample with tape. Writes output to `output_buf` and tape to `tape_buf`.
/// On entry, `*output_len` and `*tape_len` are buffer capacities.
/// On success, both are set to actual lengths and returns 0.
/// On failure, returns -1.
///
/// # Safety
/// Same requirements as `barkus_generate`, plus `tape_buf` must have capacity `*tape_len`
/// and `tape_len` must be non-null.
#[no_mangle]
pub unsafe extern "C" fn barkus_generate_with_tape(
    handle: *mut Handle,
    output_buf: *mut u8,
    output_len: *mut usize,
    tape_buf: *mut u8,
    tape_len: *mut usize,
) -> i32 {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => {
            set_last_error("generate error: null handle");
            return -1;
        }
    };

    let (ast, tape, _map) = match generate(&h.ir, &h.profile, &mut h.rng) {
        Ok(result) => result,
        Err(e) => {
            set_last_error(&format!("generate error: {e}"));
            return -1;
        }
    };

    let serialized = ast.serialize();
    let rc = write_to_buffer(&serialized, output_buf, output_len, "generate error");
    if rc != 0 {
        return rc;
    }
    write_to_buffer(&tape.bytes, tape_buf, tape_len, "generate error")
}

/// Decode output from a tape. Uses the handle's grammar and profile.
/// Writes decoded output into `output_buf`.
/// On entry, `*output_len` is the buffer capacity.
/// On success, `*output_len` is set to actual length and returns 0.
/// On failure, returns -1.
///
/// # Safety
/// `handle` must be a valid pointer from `barkus_compile`. `tape_ptr` must point to
/// `tape_len` bytes. `output_buf` must have capacity `*output_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_decode(
    handle: *mut Handle,
    tape_ptr: *const u8,
    tape_len: usize,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    let h = match handle.as_ref() {
        Some(h) => h,
        None => {
            set_last_error("decode error: null handle");
            return -1;
        }
    };

    if tape_ptr.is_null() && tape_len > 0 {
        set_last_error("decode error: null tape pointer with non-zero length");
        return -1;
    }

    let tape_bytes = if tape_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(tape_ptr, tape_len)
    };

    let (ast, _map) = match decode(&h.ir, &h.profile, tape_bytes) {
        Ok(result) => result,
        Err(e) => {
            set_last_error(&format!("decode error: {e}"));
            return -1;
        }
    };

    let serialized = ast.serialize();
    write_to_buffer(&serialized, output_buf, output_len, "decode error")
}

/// Free a handle returned by `barkus_compile`. Safe to call with null.
///
/// # Safety
/// `handle` must be a pointer returned by `barkus_compile`, or null.
#[no_mangle]
pub unsafe extern "C" fn barkus_destroy(handle: *mut Handle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Return the last error message as a null-terminated C string.
/// The pointer is valid until the next FFI call on the same thread.
/// Returns null if no error has been set.
#[no_mangle]
pub extern "C" fn barkus_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ref().map_or(ptr::null(), |cs| cs.as_ptr()))
}

// ---------------------------------------------------------------------------
// SQL generator FFI
// ---------------------------------------------------------------------------

/// JSON config blob accepted by `barkus_sql_compile`.
#[derive(serde::Deserialize, Default)]
struct SqlConfig {
    schema: Option<SqlContext>,
    max_depth: Option<u32>,
    max_total_nodes: Option<u32>,
    validity_mode: Option<barkus_core::profile::ValidityMode>,
}

/// Compile an SQL generator for the given dialect.
///
/// `dialect` / `dialect_len`: UTF-8 dialect name (`"postgresql"`, `"sqlite"`, `"trino"`, `"generic"`).
/// `config_json` / `config_json_len`: optional JSON config blob (null / 0 to use defaults).
/// `seed`: RNG seed (0 = random).
///
/// Returns an opaque handle, or null on error.
///
/// # Safety
/// Pointers must be valid for their stated lengths. Null is OK when length is 0.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_compile(
    dialect: *const u8,
    dialect_len: usize,
    config_json: *const u8,
    config_json_len: usize,
    seed: u64,
) -> *mut SqlHandle {
    // Parse dialect string.
    let dialect_str = if dialect_len == 0 {
        "sqlite"
    } else {
        if dialect.is_null() {
            set_last_error("sql compile error: null dialect pointer with non-zero length");
            return ptr::null_mut();
        }
        match std::str::from_utf8(slice::from_raw_parts(dialect, dialect_len)) {
            Ok(s) => s,
            Err(e) => {
                set_last_error(&format!("sql compile error: invalid UTF-8 dialect: {e}"));
                return ptr::null_mut();
            }
        }
    };

    // Parse optional config JSON.
    let config: SqlConfig = if config_json.is_null() || config_json_len == 0 {
        SqlConfig::default()
    } else {
        let json_bytes = slice::from_raw_parts(config_json, config_json_len);
        match serde_json::from_slice(json_bytes) {
            Ok(c) => c,
            Err(e) => {
                set_last_error(&format!("sql compile error: invalid config JSON: {e}"));
                return ptr::null_mut();
            }
        }
    };

    // Build profile.
    let mut profile_builder = Profile::builder();
    if let Some(d) = config.max_depth {
        profile_builder = profile_builder.max_depth(d);
    }
    if let Some(n) = config.max_total_nodes {
        profile_builder = profile_builder.max_total_nodes(n);
    }
    if let Some(v) = config.validity_mode {
        profile_builder = profile_builder.validity_mode(v);
    }
    let profile = profile_builder.build();

    // Build generator with dialect + embedded grammar.
    let mut builder = SqlGenerator::builder().profile(profile);

    if let Some(ctx) = config.schema {
        builder = builder.context(ctx);
    }

    // Select dialect and its bundled grammar.
    match dialect_str {
        "postgresql" => {
            builder = builder.dialect(PostgresDialect).grammar(
                include_str!("../../../grammars/antlr-sql/postgresql/PostgreSQLLexer.g4"),
                include_str!("../../../grammars/antlr-sql/postgresql/PostgreSQLParser.g4"),
            );
        }
        "sqlite" => {
            builder = builder.dialect(SqliteDialect);
            // SqliteDialect + default grammar is the builder default, but set dialect explicitly.
        }
        "trino" => {
            builder = builder.dialect(TrinoDialect).grammar(
                include_str!("../../../grammars/antlr-sql/trino/TrinoLexer.g4"),
                include_str!("../../../grammars/antlr-sql/trino/TrinoParser.g4"),
            );
        }
        "generic" => {
            builder = builder.dialect(GenericDialect);
        }
        _ => {
            set_last_error(&format!(
                "sql compile error: unknown dialect: {dialect_str:?}"
            ));
            return ptr::null_mut();
        }
    }

    let gen = match builder.build() {
        Ok(g) => g,
        Err(e) => {
            set_last_error(&format!("sql compile error: {e}"));
            return ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(SqlHandle {
        gen,
        rng: make_rng(seed),
    }))
}

/// Generate one SQL string.
///
/// # Safety
/// `handle` must be from `barkus_sql_compile`. `output_buf` capacity = `*output_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_generate(
    handle: *mut SqlHandle,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => {
            set_last_error("sql generate error: null handle");
            return -1;
        }
    };

    let (sql, _tape, _map) = match h.gen.generate(&mut h.rng) {
        Ok(r) => r,
        Err(e) => {
            set_last_error(&format!("sql generate error: {e}"));
            return -1;
        }
    };

    write_to_buffer(sql.as_bytes(), output_buf, output_len, "sql generate error")
}

/// Generate one SQL string and record the decision tape.
///
/// # Safety
/// Same as `barkus_sql_generate`, plus `tape_buf` capacity = `*tape_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_generate_with_tape(
    handle: *mut SqlHandle,
    output_buf: *mut u8,
    output_len: *mut usize,
    tape_buf: *mut u8,
    tape_len: *mut usize,
) -> i32 {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => {
            set_last_error("sql generate error: null handle");
            return -1;
        }
    };

    let (sql, tape, _map) = match h.gen.generate(&mut h.rng) {
        Ok(r) => r,
        Err(e) => {
            set_last_error(&format!("sql generate error: {e}"));
            return -1;
        }
    };

    let rc = write_to_buffer(sql.as_bytes(), output_buf, output_len, "sql generate error");
    if rc != 0 {
        return rc;
    }
    write_to_buffer(&tape.bytes, tape_buf, tape_len, "sql generate error")
}

/// Replay a decision tape to reproduce SQL output.
///
/// # Safety
/// `handle` from `barkus_sql_compile`. `tape_ptr` has `tape_len` bytes.
/// `output_buf` capacity = `*output_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_decode(
    handle: *mut SqlHandle,
    tape_ptr: *const u8,
    tape_len: usize,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    let h = match handle.as_ref() {
        Some(h) => h,
        None => {
            set_last_error("sql decode error: null handle");
            return -1;
        }
    };

    if tape_ptr.is_null() && tape_len > 0 {
        set_last_error("sql decode error: null tape pointer with non-zero length");
        return -1;
    }

    let tape_bytes = if tape_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(tape_ptr, tape_len)
    };

    let tape = DecisionTape {
        bytes: tape_bytes.to_vec(),
    };

    let (sql, _map) = match h.gen.decode(&tape) {
        Ok(r) => r,
        Err(e) => {
            set_last_error(&format!("sql decode error: {e}"));
            return -1;
        }
    };

    write_to_buffer(sql.as_bytes(), output_buf, output_len, "sql decode error")
}

/// Free an SQL handle. Safe to call with null.
///
/// # Safety
/// `handle` must be from `barkus_sql_compile`, or null.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_destroy(handle: *mut SqlHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}
