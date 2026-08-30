// Regression: wrapping a padded generic in an array used to erase its proof, because the blanket
// `impl<T: Pod, const N: usize> Pod for [T; N]` took the default empty `__LAYOUT_OK`.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Inner<const N: usize> {
    a: u8,
    b: [u32; N],
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Holder {
    xs: [Inner<1>; 2],
}

fn main() {}
