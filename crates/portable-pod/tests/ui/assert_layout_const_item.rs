// A padded instantiation named only by `assert_layout` in a `const` item. Nothing else reaches
// `Ring<3>`, so this is the check a user writes deliberately; being a `const` item, it fails at
// the item itself and under `cargo check` too, not only when some function is monomorphized.
// `Ring<4>` is padding-free and the same spelling must accept it.
use portable_pod::{Pod, assert_layout};

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Ring<const N: usize> {
    slots: [u32; N],
    tag: u64,
}

const _: () = assert_layout::<Ring<4>>();
const _: () = assert_layout::<Ring<3>>();

fn main() {}
