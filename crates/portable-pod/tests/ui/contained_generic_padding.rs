// Regression: `Inner<1>` has padding, but is only ever *contained*, never passed to an entry
// point. Before the proof was made transitive this compiled, and reading `Outer`'s bytes was
// undefined behavior. `Outer` is concrete, so its proof is forced at its definition, and that
// must now force `Inner<1>`'s too.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Inner<const N: usize> {
    a: u8,
    b: [u32; N],
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Outer {
    x: Inner<1>,
}

fn main() {}
