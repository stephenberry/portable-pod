// A padded instantiation built by `zeroed` in a `const` item. `zeroed` is a `const fn` that forces
// the layout proof, so the padded `Ring<3>` fails the build at the item, under `cargo check` too.
// `Ring<4>` is padding-free and the same spelling must accept it.
use portable_pod::{Pod, zeroed};

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Ring<const N: usize> {
    slots: [u32; N],
    tag: u64,
}

const FINE: Ring<4> = zeroed();
const PADDED: Ring<3> = zeroed();

fn main() {
    let _ = (FINE.tag, PADDED.tag);
}
