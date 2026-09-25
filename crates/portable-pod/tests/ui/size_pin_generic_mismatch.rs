// A size pin on a generic type is an identity over its parameters, checked per instantiation like
// the padding proof. This one is wrong for every `N`: it forgot the `len` field. It is a
// post-monomorphization error, so the instantiation has to reach the crate's API to be checked.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(size = 4 * N)]
struct Ring<const N: usize> {
    slots: [u32; N],
    len: u32,
}

fn main() {
    let ring = Ring::<3> { slots: [0; 3], len: 0 };
    let _bytes = portable_pod::bytes_of(&ring);
}
