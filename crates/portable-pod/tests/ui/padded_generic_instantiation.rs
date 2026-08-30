// A padded *instantiation* of a generic type. This fixture could not exist under a harness that
// type-checks its fixtures: `Ring<3>`'s layout proof is an associated const, so it is a
// post-monomorphization error that only appears at codegen. `nocompile` drives `cargo build`, so
// this fails here exactly as it would in a user's build.
//
// Note what is being pinned. `Ring<4>` is padding-free and must stay accepted; only `Ring<3>` has
// a gap, and the proof is per-instantiation rather than per-type. A test that merely asserted
// "Ring is rejected" would be asserting the wrong thing.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Ring<const N: usize> {
    slots: [u32; N],
    tag: u64,
}

fn main() {
    let _ok = Ring::<4> { slots: [0; 4], tag: 0 };
    let bad = Ring::<3> { slots: [0; 3], tag: 0 };
    let _bytes = portable_pod::bytes_of(&bad);
}
