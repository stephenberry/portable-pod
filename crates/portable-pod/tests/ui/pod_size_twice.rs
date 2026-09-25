//! Two size pins cannot both be meant. Across separate `#[pod]` attributes too.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(size = 8)]
#[pod(size = 16)]
struct Wire {
    id: u64,
}

fn main() {}
