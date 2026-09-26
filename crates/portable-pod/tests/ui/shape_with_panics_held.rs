//! A `shape_with` that panics when evaluated, on a type held by another derived concrete struct.
//! Nothing reads either shape, but `Holder`'s `SHAPE` names `Stored`'s, and rustc evaluates the
//! constants a concrete body names at its definition. So this fails, exactly as it did through
//! derive 0.2.0; `lazy_shape` in `tests/derive.rs` is the case that must build: `Stored` alone.
use portable_pod::Pod;

const fn unfinished_variant_table() -> u64 {
    panic!("not written yet")
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(shape_with = unfinished_variant_table())]
struct Stored {
    a: u32,
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Holder {
    inner: Stored,
}

fn main() {
    let _ = portable_pod::bytes_of(&Holder { inner: Stored { a: 1 } });
}
