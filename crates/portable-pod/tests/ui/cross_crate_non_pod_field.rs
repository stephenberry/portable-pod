//! A field spelled `$crate::Header` inside another crate's macro, beside that crate's own
//! `$crate::Header` field. The two print identically and are different types, and this one holds
//! a `bool`, so it is not `Pod`. The derive used to merge the two fields by spelling and never
//! bound this one, so `Out` was accepted and `read_pod` built an invalid `bool` from byte `7`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Header {
    pub flag: bool,
}

macro_rules! mine {
    () => {
        cross_crate_fixture::record!(Out { pub body: $crate::Header, });
    };
}
mine!();

fn main() {
    let _ = cross_crate_fixture::read_pod::<Out>(&[1, 7]);
}
