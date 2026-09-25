//! With `#[pod(crate = ...)]`, a field that is not `Pod` is still reported at the field. The path's
//! tokens are relocated there with `Span::located_at`, which keeps the hygiene `$crate` resolves
//! through (see `rooted_at` in the derive, and `cross_crate_non_pod_field.rs`).
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
