#![allow(private_interfaces)]

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use barkus_core::generate::{decode, generate};
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use rand::rngs::SmallRng;
use rand::SeedableRng;

struct Handle {
    ir: GrammarIr,
    profile: Profile,
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

    let rng = if seed == 0 {
        SmallRng::from_entropy()
    } else {
        SmallRng::seed_from_u64(seed)
    };

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
    let cap = *output_len;
    if serialized.len() > cap {
        set_last_error("generate error: output buffer too small");
        return -1;
    }

    ptr::copy_nonoverlapping(serialized.as_ptr(), output_buf, serialized.len());
    *output_len = serialized.len();
    0
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
    let out_cap = *output_len;
    if serialized.len() > out_cap {
        set_last_error("generate error: output buffer too small");
        return -1;
    }

    let tape_bytes = &tape.bytes;
    let tape_cap = *tape_len;
    if tape_bytes.len() > tape_cap {
        set_last_error("generate error: tape buffer too small");
        return -1;
    }

    ptr::copy_nonoverlapping(serialized.as_ptr(), output_buf, serialized.len());
    *output_len = serialized.len();

    ptr::copy_nonoverlapping(tape_bytes.as_ptr(), tape_buf, tape_bytes.len());
    *tape_len = tape_bytes.len();

    0
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
    let cap = *output_len;
    if serialized.len() > cap {
        set_last_error("decode error: output buffer too small");
        return -1;
    }

    ptr::copy_nonoverlapping(serialized.as_ptr(), output_buf, serialized.len());
    *output_len = serialized.len();
    0
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
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(ptr::null(), |cs| cs.as_ptr())
    })
}
