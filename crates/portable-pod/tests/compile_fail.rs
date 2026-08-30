//! The compile-fail suite.
//!
//! A derive that accepts an unsound type is worse than a hand-written `unsafe impl`, because it
//! launders a bad assertion through machinery that looks authoritative. These fixtures are the
//! specification: each one must fail to compile, with a message a stranger can act on.
//!
//! **A padded *generic* type is now in this suite**, which under `trybuild` it could not be. Its
//! layout proof is an associated const, so the failure is a post-monomorphization error that only
//! appears at codegen, and a harness that type-checks its fixtures never reaches it.
//! `nocompile` drives `cargo build`, so `padded_generic_instantiation.rs` fails the way a user's
//! build would. `layout_proofs_are_post_monomorphization` in `tests/derive.rs` covers the positive
//! direction across many instantiations; the crate docs section "When the check fires" describes
//! the limitation that remains, which is about *which* instantiations get proved, not about
//! whether the proof can be observed failing.
//!
//! The suite runs in `Mode::Exact`, the default, and deliberately. `Brief` compares each
//! diagnostic's code, message and location and drops the rest, which is the right trade for most
//! crates — but here the rendering *is* the product. The `#[diagnostic::on_unimplemented]` notes
//! that tell a user to reach for `Bit` instead of `bool`, or `u32` instead of `usize`, are emitted
//! as `= note:` lines, which `Brief` discards; and the derive respans its generated bound onto the
//! field's own type so the error lands on the offending field rather than on the derive attribute
//! several lines above. Both are deliberate work that only `Exact` regression-tests. The cost is
//! re-blessing when rustc reflows a diagnostic, which is the loud failure rather than the silent
//! one.

#[test]
fn ui() {
    let mut t = nocompile::cases!();
    t.dependency_path("portable-pod", ".");
    t.compile_fail_dir("tests/ui");
    t.assert();
}
