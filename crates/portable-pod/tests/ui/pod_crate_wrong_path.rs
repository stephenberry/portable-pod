//! The attribute must actually re-root *every* reference the derive emits. This names a module
//! that does not export `Pod`, so each emitted path fails to resolve — which is what pins that
//! none of them silently kept the hardcoded `::portable_pod`.
use portable_pod::Pod;

mod empty {}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(crate = crate::empty)]
struct Wire {
    id: u64,
    kind: u64,
}

fn main() {}
