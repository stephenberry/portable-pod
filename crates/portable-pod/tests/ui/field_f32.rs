// Floats are excluded deliberately: NaN payloads are not stable across targets.
use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Point {
    x: f32,
    y: f32,
}
fn main() {}
