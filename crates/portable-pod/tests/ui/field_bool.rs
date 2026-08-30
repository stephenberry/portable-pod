// `bool` is valid only for 0 and 1, so it is not any-bit-pattern. Use `portable_pod::Bit`.
use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Flags {
    on: bool,
    pad: [u8; 3],
    count: u32,
}
fn main() {}
