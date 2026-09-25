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

/// The span of a field type's first token: where a diagnostic about that field belongs.
fn field_span(ty: &TokenStream) -> Span {
    ty.clone()
        .into_iter()
        .next()
        .map_or_else(Span::call_site, |t| t.span())
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
/// Where a reference should point at a field, use `rooted_at`, which moves this crate's own tokens
/// there and leaves a user's path where it was.
fn rooted(prefix: &str, crate_path: &TokenStream, suffix: &str) -> TokenStream {
    let mut ts = lex(prefix);
    ts.extend(crate_path.clone());
    ts.extend(lex(suffix));
    ts
}

/// `rooted`, moved onto `span`: the reference to the trait that a diagnostic about one field
/// should point at.
///
/// Only this crate's own tokens move. A path the user gave in `#[pod(crate = ...)]` keeps the
/// spans it arrived with, because a span is not only a location: `$crate` resolves through it.
/// Moving `$crate` onto a field that another crate's macro wrote makes it name *that* crate, so
/// the bound checks whatever `Pod` that crate happens to export, or fails to resolve
/// (`tests/ui/cross_crate_non_pod_field.rs` pins it; every release through 0.1.4 moved the path).
/// The default `::portable_pod` is this crate's own token, so it moves, and that is what puts an
/// unsatisfied bound at the field (`tests/ui/field_usize.stderr`). A user path left in place makes
/// rustc draw the span from the path to the field instead
/// (`tests/ui/pod_crate_field_not_pod.stderr`), which is the price of `crate = ...` being correct.
fn rooted_at(
    prefix: &str,
    crate_path: Option<&TokenStream>,
    suffix: &str,
    span: Span,
) -> TokenStream {
    match crate_path {
        None => respan(rooted(prefix, &lex("::portable_pod"), suffix), span),
        Some(path) => {
            let mut ts = respan(lex(prefix), span);
            ts.extend(path.clone());
            ts.extend(respan(lex(suffix), span));
            ts
        }
    }
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
/// # What it emits besides the proof
///
/// `Pod::SHAPE`, a 64-bit hash of the field structure for a format header to carry: each field's
/// name and shape in declaration order, each const generic parameter's value, and the size. The
/// type's own name is not included. `portable_pod::shape` documents the algorithm exactly, and it
/// is frozen: the value changes only in a semver-major release of `portable-pod`.
///
/// # Using the derive through a re-export
///
/// By default the expansion names `::portable_pod::Pod`, which resolves only in a crate that
/// depends on `portable-pod` directly under that name. A library that re-exports the trait must
/// say where it lives with `#[pod(crate = ::my_engine::mem)]`, or its users get
/// ``cannot find `portable_pod` in the crate root``. The value is a path, not a string, and only
/// the `Pod` trait has to be reachable there. See the crate docs for a worked example.
///
/// # Pinning the size and alignment
///
/// `#[pod(size = <expr>)]` and `#[pod(align = <expr>)]` state the layout outright, so a change that
/// keeps the type padding-free but moves its bytes (a field added, widened, or reordered across an
/// alignment boundary) fails the build instead of silently changing every checksum and file built
/// on it. Each value is a `usize` constant expression; on a generic type it may name the type's
/// parameters and is checked per instantiation. They combine with `crate` in one attribute or
/// several. See the crate docs, "Pinning the layout".
///
/// # Extending the shape
///
/// `#[pod(shape_with = <expr>)]` folds one more `u64` constant expression into `Pod::SHAPE`, for
/// meaning the fields cannot express, such as the variant table of an enum whose discriminant a
/// wrapper stores as an integer. On a generic type it may name the type's parameters.
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
    let user_root = input.crate_path.as_ref();

    // Bound every field type. Field types, not generic parameters: an unsatisfied concrete bound
    // (`where bool: Pod`) is a hard error, and the same clause covers generic field types, so the
    // derive never has to reason about type parameters.
    //
    // One bound and one inherited proof per *field*, never merged across fields whose types are
    // spelled alike. Two spellings that print the same can name different types: `$crate::Header`
    // prints identically whichever crate's macro produced it, so two fields written that way by
    // two crates' macros are two types, and merging them left the second unbounded -- a non-`Pod`
    // field in a `Pod` struct, unsound in every release through 0.1.4. No spelling-based merge can
    // be proved safe: even without `$crate`, a `macro` (macros 2.0, nightly) resolves a plain
    // `Header` at its definition site, so identical tokens with no marker at all can still name
    // two types. The merge was a compile-time saving and it is given up knowingly: DESIGN.md §12
    // has the cost. `tests/cross_crate.rs` and `tests/ui/cross_crate_non_pod_field.rs` are the
    // regression tests.
    let mut bounds = TokenStream::new();
    let mut inherit = TokenStream::new();
    for f in &input.fields {
        // Respan our own tokens onto the field's type so `usize: Pod` is reported at the field,
        // not at the `Pod` in the derive attribute several lines above it. The type's own tokens
        // are extended in verbatim and keep the spans they came with.
        let at = field_span(&f.ty);
        bounds.extend(f.ty.clone());
        bounds.extend(rooted_at(":", user_root, "::Pod,", at));
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
        inherit.extend(rooted_at("as", user_root, "::Pod>::__LAYOUT_OK;", at));
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

    // The message goes in as the argument to `"{}"`, never as the format string itself. It embeds
    // each field's type as written, and a type can contain braces (`Inline<{ K }>`) that a format
    // string would read as a placeholder, failing the expansion even for a struct with no padding.
    // One argument to `"{}"` is also the form `core::panic!` special-cases for const evaluation.
    // Tested by `braced_field_types` in `tests/derive.rs` and by
    // `tests/ui/internal_padding_braced_type.rs`.
    let mut assert_args = lex("::core::mem::size_of::<Self>() ==");
    assert_args.extend(total);
    assert_args.extend(lex(r#", "{}","#));
    assert_args.extend(TokenStream::from(TokenTree::Literal(Literal::string(&msg))));
    let mut checks = lex("::core::assert!");
    checks.extend(delimit(Delimiter::Parenthesis, assert_args));
    checks.extend(lex(";"));

    if let Some(pin) = &input.size {
        checks.extend(pin_check(input, Pinned::Size, pin));
    }
    if let Some(pin) = &input.align {
        checks.extend(pin_check(input, Pinned::Align, pin));
    }

    // `const __LAYOUT_OK: () = { <inherit> <checks> };`
    let mut proof = inherit;
    proof.extend(checks);
    let mut body = lex("#[allow(clippy::let_unit_value)] const __LAYOUT_OK: () =");
    body.extend(delimit(Delimiter::Brace, proof));
    body.extend(lex(";"));
    body.extend(shape(input, root));

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

/// `const SHAPE`: the fields' names and shapes in declaration order, then each const parameter's
/// value, then any `#[pod(shape_with = ...)]`, then the size. The algorithm and what it leaves out
/// are documented on `portable_pod::shape`, and DESIGN.md §12 has the reasoning.
///
/// ```text
/// const SHAPE: Option<u64> = <() as Pod>::__SHAPE_FOLD
///     .fields(&[("a", <u32 as Pod>::SHAPE), ("b", <[u8; 4] as Pod>::SHAPE)])
///     .param(N as u128)
///     .with(<shape_with>)
///     .finish(size_of::<Self>());
/// ```
///
/// (Every path in the real expansion is absolute, `::core::primitive::u64` and so on, so a
/// `type u64 = u32;` in the user's scope changes nothing; `shadowed_names` in `tests/shape.rs`.)
///
/// Three decisions are in that shape:
///
/// * **The fold is reached through the trait.** `<() as Pod>::__SHAPE_FOLD` is a
///   `portable_pod::shape::Fold`, and method calls on it resolve without naming that type's path,
///   so the expansion still names nothing but `<root>::Pod`, which is all `#[pod(crate = ...)]`
///   asks a re-exporting crate to provide.
/// * **One call for all the fields.** The cost of this const is type-checking it, at every
///   derive, whether or not anything reads it; evaluating it is noise by comparison. A `.field(..)`
///   method call per field cost measurably more than one `.fields(&[..])` call (DESIGN.md §12).
///   Each field has its own projection, never one shared by fields spelled alike: see the bounds
///   in `expand` for why that sharing was unsound.
/// * **No assertion, and no forced layout proof.** A const panic site costs compile time at every
///   derive (§5.1). Forcing `__LAYOUT_OK` here would be redundant, since a shape is only useful
///   beside bytes an entry point produced and every entry point forces it, and it would add a
///   second "erroneous constant" trail to every padding diagnostic.
///
/// Each projection is respanned onto its field, where the field bound and the transitive proof in
/// `expand` also land, so rustc reports a field type that is not `Pod` once per field.
fn shape(input: &parse::Input, root: &TokenStream) -> TokenStream {
    // One `(name, <Field as Pod>::SHAPE)` pair per field, never shared between fields whose types
    // are spelled alike, for the reason given at the bounds in `expand`: a shared projection
    // would give `$crate::Header` from two crates the same shape.
    let mut fields = TokenStream::new();
    for f in &input.fields {
        let at = field_span(&f.ty);
        let mut pair = TokenStream::from(TokenTree::Literal(Literal::string(&f.shape_name)));
        pair.extend(lex(","));
        pair.extend(respan(lex("<"), at));
        pair.extend(f.ty.clone());
        pair.extend(rooted_at(
            "as",
            input.crate_path.as_ref(),
            "::Pod>::SHAPE",
            at,
        ));
        fields.extend(delimit(Delimiter::Parenthesis, pair));
        fields.extend(lex(","));
    }

    let mut fold = rooted("<() as", root, "::Pod>::__SHAPE_FOLD");
    if !input.fields.is_empty() {
        let mut slice = lex("&");
        slice.extend(delimit(Delimiter::Bracket, fields));
        fold.extend(lex(".fields"));
        fold.extend(delimit(Delimiter::Parenthesis, slice));
    }
    for param in &input.const_params {
        // `as u128` is lossless for every type a const parameter can have: an integer, `bool` or
        // `char`.
        let mut arg = TokenStream::from(TokenTree::Ident(param.clone()));
        arg.extend(lex("as ::core::primitive::u128"));
        fold.extend(lex(".param"));
        fold.extend(delimit(Delimiter::Parenthesis, arg));
    }
    if let Some(with) = &input.shape_with {
        // The user's tokens keep their spans, so a value of the wrong type is reported inside
        // their `#[pod(...)]` as "expected `u64`".
        fold.extend(lex(".with"));
        fold.extend(delimit(Delimiter::Parenthesis, with.expr.clone()));
    }
    fold.extend(lex(".finish(::core::mem::size_of::<Self>())"));

    // `unused_parens` is for a `shape_with` the user had to parenthesise (`(1 << 4)`), as for the
    // pins.
    let mut ts = lex("#[allow(unused_parens)] \
         const SHAPE: ::core::option::Option<::core::primitive::u64> =");
    ts.extend(fold);
    ts.extend(lex(";"));
    ts
}

/// Which layout property a `#[pod(...)]` pin names.
#[derive(Clone, Copy)]
enum Pinned {
    Size,
    Align,
}

impl Pinned {
    fn key(self) -> &'static str {
        match self {
            Pinned::Size => "size",
            Pinned::Align => "align",
        }
    }

    fn intrinsic(self) -> &'static str {
        match self {
            Pinned::Size => "size_of",
            Pinned::Align => "align_of",
        }
    }
}

/// The check behind `#[pod(size = N)]` or `#[pod(align = N)]`, emitted into `__LAYOUT_OK` after the
/// padding proof.
///
/// The padding proof shows a layout has no gaps; it cannot show the layout is the one bytes were
/// written against. A field added, removed or widened still proves padding-free, and every
/// checksum, save file and wire message built on the old layout silently changes meaning. A pin
/// states the size or alignment outright, so that change fails the build and names itself.
///
/// The two shapes differ because only a concrete type can say what it found.
///
/// * **Concrete**: `let _: [(); N] = [(); size_of::<Name>()];`. Array lengths are compared at type
///   check, so a mismatch is a type error that `cargo check` reports, and rustc renders both
///   lengths: "expected an array with a size of 96, found one with a size of 104". That second
///   number is the one a reader needs, and a const `assert!` has no way to print it, since const
///   panics cannot format an integer. The generated tokens are respanned onto the pinned
///   expression, so the error points into the user's `#[pod(...)]`. The type is named rather than
///   written `Self`, because an array length may not mention `Self` inside an impl.
/// * **Generic**: an `assert!` in the associated const, checked per instantiation like the padding
///   proof, and for the same reason: an array length may not depend on a generic parameter. rustc
///   names the failing instantiation (`<Ring<3> as Pod>::__LAYOUT_OK`), so the message only has to
///   say what was pinned.
///
/// The expression is re-emitted verbatim, so it may use the type's own parameters
/// (`size = 4 * N + 4`); a generic pin is then an identity over every instantiation, not one
/// number.
fn pin_check(input: &parse::Input, what: Pinned, pin: &parse::Pin) -> TokenStream {
    let name = &input.name;
    let key = what.key();
    let intrinsic = what.intrinsic();
    if input.is_concrete {
        // Built group by group rather than lexed whole, so the user's expression and type name
        // keep their own spans while everything this crate adds moves onto the pinned value.
        let bracketed = |inner: TokenStream| {
            let mut g = Group::new(Delimiter::Bracket, inner);
            g.set_span(pin.span);
            TokenStream::from(TokenTree::Group(g))
        };
        let mut expected = respan(lex("();"), pin.span);
        expected.extend(pin.expr.clone());
        let mut found = respan(lex("(); ::core::mem::"), pin.span);
        found.extend(respan(lex(intrinsic), pin.span));
        found.extend(respan(lex("::<"), pin.span));
        found.extend(TokenStream::from(TokenTree::Ident(name.clone())));
        found.extend(respan(lex(">()"), pin.span));

        // The allow is for a pin the user had to parenthesise (`size = (1 << 4)`, see
        // `parse::parse_pin`), which in array-length position rustc then calls unnecessary.
        let mut ts = lex("#[allow(unused_parens)]");
        ts.extend(respan(lex("let _:"), pin.span));
        ts.extend(bracketed(expected));
        ts.extend(respan(lex("="), pin.span));
        ts.extend(bracketed(found));
        ts.extend(respan(lex(";"), pin.span));
        return ts;
    }

    let expr = pin.expr.to_string();
    let why = match what {
        Pinned::Size => {
            "A size pin fails when a field is added, removed or resized, which changes what every \
             byte written against the old layout means. If the change is intended, update the pin."
        }
        Pinned::Align => {
            "An alignment pin fails when a field's alignment or the `repr(align)` changes, or on a \
             target where a field is less aligned: a `u64` is 8-aligned on 64-bit targets but \
             4-aligned on 32-bit x86. To hold the alignment on every target, raise it with \
             `#[repr(C, align(N))]`."
        }
    };
    // As with the padding message: an argument to `"{}"`, never the format string, because the
    // pinned expression can contain braces.
    let msg = format!(
        "`{name}` does not have the {what_word} its `#[pod({key} = {expr})]` pins: \
         `{intrinsic}::<{name}>()` differs from `{expr}`. {why}",
        what_word = match what {
            Pinned::Size => "size",
            Pinned::Align => "alignment",
        },
    );
    // Bound to a typed `let` first, so an expression of the wrong type is reported as "expected
    // `usize`" at the expression, and so the comparison needs no parentheses around user tokens.
    let mut ts = lex("#[allow(unused_parens)] let __pod_pinned: ::core::primitive::usize =");
    ts.extend(pin.expr.clone());
    ts.extend(lex(";"));
    let mut args = lex("::core::mem::");
    args.extend(lex(intrinsic));
    args.extend(lex("::<Self>() == __pod_pinned, \"{}\","));
    args.extend(TokenStream::from(TokenTree::Literal(Literal::string(&msg))));
    ts.extend(lex("::core::assert!"));
    ts.extend(delimit(Delimiter::Parenthesis, args));
    ts.extend(lex(";"));
    ts
}
