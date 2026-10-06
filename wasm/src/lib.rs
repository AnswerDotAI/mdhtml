//! The browser's calls into `mdhtml`, through the plain C ABI, which `mdhtml.js` wraps. Strings cross as UTF-8 in the
//! module's memory: JavaScript writes its argument into a buffer from `alloc`, and reads each result from the pointer and
//! length it gets back, then frees it.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use mdhtml::{Options, parse, render};
use std::ptr::slice_from_raw_parts_mut;

/// A buffer of `len` bytes for JavaScript to write into.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 { Box::into_raw(vec![0u8; len].into_boxed_slice()).cast() }

/// Free a result.
///
/// # Safety
/// `ptr` and `len` must be a result's pointer and length, not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) { drop(unsafe { Box::from_raw(slice_from_raw_parts_mut(ptr, len)) }) }

/// Render Markdown to an MDHTML fragment with the default options. Returns the result's pointer in the high 32 bits and its
/// length in the low 32 bits.
///
/// # Safety
/// `ptr` and `len` must be a buffer from `alloc` holding UTF-8, which this takes and frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md2mdhtml(ptr: *mut u8, len: usize) -> u64 {
    let src = unsafe { Box::from_raw(slice_from_raw_parts_mut(ptr, len)) };
    let html = render(&parse(unsafe { std::str::from_utf8_unchecked(&src) }, &Options::default())).into_bytes().into_boxed_slice();
    let len = html.len() as u64;
    ((Box::into_raw(html).cast::<u8>() as u64) << 32) | len
}
