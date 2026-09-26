//! A padded concrete struct, used everywhere a `Pod` type can be, still fails: the impl is
//! unconditional, and the layout proof forced at the definition is what refuses it. The golden is
//! that one failure.
use portable_pod::{Pod, bytes_of, read_pod, zeroed};

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Padded {
    a: u8,
    b: u32,
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Holder {
    many: [Padded; 2],
}

fn main() {
    let p: Padded = zeroed();
    let _ = bytes_of(&p).len() + bytes_of(&Holder { many: [p; 2] }).len();
    let _ = read_pod::<Padded>(&[0; 8]);
}
