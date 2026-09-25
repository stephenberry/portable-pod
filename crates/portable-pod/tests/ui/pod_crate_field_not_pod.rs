//! With `#[pod(crate = ...)]`, a field that is not `Pod` is reported on a span running from the
//! path to the field, rather than on the field alone. The path keeps its own spans because
//! `$crate` resolves through them (see `rooted_at` in the derive, and
//! `cross_crate_non_pod_field.rs`).
mod reexport {
    pub use portable_pod::Pod;
}

#[derive(Clone, Copy, reexport::Pod)]
#[repr(C)]
#[pod(crate = crate::reexport)]
struct Span {
    start: u32,
    offset: usize,
}

fn main() {}
