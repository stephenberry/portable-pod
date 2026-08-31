//! `#[pod(crate = ...)]` takes a path, not a string. `serde` and `bytemuck` spell their equivalent
//! with quotes, so this is the mistake a user arriving from either will make; the error names the
//! exact replacement rather than just rejecting the literal.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(crate = "::portable_pod")]
struct Wire {
    id: u64,
}

fn main() {}
