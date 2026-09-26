//! A concrete struct with one field that is not `Pod`, used everywhere a `Pod` type can be. The
//! golden is one error, at the field.
//!
//! Through derive 0.2.0 the impl was bounded `where usize: Pod`, which is an error at the
//! definition and also leaves the impl unusable, so every use below failed its own `Pod` bound as
//! well: one error per use, on top of the one at the field. A concrete type's impl is now
//! unconditional, and each field is proved `Pod` in the impl's body, which reports it once.
use portable_pod::{Pod, assert_layout, bytes_of, read_pod, zeroed};

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Sample {
    id: u64,
    offset: usize,
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Holder {
    one: Sample,
    many: [Sample; 2],
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Wrap<T> {
    inner: T,
}

const _: () = assert_layout::<Sample>();
const ZERO: Sample = zeroed();
const SHAPE: Option<u64> = Sample::SHAPE;

fn main() {
    let s = ZERO;
    let h = Holder { one: s, many: [s; 2] };
    let _ = bytes_of(&s).len() + bytes_of(&h).len() + bytes_of(&Wrap { inner: s }).len();
    let _ = read_pod::<Sample>(bytes_of(&s));
    let _ = read_pod::<[Sample; 3]>(&[0; 48]);
    let _ = (SHAPE, Holder::SHAPE, <Wrap<Sample>>::SHAPE);
}
