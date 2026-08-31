//! Derive macro for [`portable_pod::Pod`](https://docs.rs/portable-pod).
//!
//! You do not use this crate directly; enable the `derive` feature of `portable-pod` (on by
//! default) and use `portable_pod::Pod`.
//!
//! # No dependencies
//!
//! This crate depends on nothing. `proc_macro` is a sysroot crate, like `core` and `alloc`, so
//! `portable-pod` and its derive together pull in zero third-party code — which a crate whose
//! entire claim is "your bytes depend on nothing" ought to be able to say without a footnote.
//!
//! `proc-macro2` and `quote` were used at first and are not needed here. `proc-macro2` exists
//! mainly so expansion logic can run *outside* a proc-macro invocation, where `proc_macro`'s
//! types panic; that buys unit tests of `expand`, which this crate does not have — it is tested
//! through the compiled macro, by `portable-pod`'s `tests/derive.rs` and its compile-fail fixtures,
//! which exercise the real thing rather than a stand-in. `quote` is quasi-quoting sugar, replaced
//! here by lexing ordinary format strings.
//!
//! The one API difference worth knowing if you edit `parse.rs`: `proc_macro::Ident` does not
//! implement `PartialEq`, so keyword tests go through `parse::is`.

use proc_macro::{Delimiter, Group, Literal, Span, TokenStream, TokenTree};

mod parse;

/// Lex a fragment of generated code.
///
/// The argument is always a literal written in this file, never anything a user supplied, so a
/// lex failure is a bug here rather than something a caller can provoke.
fn lex(src: &str) -> TokenStream {
    src.parse().expect("generated fragment failed to lex")
}

/// Move every token of a generated fragment onto `span`, so a diagnostic about it points at the
/// user's code rather than at the derive attribute.
///
/// This is `quote_spanned!`'s job. Tokens that came from the user are concatenated separately and
/// keep the spans they arrived with; only this crate's own fragments are moved.
fn respan(ts: TokenStream, span: Span) -> TokenStream {
    ts.into_iter()
        .map(|t| match t {
            TokenTree::Group(g) => {
                let mut regrouped = Group::new(g.delimiter(), respan(g.stream(), span));
                regrouped.set_span(span);
                TokenTree::Group(regrouped)
            }
            mut leaf => {
                leaf.set_span(span);
                leaf
            }
        })
        .collect()
}

/// Wrap a stream in a delimiter.
fn delimit(delimiter: Delimiter, ts: TokenStream) -> TokenStream {
    TokenStream::from(TokenTree::Group(Group::new(delimiter, ts)))
}

/// `<prefix><crate_path><suffix>`, the way every reference to the trait is built.
///
/// The path is `::portable_pod` unless `#[pod(crate = ...)]` said otherwise. Nothing in the
/// expansion may name `::portable_pod` directly: a crate that re-exports `Pod` sets this, and a
/// single hardcoded mention would break the whole point (see the crate docs).
///
/// The caller respans the whole result onto the offending field where that matters, which does
/// move the user's path tokens too — a deliberate exception to `respan`'s note above. Splicing the
/// path in un-respanned was tried and is worse: rustc then attributes an unsatisfied bound to the
/// derive attribute instead of to the field, which is the diagnostic this crate works hardest to
/// get right (`tests/ui/field_usize.stderr` is the regression test, and it moved). The cost is
/// that a typo *inside* the path is reported at the field rather than at the typo, which is the
/// rarer mistake by far.
fn rooted(prefix: &str, crate_path: &TokenStream, suffix: &str) -> TokenStream {
    let mut ts = lex(prefix);
    ts.extend(crate_path.clone());
    ts.extend(lex(suffix));
    ts
}

/// Derive [`Pod`], proving the contract at compile time.
///
/// # What it checks
///
/// * The item is a struct. Enums and unions are rejected with an explanation.
/// * It carries `#[repr(C)]`, `#[repr(transparent)]`, or `#[repr(C, align(N))]`.
///   `#[repr(packed)]` and the default `repr(Rust)` are rejected.
/// * **No padding, internal or tail**: `size_of::<Self>()` equals the sum of the field sizes.
///   Under `repr(C)` that single equation is a complete proof, because every alignment
///   rounding can only increase the size, so equality means no rounding occurred anywhere.
///   The error lists the fields in declaration order and states the placement rule; it does
///   not name the individual gap, which is a deliberate compile-time trade (see DESIGN.md
///   §5.1 — per-field `offset_of!` checks cost ~85% of this derive's total time).
/// * **Every field is `Pod`**, via a generated where-clause bound. This discharges the
///   any-bit-pattern and position-independence clauses by induction, and is why a `bool`,
///   `usize`, or `f32` field fails to compile.
///
/// For a generic type the layout proof is an associated const, so it is checked **per
/// instantiation**: `Ring<3>` and `Ring<7>` are proved separately, and neither has to be named
/// in a test. Each of its **type** parameters is additionally bound `Copy`, which is the
/// `Pod: Copy` supertrait obligation and nothing more — the struct itself does not have to
/// declare `Copy` on its parameters, and most do not, preferring to put bounds on their impls.
///
/// # Using the derive through a re-export
///
/// By default the expansion names `::portable_pod::Pod`, which resolves only in a crate that
/// depends on `portable-pod` directly under that name. A library that re-exports the trait must
/// say where it lives with `#[pod(crate = ::my_engine::mem)]`, or its users get
/// ``cannot find `portable_pod` in the crate root``. The value is a path, not a string, and only
/// the `Pod` trait has to be reachable there. See the crate docs for a worked example.
///
/// # Padding must be eliminated, not excused
///
/// There is no opt-out. An earlier version of this crate offered
/// `#[pod(tail_padding_is_zero)]` for types whose trailing alignment bytes were always zero by
/// construction; it was removed because it could not be used soundly. Returning a value by
/// value is a *typed copy*, and a typed copy leaves padding bytes uninitialized regardless of
/// what was there before — so `zeroed()`, `read_pod()`, and every struct literal produced a
/// value whose padding was uninitialized, and reading it was undefined behavior.
///
/// The fix is one line, and it keeps all four clauses machine-checked:
///
/// ```
/// # use portable_pod::Pod;
/// #[derive(Clone, Copy, Pod)]
/// #[repr(C)]
/// struct Table {
///     keys: [u64; 8],
///     len: u32,
///     _pad: u32, // <- fills the alignment gap, so there is no padding to reason about
/// }
/// ```
#[proc_macro_derive(Pod, attributes(pod))]
pub fn derive_pod(input: TokenStream) -> TokenStream {
    let parsed = match parse::parse(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };
    expand(&parsed)
}

fn expand(input: &parse::Input) -> TokenStream {
    let name = &input.name;
    let decl = &input.generics_decl;
    let uses = &input.generics_use;
    // `#[pod(crate = ...)]`, or this crate. Defaulted here rather than in `parse`, beside the
    // four sites that consume it.
    let root = &input
        .crate_path
        .clone()
        .unwrap_or_else(|| lex("::portable_pod"));

    // Bound every field type. Field types, not generic parameters: an unsatisfied concrete bound
    // (`where bool: Pod`) is a hard error, and the same clause covers generic field types, so the
    // derive never has to reason about type parameters. Deduplicated so a struct with four `u32`
    // fields does not emit four identical predicates.
    let mut seen = Vec::<String>::new();
    let mut bounds = TokenStream::new();
    let mut inherit = TokenStream::new();
    for f in &input.fields {
        let key = f.ty.to_string();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        // Respan our own tokens onto the field's type so `usize: Pod` is reported at the field,
        // not at the `Pod` in the derive attribute several lines above it. The type's own tokens
        // are extended in verbatim and keep the spans they came with.
        let at =
            f.ty.clone()
                .into_iter()
                .next()
                .map_or_else(Span::call_site, |t| t.span());
        bounds.extend(f.ty.clone());
        bounds.extend(respan(rooted(":", root, "::Pod,"), at));
        // Force each field's own layout proof, making the proof transitive.
        //
        // This line is load-bearing and its absence was unsound. A field type's proof is only
        // evaluated when the const is *named*, and `size_of::<Inner<1>>()` names `size_of`, not
        // `<Inner<1> as Pod>::__LAYOUT_OK`. So a padded generic was accepted whenever it was
        // merely *contained* in another type rather than reaching an entry point itself, while
        // the containing type's own checks passed vacuously (one field, offset 0, sizes equal).
        // The `Pod` bound above is not a substitute: it proves `Inner<1>: Pod`, not that
        // `Inner<1>`'s layout was ever checked.
        inherit.extend(respan(lex("let _: () = <"), at));
        inherit.extend(f.ty.clone());
        inherit.extend(respan(rooted("as", root, "::Pod>::__LAYOUT_OK;"), at));
    }

    // `Pod: Copy`, so the impl must prove `Self: Copy` -- and for a generic type with a *derived*
    // `Copy` (`impl<K: Copy, V: Copy> Copy for Table<K, V>`) nothing above supplies it. Bounding
    // the field types does not reach it: `[K; CAP]: Pod` does not let the solver conclude
    // `K: Copy`, because that would mean reasoning backwards through the blanket
    // `impl<T: Pod, const N: usize> Pod for [T; N]`. Without this predicate the derive failed on
    // any generic struct that did not already declare `Copy` on its own parameters, which is most
    // of them -- bounds belong on impls, not on struct definitions.
    //
    // The obligation is spelled as itself rather than as `T: Copy` on each type parameter. That
    // per-parameter form is *sufficient* but not *necessary*, so it rejects types this crate has
    // no business rejecting: a parameter reachable only through an associated-type projection,
    // carrying a hand-written `Copy` impl, need not be `Copy` for the struct to be. `Self: Copy`
    // is exactly the supertrait bound and admits every one of those. It also keeps the rule in
    // §5 -- this derive never reasons about type parameters -- rather than making an exception
    // to it, and it needs no case analysis over lifetimes and const parameters, which cannot
    // carry the bound at all.
    //
    // Only for a generic type. A concrete one proves `Self: Copy` directly at the impl, which is
    // a better diagnostic than deferring it to a use site (see `tests/ui/field_not_copy.rs`).
    let copy_bound = if input.is_concrete {
        TokenStream::new()
    } else {
        lex("Self: ::core::marker::Copy,")
    };

    let existing = &input.where_predicates;
    let mut where_clause = TokenStream::new();
    if !existing.is_empty() || !bounds.is_empty() || !copy_bound.is_empty() {
        where_clause.extend(lex("where"));
        if !existing.is_empty() {
            where_clause.extend(existing.clone());
            where_clause.extend(lex(","));
        }
        where_clause.extend(copy_bound);
        where_clause.extend(bounds);
    }

    // Padding, internal and tail alike: `size_of::<Self>()` must equal the sum of the field sizes.
    //
    // This one equation is a complete proof. `repr(C)` places each field at the next offset that is
    // a multiple of its alignment and rounds the total up to the struct's own alignment; every one
    // of those roundings can only *increase* the size, so the total equals the sum exactly when no
    // rounding did anything, which is exactly when there is no padding anywhere. Internal padding
    // cannot hide behind a compensating shortfall elsewhere, because there is no shortfall to be
    // had. (`repr(transparent)` is the degenerate case: one non-ZST field, and ZSTs contribute
    // zero to both sides. `repr(C, align(N))` that over-aligns correctly fails, since the raised
    // alignment is tail padding.)
    //
    // It is also one assert and N terms. Earlier versions emitted a per-field `offset_of!` check as
    // well, so the error could name the field the gap precedes. Measured, those checks *were* the
    // derive at the use site — not the macro expansion, which is about a quarter of the cost, but
    // the code it emits, which rustc must type-check and const-evaluate. Over 200 derived structs
    // (`cargo check`, delta against the same structs without `Pod`):
    //
    //     fields/struct     with per-field checks     this form
    //         4                   +0.11s               +0.03s
    //         8                   +0.20s               +0.03s
    //        16                   +0.39s               +0.06s
    //        48                   +1.11s               +0.08s
    //
    // This form is essentially flat in field count; the per-field checks were ~85% of the total at
    // every size. Guarding them behind this cheaper check recovers nothing (measured), because the
    // cost is type-checking and MIR construction, which happen whether or not const-evaluation
    // reaches them. The diagnostic is therefore paid for statically instead: the message names the
    // fields in declaration order and states the `repr(C)` rule, which is the information a reader
    // needs to locate the gap, and which is also available when the struct was itself generated by
    // another macro and is not in the source at all. See DESIGN.md §5.1.
    let mut total = lex("0usize");
    for f in &input.fields {
        total.extend(lex("+ ::core::mem::size_of::<"));
        total.extend(f.ty.clone());
        total.extend(lex(">()"));
    }

    let listing = if input.fields.is_empty() {
        String::from("It has no fields")
    } else {
        let mut s = String::from("Fields in declaration order: ");
        for (i, f) in input.fields.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("{}: {}", f.label, f.ty));
        }
        s
    };
    let msg = format!(
        "`{name}` has padding, so it cannot be `Pod`: `size_of::<{name}>()` exceeds the sum of its \
         field sizes, and reading a padding byte observes uninitialized memory. Under `repr(C)` \
         each field goes at the next offset that is a multiple of its alignment, and the size is \
         rounded up to the struct's alignment, so a gap sits before any field more aligned than \
         the offset it would otherwise take, and after the last field. {listing}. Reorder them \
         widest-first, or insert explicit zeroed padding fields. Always-zero padding is not an \
         escape: a typed copy leaves padding uninitialized however the value was built."
    );

    let mut assert_args = lex("::core::mem::size_of::<Self>() ==");
    assert_args.extend(total);
    assert_args.extend(lex(","));
    assert_args.extend(TokenStream::from(TokenTree::Literal(Literal::string(&msg))));
    let mut checks = lex("::core::assert!");
    checks.extend(delimit(Delimiter::Parenthesis, assert_args));
    checks.extend(lex(";"));

    // `const __LAYOUT_OK: () = { <inherit> <checks> };`
    let mut proof = inherit;
    proof.extend(checks);
    let mut body = lex("#[allow(clippy::let_unit_value)] const __LAYOUT_OK: () =");
    body.extend(delimit(Delimiter::Brace, proof));
    body.extend(lex(";"));

    let mut out = lex("#[automatically_derived] unsafe impl");
    if !decl.is_empty() {
        out.extend(lex("<"));
        out.extend(decl.clone());
        out.extend(lex(">"));
    }
    out.extend(rooted("", root, "::Pod for"));
    out.extend(TokenStream::from(TokenTree::Ident(name.clone())));
    if !uses.is_empty() {
        out.extend(lex("<"));
        out.extend(uses.clone());
        out.extend(lex(">"));
    }
    out.extend(where_clause);
    out.extend(delimit(Delimiter::Brace, body));

    // A concrete type's proof is forced here, so it holds whether or not anyone uses the type.
    // A generic type's cannot be: there is no way to name every instantiation, so its proof
    // fires when an instantiation reaches one of the crate's entry points.
    if input.is_concrete {
        out.extend(lex("const _: () = <"));
        out.extend(TokenStream::from(TokenTree::Ident(name.clone())));
        out.extend(rooted("as", root, "::Pod>::__LAYOUT_OK;"));
    }

    out
}
