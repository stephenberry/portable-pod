//! A transparent newtype declared by another crate's macro, over a type this crate names as
//! `$crate::Header` and that is not `Pod`. The field's type must resolve here, not in the macro's
//! crate (whose `Header` is `Pod`), so this fails, once, at the field.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Header {
    pub flag: bool,
}

macro_rules! mine {
    () => {
        cross_crate_fixture::wrap!(Wrapped($crate::Header));
    };
}
mine!();

fn main() {
    let _ = cross_crate_fixture::read_pod::<Wrapped>(&[7]);
}
