#![allow(private_interfaces)]

use std::any::Any;
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::slice;
use std::sync::Mutex;

use barkus_core::generate::{decode, generate};
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use barkus_core::tape::DecisionTape;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use barkus_sql::context::SqlContext;
use barkus_sql::dialect::{
    GenericDialect, MySqlDialect, PostgresDialect, SqliteDialect, TrinoDialect,
};
use barkus_sql::SqlGenerator;

/// Compiled grammar handle. `ir` and `profile` are immutable after compile;
/// `rng` is wrapped in a Mutex so a Handle can be shared across threads:
/// `barkus_decode` (which never consults rng) is wait-free across any number
/// of goroutines, and `barkus_generate` (which does consume rng) serializes
/// on the mutex. `unsafe impl Sync` makes this contract visible to Go's
/// cgo layer, which carries no automatic Sync inference.
struct Handle {
    ir: GrammarIr,
    profile: Profile,
    rng: Mutex<SmallRng>,
}
unsafe impl Sync for Handle {}

struct SqlHandle {
    gen: SqlGenerator,
    rng: Mutex<SmallRng>,
}
unsafe impl Sync for SqlHandle {}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_last_error(msg: &str) {
    let cstring = CString::new(msg).unwrap_or_else(|_| {
        let sanitized = msg.replace('\0', "\\0");
        CString::new(sanitized).expect("sanitized error message contains no NUL bytes")
    });
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(cstring);
    });
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(&s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

fn ffi_catch_i32<F: FnOnce() -> i32>(f: F) -> i32 {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(rc) => rc,
        Err(payload) => {
            set_last_error(&format!("internal panic: {}", panic_message(payload)));
            -1
        }
    }
}

fn ffi_catch_ptr<T, F: FnOnce() -> *mut T>(f: F) -> *mut T {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(ptr) => ptr,
        Err(payload) => {
            set_last_error(&format!("internal panic: {}", panic_message(payload)));
            ptr::null_mut()
        }
    }
}

fn ffi_catch_void<F: FnOnce()>(f: F) {
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(f)) {
        set_last_error(&format!("internal panic: {}", panic_message(payload)));
    }
}

/// Validate that a caller-provided FFI buffer can hold `src` without mutating any out-params.
///
/// # Safety
/// `dst_len` must be valid for reads when non-null.
unsafe fn validate_buffer(
    src: &[u8],
    dst: *mut u8,
    dst_len: *mut usize,
    label: &str,
    buf_name: &str,
) -> i32 {
    if dst_len.is_null() {
        set_last_error(&format!("{label}: null {buf_name} length pointer"));
        return -1;
    }
    let cap = *dst_len;
    if src.len() > cap {
        set_last_error(&format!("{label}: {buf_name} buffer too small"));
        return -1;
    }
    if !src.is_empty() && dst.is_null() {
        set_last_error(&format!("{label}: null {buf_name} buffer pointer"));
        return -1;
    }
    0
}

/// Copy `src` into a caller-provided FFI buffer. Returns 0 on success, -1 on error.
///
/// # Safety
/// `dst` must have capacity `*dst_len` when non-null. `dst_len` must be valid when non-null.
unsafe fn write_to_buffer(src: &[u8], dst: *mut u8, dst_len: *mut usize, label: &str) -> i32 {
    if validate_buffer(src, dst, dst_len, label, "output") != 0 {
        return -1;
    }
    if !src.is_empty() {
        ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
    }
    *dst_len = src.len();
    0
}

/// Validate both output and tape buffers before mutating either buffer or length pointer.
///
/// # Safety
/// `output_buf` / `tape_buf` must have capacities `*output_len` / `*tape_len` when non-null.
unsafe fn write_output_and_tape(
    output: &[u8],
    output_buf: *mut u8,
    output_len: *mut usize,
    tape: &[u8],
    tape_buf: *mut u8,
    tape_len: *mut usize,
    label: &str,
) -> i32 {
    if validate_buffer(output, output_buf, output_len, label, "output") != 0 {
        return -1;
    }
    if validate_buffer(tape, tape_buf, tape_len, label, "tape") != 0 {
        return -1;
    }
    if !output.is_empty() {
        ptr::copy_nonoverlapping(output.as_ptr(), output_buf, output.len());
    }
    if !tape.is_empty() {
        ptr::copy_nonoverlapping(tape.as_ptr(), tape_buf, tape.len());
    }
    *output_len = output.len();
    *tape_len = tape.len();
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
    ffi_catch_ptr(|| unsafe {
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

        let handle = Box::new(Handle {
            ir,
            profile,
            rng: Mutex::new(make_rng(seed)),
        });
        Box::into_raw(handle)
    })
}

#[derive(serde::Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum GrammarFormat {
    #[default]
    Ebnf,
    Antlr,
    Peg,
}

#[derive(serde::Deserialize, Default)]
struct GrammarConfig {
    max_depth: Option<u32>,
    max_total_nodes: Option<u32>,
    validity_mode: Option<barkus_core::profile::ValidityMode>,
    #[serde(default)]
    format: GrammarFormat,
}

/// Parse an optional JSON config blob from the FFI's (ptr, len) pair.
/// Returns `Some(Default::default())` when the input is null/empty,
/// `None` after setting last_error on a malformed payload.
///
/// # Safety
/// `ptr` must be valid for `len` bytes (or null when `len == 0`).
unsafe fn parse_optional_config<T>(ptr: *const u8, len: usize, label: &str) -> Option<T>
where
    T: Default + serde::de::DeserializeOwned,
{
    if ptr.is_null() && len > 0 {
        let msg = format!("{label}: null config pointer with non-zero length");
        set_last_error(&msg);
        return None;
    }
    if len == 0 {
        return Some(T::default());
    }
    let bytes = slice::from_raw_parts(ptr, len);
    match serde_json::from_slice(bytes) {
        Ok(c) => Some(c),
        Err(e) => {
            set_last_error(&format!("{label}: invalid config JSON: {e}"));
            None
        }
    }
}

fn apply_profile_overrides(
    mut b: barkus_core::profile::ProfileBuilder,
    max_depth: Option<u32>,
    max_total_nodes: Option<u32>,
    validity_mode: Option<barkus_core::profile::ValidityMode>,
) -> barkus_core::profile::ProfileBuilder {
    if let Some(d) = max_depth {
        b = b.max_depth(d);
    }
    if let Some(n) = max_total_nodes {
        b = b.max_total_nodes(n);
    }
    if let Some(v) = validity_mode {
        b = b.validity_mode(v);
    }
    b
}

/// Compile a grammar with a JSON config blob (`validity_mode`, `max_depth`,
/// `max_total_nodes`, `format`). Pass null/0 for the config to use defaults.
/// Returns null on error (call `barkus_last_error` for details).
///
/// # Safety
/// `source` / `config_json` must point to valid bytes for their stated lengths
/// (or be null when length is 0).
#[no_mangle]
pub unsafe extern "C" fn barkus_compile_with_config(
    source: *const u8,
    source_len: usize,
    config_json: *const u8,
    config_json_len: usize,
    seed: u64,
) -> *mut Handle {
    ffi_catch_ptr(|| unsafe {
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
                    set_last_error(&format!("compile error: invalid UTF-8 source: {e}"));
                    return ptr::null_mut();
                }
            }
        };

        let config: GrammarConfig =
            match parse_optional_config(config_json, config_json_len, "compile error") {
                Some(c) => c,
                None => return ptr::null_mut(),
            };

        let ir_result = match config.format {
            GrammarFormat::Ebnf => barkus_ebnf::compile(src).map_err(|e| e.to_string()),
            GrammarFormat::Antlr => barkus_antlr::compile(src).map_err(|e| e.to_string()),
            GrammarFormat::Peg => barkus_peg::compile(src).map_err(|e| e.to_string()),
        };
        let ir = match ir_result {
            Ok(ir) => ir,
            Err(e) => {
                set_last_error(&format!("compile error: {e}"));
                return ptr::null_mut();
            }
        };

        let profile = apply_profile_overrides(
            Profile::builder(),
            config.max_depth,
            config.max_total_nodes,
            config.validity_mode,
        )
        .build();

        let handle = Box::new(Handle {
            ir,
            profile,
            rng: Mutex::new(make_rng(seed)),
        });
        Box::into_raw(handle)
    })
}

/// Generate one sample, writing output bytes into `output_buf`.
/// On entry, `*output_len` is the buffer capacity.
/// On success, `*output_len` is set to the actual length and returns 0.
/// On failure, returns -1 (call `barkus_last_error`).
///
/// Concurrency: safe to call from multiple threads on the same handle;
/// the call serializes on an internal RNG mutex (cheap because generate
/// is invoked far less often than decode in fuzz harnesses).
///
/// # Safety
/// `handle` must be a valid pointer from `barkus_compile`. `output_buf` must
/// have capacity `*output_len`. `output_len` must be non-null.
#[no_mangle]
pub unsafe extern "C" fn barkus_generate(
    handle: *const Handle,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    ffi_catch_i32(|| unsafe {
        let h = match handle.as_ref() {
            Some(h) => h,
            None => {
                set_last_error("generate error: null handle");
                return -1;
            }
        };

        let mut rng = match h.rng.lock() {
            Ok(guard) => guard,
            Err(_) => {
                set_last_error("generate error: rng mutex poisoned");
                return -1;
            }
        };
        let (ast, _tape, _map) = match generate(&h.ir, &h.profile, &mut *rng) {
            Ok(result) => result,
            Err(e) => {
                set_last_error(&format!("generate error: {e}"));
                return -1;
            }
        };

        let serialized = ast.serialize();
        write_to_buffer(&serialized, output_buf, output_len, "generate error")
    })
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
    handle: *const Handle,
    output_buf: *mut u8,
    output_len: *mut usize,
    tape_buf: *mut u8,
    tape_len: *mut usize,
) -> i32 {
    ffi_catch_i32(|| unsafe {
        let h = match handle.as_ref() {
            Some(h) => h,
            None => {
                set_last_error("generate error: null handle");
                return -1;
            }
        };

        let mut rng = match h.rng.lock() {
            Ok(guard) => guard,
            Err(_) => {
                set_last_error("generate error: rng mutex poisoned");
                return -1;
            }
        };
        let (ast, tape, _map) = match generate(&h.ir, &h.profile, &mut *rng) {
            Ok(result) => result,
            Err(e) => {
                set_last_error(&format!("generate error: {e}"));
                return -1;
            }
        };

        let serialized = ast.serialize();
        write_output_and_tape(
            &serialized,
            output_buf,
            output_len,
            &tape.bytes,
            tape_buf,
            tape_len,
            "generate error",
        )
    })
}

/// Decode output from a tape. Uses the handle's grammar and profile.
/// Writes decoded output into `output_buf`.
/// On entry, `*output_len` is the buffer capacity.
/// On success, `*output_len` is set to actual length and returns 0.
/// On failure, returns -1.
///
/// Concurrency: safe to call from multiple threads on the same handle.
/// Decode reads `ir` and `profile` only; it never consults the RNG, so
/// no synchronization is needed.
///
/// # Safety
/// `handle` must be a valid pointer from `barkus_compile`. `tape_ptr` must point to
/// `tape_len` bytes. `output_buf` must have capacity `*output_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_decode(
    handle: *const Handle,
    tape_ptr: *const u8,
    tape_len: usize,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    ffi_catch_i32(|| unsafe {
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
    })
}

/// Free a handle returned by `barkus_compile`. Safe to call with null.
///
/// # Safety
/// `handle` must be a pointer returned by `barkus_compile`, or null.
#[no_mangle]
pub unsafe extern "C" fn barkus_destroy(handle: *mut Handle) {
    ffi_catch_void(|| {
        if !handle.is_null() {
            unsafe {
                drop(Box::from_raw(handle));
            }
        }
    });
}

/// Return the last error message as a null-terminated C string.
/// The pointer is valid until the next FFI call on the same thread.
/// Returns null if no error has been set.
#[no_mangle]
pub extern "C" fn barkus_last_error() -> *const c_char {
    panic::catch_unwind(|| {
        LAST_ERROR.with(|cell| cell.borrow().as_ref().map_or(ptr::null(), |cs| cs.as_ptr()))
    })
    .unwrap_or(ptr::null())
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
    ffi_catch_ptr(|| unsafe {
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

        let config: SqlConfig =
            match parse_optional_config(config_json, config_json_len, "sql compile error") {
                Some(c) => c,
                None => return ptr::null_mut(),
            };

        let profile = apply_profile_overrides(
            Profile::builder(),
            config.max_depth,
            config.max_total_nodes,
            config.validity_mode,
        )
        .build();

        // Build generator with dialect + embedded grammar.
        let mut builder = SqlGenerator::builder().profile(profile);

        if let Some(ctx) = config.schema {
            builder = builder.context(ctx);
        }

        // Select dialect and its bundled grammar.
        match dialect_str {
            "postgresql" => {
                builder = builder.dialect(PostgresDialect).grammar(
                    include_str!("../../../grammars/postgresql/PostgreSQLLexer.g4"),
                    include_str!("../../../grammars/postgresql/PostgreSQLParser.g4"),
                );
            }
            "sqlite" => {
                builder = builder.dialect(SqliteDialect);
                // SqliteDialect + default grammar is the builder default, but set dialect explicitly.
            }
            "trino" => {
                builder = builder.dialect(TrinoDialect).grammar(
                    include_str!("../../../grammars/trino/TrinoLexer.g4"),
                    include_str!("../../../grammars/trino/TrinoParser.g4"),
                );
            }
            "mysql" => {
                builder = builder.dialect(MySqlDialect).grammar(
                    include_str!("../../../grammars/mysql/MySQLLexer.g4"),
                    include_str!("../../../grammars/mysql/MySQLParser.g4"),
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
            rng: Mutex::new(make_rng(seed)),
        }))
    })
}

/// Generate one SQL string.
///
/// Concurrency: safe to call from multiple threads on the same handle;
/// serializes on the SqlHandle's internal RNG mutex.
///
/// # Safety
/// `handle` must be from `barkus_sql_compile`. `output_buf` capacity = `*output_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_generate(
    handle: *const SqlHandle,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    ffi_catch_i32(|| unsafe {
        let h = match handle.as_ref() {
            Some(h) => h,
            None => {
                set_last_error("sql generate error: null handle");
                return -1;
            }
        };

        let mut rng = match h.rng.lock() {
            Ok(guard) => guard,
            Err(_) => {
                set_last_error("sql generate error: rng mutex poisoned");
                return -1;
            }
        };
        let (sql, _tape, _map) = match h.gen.generate(&mut *rng) {
            Ok(r) => r,
            Err(e) => {
                set_last_error(&format!("sql generate error: {e}"));
                return -1;
            }
        };

        write_to_buffer(sql.as_bytes(), output_buf, output_len, "sql generate error")
    })
}

/// Generate one SQL string and record the decision tape.
///
/// # Safety
/// Same as `barkus_sql_generate`, plus `tape_buf` capacity = `*tape_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_generate_with_tape(
    handle: *const SqlHandle,
    output_buf: *mut u8,
    output_len: *mut usize,
    tape_buf: *mut u8,
    tape_len: *mut usize,
) -> i32 {
    ffi_catch_i32(|| unsafe {
        let h = match handle.as_ref() {
            Some(h) => h,
            None => {
                set_last_error("sql generate error: null handle");
                return -1;
            }
        };

        let mut rng = match h.rng.lock() {
            Ok(guard) => guard,
            Err(_) => {
                set_last_error("sql generate error: rng mutex poisoned");
                return -1;
            }
        };
        let (sql, tape, _map) = match h.gen.generate(&mut *rng) {
            Ok(r) => r,
            Err(e) => {
                set_last_error(&format!("sql generate error: {e}"));
                return -1;
            }
        };

        write_output_and_tape(
            sql.as_bytes(),
            output_buf,
            output_len,
            &tape.bytes,
            tape_buf,
            tape_len,
            "sql generate error",
        )
    })
}

/// Replay a decision tape to reproduce SQL output.
///
/// Concurrency: safe to call from multiple threads on the same handle;
/// decode reads only the immutable compiled grammar.
///
/// # Safety
/// `handle` from `barkus_sql_compile`. `tape_ptr` has `tape_len` bytes.
/// `output_buf` capacity = `*output_len`.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_decode(
    handle: *const SqlHandle,
    tape_ptr: *const u8,
    tape_len: usize,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32 {
    ffi_catch_i32(|| unsafe {
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
    })
}

/// Free an SQL handle. Safe to call with null.
///
/// # Safety
/// `handle` must be from `barkus_sql_compile`, or null.
#[no_mangle]
pub unsafe extern "C" fn barkus_sql_destroy(handle: *mut SqlHandle) {
    ffi_catch_void(|| {
        if !handle.is_null() {
            unsafe {
                drop(Box::from_raw(handle));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    fn last_error_string() -> Option<String> {
        let ptr = barkus_last_error();
        if ptr.is_null() {
            None
        } else {
            let cstr = unsafe { CStr::from_ptr(ptr) };
            Some(cstr.to_string_lossy().into_owned())
        }
    }

    #[test]
    fn write_to_buffer_empty_output_with_null_dst() {
        let mut dst_len: usize = 0;
        let rc = unsafe { write_to_buffer(&[], ptr::null_mut(), &mut dst_len, "test") };
        assert_eq!(rc, 0);
        assert_eq!(dst_len, 0);

        // Non-empty output with null dst must fail instead of invoking UB.
        let mut dst_len_nonzero: usize = 16;
        let rc = unsafe { write_to_buffer(b"abc", ptr::null_mut(), &mut dst_len_nonzero, "test") };
        assert_eq!(rc, -1);
        assert_eq!(dst_len_nonzero, 16);
        let err = last_error_string().expect("expected error message");
        assert!(err.contains("null output buffer pointer"));

        // Null dst_len must fail safely.
        let mut buf = [0u8; 8];
        let rc = unsafe { write_to_buffer(b"abc", buf.as_mut_ptr(), ptr::null_mut(), "test") };
        assert_eq!(rc, -1);
        let err = last_error_string().expect("expected error message");
        assert!(err.contains("null output length pointer"));
    }

    #[test]
    fn generate_with_tape_too_small_tape_leaves_output_untouched() {
        let src = br#"start = "a" | "b" | "c" ;"#;
        let handle = unsafe { barkus_compile(src.as_ptr(), src.len(), 42, 0) };
        assert!(!handle.is_null());

        let sentinel = [0xAAu8; 16];
        let mut output_buf = sentinel;
        let mut output_len = output_buf.len();
        let mut tape_buf = [0xBBu8; 1];
        let mut tape_len: usize = 0;

        let rc = unsafe {
            barkus_generate_with_tape(
                handle,
                output_buf.as_mut_ptr(),
                &mut output_len,
                tape_buf.as_mut_ptr(),
                &mut tape_len,
            )
        };
        assert_eq!(rc, -1);
        assert_eq!(output_buf, sentinel);
        assert_eq!(output_len, sentinel.len());
        assert_eq!(tape_buf, [0xBBu8; 1]);
        assert_eq!(tape_len, 0);
        let err = last_error_string().expect("expected error message");
        assert!(err.contains("tape buffer too small"));

        unsafe { barkus_destroy(handle) };
    }

    #[test]
    fn compile_with_config_rejects_null_config_with_nonzero_len() {
        let src = br#"start = "hello" ;"#;
        let cfg = ptr::null();
        let handle = unsafe { barkus_compile_with_config(src.as_ptr(), src.len(), cfg, 10, 1) };
        assert!(handle.is_null());
        let err = last_error_string().expect("expected error message");
        assert!(
            err.contains("null config pointer with non-zero length"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn set_last_error_preserves_message_with_interior_nul() {
        set_last_error("before\0after");
        let ptr = barkus_last_error();
        assert!(!ptr.is_null());
        let err = last_error_string().expect("expected non-null error string");
        assert_eq!(err, "before\\0after");
    }

    #[test]
    fn compile_and_generate_catches_panic_and_handles_poisoned_mutex() {
        let src = br#"start = "a" ;"#;
        let handle = unsafe { barkus_compile(src.as_ptr(), src.len(), 1, 0) };
        assert!(!handle.is_null());

        // Force a deterministic panic inside `generate()` (index out of bounds on start production)
        // while holding `h.rng.lock()`, so we exercise both panic catching across `extern "C"`
        // and poisoned-mutex handling on the subsequent call.
        unsafe {
            (*handle).ir.productions.clear();
        }

        let mut buf = [0u8; 64];
        let mut len = buf.len();
        let rc = unsafe { barkus_generate(handle, buf.as_mut_ptr(), &mut len) };
        assert_eq!(rc, -1);
        let err = last_error_string().expect("expected error after caught panic");
        assert!(err.contains("internal panic"), "unexpected error: {err}");

        let mut len2 = buf.len();
        let rc2 = unsafe { barkus_generate(handle, buf.as_mut_ptr(), &mut len2) };
        assert_eq!(rc2, -1);
        let err2 = last_error_string().expect("expected error after poisoned mutex");
        assert!(
            err2.contains("rng mutex poisoned"),
            "unexpected error: {err2}"
        );

        unsafe { barkus_destroy(handle) };

        // Also verify `ffi_catch_ptr` converts panics into null + last_error.
        let null_ptr: *mut Handle = ffi_catch_ptr(|| panic!("compile panic"));
        assert!(null_ptr.is_null());
        let err3 = last_error_string().expect("expected error after ptr panic");
        assert!(err3.contains("compile panic"), "unexpected error: {err3}");
    }
}
