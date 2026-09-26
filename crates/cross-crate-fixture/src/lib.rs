//! A second crate, for `portable-pod`'s tests. Not published.
//!
//! `record!` derives `Pod` on a struct whose first field is `$crate::Header`, this crate's
//! `Header`. A caller that passes a field spelled `$crate::Header` of its own hands the derive two
//! fields whose types print identically and are different types. The derive used to merge fields
//! by spelling, so the caller's field was never bounded `Pod` and never shaped: a non-`Pod` field
//! was accepted in a `Pod` struct. `tests/cross_crate.rs` and `tests/ui/cross_crate_non_pod_field.rs`
//! in `portable-pod` are the regression tests.
//!
//! `wrap!` declares a `#[pod(transparent)]` newtype over a type its caller names, so a caller's
//! `$crate::Header` must be the type whose shape the newtype forwards.

pub use portable_pod::{Pod, read_pod};
// Under its own name: whether `portable_pod::Pod` also brings the derive depends on features that
// other crates in the build may turn on.
pub use portable_pod_derive::Pod as DerivePod;

#[derive(Clone, Copy, DerivePod)]
#[repr(C)]
pub struct Header {
    pub id: u8,
}

/// `record!(Name { more fields, })` declares `Name` with `head: $crate::Header` first.
#[macro_export]
macro_rules! record {
    ($n:ident { $($r:tt)* }) => {
        #[derive(Clone, Copy, $crate::DerivePod)]
        #[repr(C)]
        #[pod(crate = $crate)]
        pub struct $n {
            pub head: $crate::Header,
            $($r)*
        }
    };
}

/// A transparent newtype declared here: its shape is `u32`'s.
#[derive(Clone, Copy, DerivePod)]
#[repr(transparent)]
#[pod(transparent)]
pub struct Id(pub u32);

/// `wrap!(Name(Type))` declares `Name` as a `#[pod(transparent)]` newtype over `Type`.
#[macro_export]
macro_rules! wrap {
    ($n:ident($t:ty)) => {
        #[derive(Clone, Copy, $crate::DerivePod)]
        #[repr(transparent)]
        #[pod(crate = $crate, transparent)]
        pub struct $n(pub $t);
    };
}
