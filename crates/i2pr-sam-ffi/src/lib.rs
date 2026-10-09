//! Small C ABI over the blocking Rust client.
//!
//! Handles are monotonically allocated integers. All input/output memory belongs to the
//! caller, so Rust allocations and panics never cross the ABI. Each byte input is limited
//! to 64 KiB and must contain valid UTF-8.

#![allow(unsafe_code)]

use i2pr_sam_blocking::BlockingClient;
use std::{
    collections::HashMap,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

const MAX_INPUT: usize = 64 * 1024;
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static CLIENTS: OnceLock<Mutex<HashMap<u64, Arc<BlockingClient>>>> = OnceLock::new();

fn clients() -> &'static Mutex<HashMap<u64, Arc<BlockingClient>>> {
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn client(handle: u64) -> Option<Arc<BlockingClient>> {
    clients().lock().ok()?.get(&handle).cloned()
}

fn guarded(f: impl FnOnce() -> i32) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or(255)
}

unsafe fn input<'a>(ptr: *const u8, len: usize) -> Result<&'a str, i32> {
    if len > MAX_INPUT || (ptr.is_null() && len != 0) {
        return Err(1);
    }
    // SAFETY: non-null/length bounds are checked above; caller promises readable memory.
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    std::str::from_utf8(bytes).map_err(|_| 1)
}

/// Connect to a SAM endpoint (`host:port`). Writes the new opaque handle to `out_handle`.
/// Returns 0 on success, 1 for invalid input, 2 for router/runtime errors, 3 for null output,
/// and 255 if a Rust panic was contained.
///
/// # Safety
/// `endpoint` must reference `len` readable bytes unless `len` is zero, and `out_handle`
/// must point to writable storage for one `u64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn i2pr_sam_connect(
    endpoint: *const u8,
    len: usize,
    out_handle: *mut u64,
) -> i32 {
    guarded(|| {
        if out_handle.is_null() {
            return 3;
        }
        // SAFETY: validated by input; pointer validity is the caller's ABI obligation.
        let endpoint = match unsafe { input(endpoint, len) }
            .and_then(|s| s.parse::<SocketAddr>().map_err(|_| 1))
        {
            Ok(v) => v,
            Err(e) => return e,
        };
        let client = match BlockingClient::connect_endpoint(endpoint) {
            Ok(v) => v,
            Err(_) => return 2,
        };
        let id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        if clients()
            .lock()
            .map(|mut g| g.insert(id, Arc::new(client)))
            .is_err()
        {
            return 2;
        }
        // SAFETY: non-null checked above; caller provides writable storage for one u64.
        unsafe { out_handle.write(id) };
        0
    })
}

/// Resolve a name and copy its UTF-8 value to caller storage. `out_len` receives required
/// bytes; a too-small buffer returns 4 without a partial write.
///
/// # Safety
/// `name` must reference `name_len` readable bytes unless zero; `out_len` must be writable;
/// and `out` must reference `capacity` writable bytes whenever the required output fits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn i2pr_sam_lookup(
    handle: u64,
    name: *const u8,
    name_len: usize,
    out: *mut u8,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guarded(|| {
        if out_len.is_null() {
            return 3;
        }
        // SAFETY: caller supplies readable input storage as required by the ABI.
        let name = match unsafe { input(name, name_len) } {
            Ok(v) => v,
            Err(e) => return e,
        };
        let value = match client(handle).and_then(|c| c.lookup(name).ok()) {
            Some(v) => v,
            None => return 2,
        };
        // SAFETY: caller supplies writable out_len storage.
        unsafe { out_len.write(value.len()) };
        if value.len() > capacity {
            return 4;
        }
        if out.is_null() && !value.is_empty() {
            return 3;
        }
        if !value.is_empty() {
            // SAFETY: capacity check proves enough writable caller-provided storage.
            unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), out, value.len()) };
        }
        0
    })
}

/// Release a client handle. Releasing an unknown/already released handle returns 1.
#[unsafe(no_mangle)]
pub extern "C" fn i2pr_sam_close(handle: u64) -> i32 {
    guarded(|| {
        if clients()
            .lock()
            .ok()
            .and_then(|mut g| g.remove(&handle))
            .is_some()
        {
            0
        } else {
            1
        }
    })
}

/// Generate a Destination. Caller storage receives the public and private tokens; private
/// key material is returned only because this operation explicitly requests it. Both output
/// lengths are always set before a capacity error (4). Invalid output pointers return 3.
///
/// # Safety
/// `public_len` and `secret_len` must be writable. When the function can succeed,
/// `public` and `secret` must each reference writable storage of their respective capacities.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn i2pr_sam_generate_destination(
    handle: u64,
    public: *mut u8,
    public_cap: usize,
    public_len: *mut usize,
    secret: *mut u8,
    secret_cap: usize,
    secret_len: *mut usize,
) -> i32 {
    guarded(|| {
        if public_len.is_null() || secret_len.is_null() {
            return 3;
        }
        let pair = match client(handle).and_then(|c| c.generate_destination(None).ok()) {
            Some(v) => v,
            None => return 2,
        };
        let p = pair.public().as_str().as_bytes();
        let s = pair.secret().expose().as_bytes();
        // SAFETY: caller provides valid length pointers.
        unsafe {
            public_len.write(p.len());
            secret_len.write(s.len());
        }
        if p.len() > public_cap || s.len() > secret_cap {
            return 4;
        }
        if public.is_null() || secret.is_null() {
            return 3;
        }
        // SAFETY: both caller buffers were checked for capacity.
        unsafe {
            std::ptr::copy_nonoverlapping(p.as_ptr(), public, p.len());
            std::ptr::copy_nonoverlapping(s.as_ptr(), secret, s.len());
        }
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn c_abi_connect_lookup_and_release() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("HELLO VERSION"));
            writeln!(reader.get_mut(), "HELLO REPLY RESULT=OK VERSION=3.3").unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert_eq!(line, "NAMING LOOKUP NAME=example.i2p\n");
            writeln!(
                reader.get_mut(),
                "NAMING REPLY RESULT=OK NAME=example.i2p VALUE=peer-destination"
            )
            .unwrap();
        });

        let endpoint = addr.to_string();
        let mut handle = 0;
        // SAFETY: input pointers refer to live byte slices; out_handle is writable.
        assert_eq!(
            unsafe { i2pr_sam_connect(endpoint.as_ptr(), endpoint.len(), &mut handle) },
            0
        );
        let name = b"example.i2p";
        let mut output = [0_u8; 128];
        let mut output_len = 0;
        // SAFETY: all pointers refer to live, correctly sized caller-owned storage.
        assert_eq!(
            unsafe {
                i2pr_sam_lookup(
                    handle,
                    name.as_ptr(),
                    name.len(),
                    output.as_mut_ptr(),
                    output.len(),
                    &mut output_len,
                )
            },
            0
        );
        assert_eq!(&output[..output_len], b"peer-destination");
        assert_eq!(i2pr_sam_close(handle), 0);
        assert_eq!(i2pr_sam_close(handle), 1);
        server.join().unwrap();
    }

    #[test]
    fn invalid_input_and_unknown_handle_are_reported() {
        let mut handle = 0;
        // SAFETY: output pointer is valid; null input with positive length is rejected before read.
        assert_eq!(
            unsafe { i2pr_sam_connect(std::ptr::null(), 1, &mut handle) },
            1
        );
        let name = b"name";
        let mut output_len = 0;
        // SAFETY: input and length pointer are valid; unknown handle fails without output access.
        assert_eq!(
            unsafe {
                i2pr_sam_lookup(
                    u64::MAX,
                    name.as_ptr(),
                    name.len(),
                    std::ptr::null_mut(),
                    0,
                    &mut output_len,
                )
            },
            2
        );
    }
}
