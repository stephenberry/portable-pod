//! The only `#[pod]` argument is `crate`. Anything else is rejected by name.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(krate = ::portable_pod)]
struct Wire {
    id: u64,
}

fn main() {}
