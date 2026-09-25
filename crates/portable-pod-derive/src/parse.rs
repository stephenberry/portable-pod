//! A shallow parser for the subset of a struct definition this derive needs.
//!
//! It never inspects a field *type*: a type is captured as an opaque token run and is only ever
//! re-emitted into `size_of::<...>()` and a where-clause bound. That is what makes doing this
//! without `syn` reasonable rather than reckless. The Rust-tracking surface is the scanner's
//! handling of attributes, visibility, generic parameters, and angle-bracket nesting, and
//! nothing beyond.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// Is this identifier the given keyword?
///
/// `proc_macro::Ident` does not implement `PartialEq`, unlike `proc_macro2`'s, so a bare
/// `id == "struct"` does not compile. This is the whole of the difference between the two APIs
/// as far as this file is concerned.
pub fn is(id: &Ident, kw: &str) -> bool {
    id.to_string() == kw
}

pub struct Error {
    pub span: Span,
    pub msg: String,
}

impl Error {
    fn new(span: Span, msg: impl Into<String>) -> Self {
        Error {
            span,
            msg: msg.into(),
        }
    }
    /// Render as a `compile_error!` invocation carrying the offending span.
    pub fn to_compile_error(&self) -> TokenStream {
        let mut ts = TokenStream::new();
        ts.extend([
            TokenTree::Punct(Punct::new(':', Spacing::Joint)),
            TokenTree::Punct(Punct::new(':', Spacing::Alone)),
            TokenTree::Ident(Ident::new("core", self.span)),
            TokenTree::Punct(Punct::new(':', Spacing::Joint)),
            TokenTree::Punct(Punct::new(':', Spacing::Alone)),
            TokenTree::Ident(Ident::new("compile_error", self.span)),
            TokenTree::Punct(Punct::new('!', Spacing::Alone)),
            TokenTree::Group({
                let mut g = Group::new(
                    Delimiter::Brace,
                    TokenStream::from(TokenTree::Literal(Literal::string(&self.msg))),
                );
                g.set_span(self.span);
                g
            }),
        ]);
        ts
    }
}

pub struct Field {
    /// How to name this field in a diagnostic: a field name, or a tuple index.
    pub label: String,
    /// The name the shape folds in: the label without any `r#`, since `r#type` and `type` name
    /// the same field.
    pub shape_name: String,
    /// The field type, verbatim and uninspected.
    pub ty: TokenStream,
}

pub struct Input {
    pub name: Ident,
    /// Parameters as written, defaults stripped: `const N: usize, T: Copy`.
    pub generics_decl: TokenStream,
    /// The same parameters in use position: `N, T`.
    pub generics_use: TokenStream,
    /// The names of the const parameters, in declaration order: what the shape folds in.
    pub const_params: Vec<Ident>,
    /// Predicates from an existing `where` clause, if any.
    pub where_predicates: TokenStream,
    pub fields: Vec<Field>,
    /// True when the type has no generic parameters, so its proof can be forced unconditionally.
    pub is_concrete: bool,
    /// Path from `#[pod(crate = ...)]`, if given. Every reference the expansion emits is rooted
    /// here, so a crate that re-exports the trait can be named instead of `::portable_pod`.
    /// `expand` supplies the default.
    pub crate_path: Option<TokenStream>,
    /// The expression from `#[pod(size = ...)]`, if given: the size the type must have.
    pub size: Option<Pin>,
    /// The expression from `#[pod(align = ...)]`, if given: the alignment the type must have.
    pub align: Option<Pin>,
    /// The expression from `#[pod(shape_with = ...)]`, if given: a `u64` folded into the shape.
    pub shape_with: Option<Pin>,
}

/// A `size`, `align` or `shape_with` value: a constant expression, captured uninspected like a
/// field type.
pub struct Pin {
    pub expr: TokenStream,
    /// The expression's first token, inside the user's attribute: where a mismatch is reported.
    pub span: Span,
}

/// Is this `>` the tail of `->` or `=>` rather than a closing angle bracket?
fn is_arrow_tail(prev: Option<&TokenTree>) -> bool {
    matches!(prev, Some(TokenTree::Punct(p))
        if p.spacing() == Spacing::Joint && (p.as_char() == '-' || p.as_char() == '='))
}

/// Is this `=` part of a multi-character operator (`==`, `=>`, or the tail of `>=`/`<=`/`!=`)
/// rather than the `=` that introduces a generic default?
fn is_compound_eq(here: &Punct, prev: Option<&TokenTree>, next: Option<&TokenTree>) -> bool {
    // A preceding punct joined to this one means this `=` continues that operator.
    let continues = matches!(prev, Some(TokenTree::Punct(p)) if p.spacing() == Spacing::Joint);
    // Or this `=` starts one, which requires it to be joined to an `=` or `>`.
    let starts = here.spacing() == Spacing::Joint
        && matches!(next, Some(TokenTree::Punct(p)) if matches!(p.as_char(), '=' | '>'));
    continues || starts
}

/// Consume a balanced `<...>` run starting at `*i`, returning the interior tokens.
fn take_angles(toks: &[TokenTree], i: &mut usize) -> Vec<TokenTree> {
    debug_assert!(matches!(&toks[*i], TokenTree::Punct(p) if p.as_char() == '<'));
    *i += 1;
    let mut depth = 1usize;
    let mut out = Vec::new();
    while *i < toks.len() {
        if let TokenTree::Punct(p) = &toks[*i] {
            match p.as_char() {
                '<' => depth += 1,
                '>' if !is_arrow_tail(out.last()) => {
                    depth -= 1;
                    if depth == 0 {
                        *i += 1;
                        return out;
                    }
                }
                _ => {}
            }
        }
        out.push(toks[*i].clone());
        *i += 1;
    }
    out
}

/// Split on commas that are not nested inside angle brackets. Parenthesised, bracketed and
/// braced runs need no tracking: the token tree already groups them.
fn split_commas(toks: &[TokenTree]) -> Vec<Vec<TokenTree>> {
    let mut out = Vec::new();
    let mut cur: Vec<TokenTree> = Vec::new();
    let mut depth = 0usize;
    for t in toks {
        if let TokenTree::Punct(p) = t {
            match p.as_char() {
                '<' => depth += 1,
                '>' if depth > 0 && !is_arrow_tail(cur.last()) => depth -= 1,
                ',' if depth == 0 => {
                    if !cur.is_empty() {
                        out.push(core::mem::take(&mut cur));
                    }
                    continue;
                }
                _ => {}
            }
        }
        cur.push(t.clone());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Skip `#[...]` attribute runs.
fn skip_attrs(toks: &[TokenTree], i: &mut usize) {
    while *i + 1 < toks.len() {
        let is_hash = matches!(&toks[*i], TokenTree::Punct(p) if p.as_char() == '#');
        let is_brack =
            matches!(&toks[*i + 1], TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket);
        if is_hash && is_brack {
            *i += 2;
        } else {
            break;
        }
    }
}

/// Is `ts` a visibility: nothing, `pub`, or `pub(...)`? What a `$vis:vis` fragment can hold.
fn is_visibility(ts: TokenStream) -> bool {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    match toks.as_slice() {
        [] => true,
        [TokenTree::Ident(kw)] => is(kw, "pub"),
        [TokenTree::Ident(kw), TokenTree::Group(g)] => {
            is(kw, "pub") && g.delimiter() == Delimiter::Parenthesis
        }
        _ => false,
    }
}

/// Skip `pub`, `pub(crate)`, `pub(in path)`.
///
/// A `macro_rules!` `$vis:vis` fragment reaches a derive as an invisible (`Delimiter::None`) group
/// around the visibility, and around nothing for a private item, so such a group is skipped whole:
/// an item a macro declares with `$vis struct` derives as it would written out.
fn skip_vis(toks: &[TokenTree], i: &mut usize) {
    if let Some(TokenTree::Group(g)) = toks.get(*i)
        && g.delimiter() == Delimiter::None
        && is_visibility(g.stream())
    {
        *i += 1;
        return;
    }
    if let Some(TokenTree::Ident(id)) = toks.get(*i)
        && is(id, "pub")
    {
        *i += 1;
        if let Some(TokenTree::Group(g)) = toks.get(*i)
            && g.delimiter() == Delimiter::Parenthesis
        {
            *i += 1;
        }
    }
}

struct Repr {
    c_or_transparent: bool,
    packed: bool,
    /// Span of the offending `repr` token, for diagnostics.
    span: Option<Span>,
}

/// Everything the derive reads off the item's attributes.
struct Attrs {
    repr: Repr,
    pod: PodArgs,
}

/// The arguments of every `#[pod(...)]` attribute on the item, merged.
#[derive(Default)]
struct PodArgs {
    /// The path from `#[pod(crate = ...)]`, if one was given.
    crate_path: Option<TokenStream>,
    size: Option<Pin>,
    align: Option<Pin>,
    shape_with: Option<Pin>,
}

/// The arguments `#[pod(...)]` accepts, spelled out for every diagnostic that lists them.
const POD_ARGS: &str = "`crate`, `size`, `align` and `shape_with`, as \
     `#[pod(crate = ::my_crate, size = 16, align = 8, shape_with = 0x1234)]`";

/// Parse one `#[pod(...)]` attribute's arguments.
///
/// `crate` takes a *path* (`::my_engine::mem`), not a string. Every reference the expansion emits
/// is rooted at it, so this is what lets a crate re-export `Pod` and have the derive keep working
/// through the re-export. `size` and `align` each take a `usize` constant expression, pinning the
/// layout the type must have. `shape_with` takes a `u64` constant expression, folded into the
/// type's shape.
fn parse_pod_attr(head: &Ident, args: Option<&TokenTree>, seen: &mut PodArgs) -> Result<(), Error> {
    let Some(TokenTree::Group(g)) = args else {
        return Err(Error::new(
            head.span(),
            format!("`#[pod]` takes arguments: {POD_ARGS}."),
        ));
    };
    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
    // `split_commas` never yields an empty run, so `arg[0]` is always there.
    for arg in split_commas(&inner) {
        let TokenTree::Ident(key) = &arg[0] else {
            return Err(Error::new(
                arg[0].span(),
                format!(
                    "expected `<argument> = <value>` inside `#[pod(...)]`. The arguments are {POD_ARGS}."
                ),
            ));
        };
        let wanted = if is(key, "crate") {
            "a path: write `#[pod(crate = ::my_crate)]`"
        } else if is(key, "size") {
            "the size in bytes: write `#[pod(size = 16)]`"
        } else if is(key, "align") {
            "the alignment in bytes: write `#[pod(align = 8)]`"
        } else if is(key, "shape_with") {
            "a `u64` constant expression: write `#[pod(shape_with = 0x1234)]`"
        } else {
            return Err(Error::new(
                key.span(),
                format!("unknown `#[pod]` argument `{key}`. The arguments are {POD_ARGS}."),
            ));
        };
        // A missing `=` and a missing value are one mistake to a reader, so they get one message.
        let value = match arg.get(1) {
            Some(TokenTree::Punct(p)) if p.as_char() == '=' && arg.len() > 2 => &arg[2..],
            _ => {
                return Err(Error::new(key.span(), format!("`{key}` needs {wanted}.")));
            }
        };
        if !is(key, "crate") {
            let slot = if is(key, "size") {
                &mut seen.size
            } else if is(key, "align") {
                &mut seen.align
            } else {
                &mut seen.shape_with
            };
            if slot.is_some() {
                return Err(Error::new(
                    key.span(),
                    format!(
                        "`{key}` is given more than once; keep a single `#[pod({key} = ...)]`."
                    ),
                ));
            }
            *slot = Some(parse_pin(key, value)?);
            continue;
        }
        let path = value;
        // No literal can begin a path, so any literal here is the mistake -- test the token kind
        // rather than the quoting, or a raw string, a byte string and a bare number all fall
        // through into the expansion and surface as `proc-macro derive produced unparsable
        // tokens`, which tells a user nothing. A quoted path is the specific case worth naming:
        // `serde` and `bytemuck` both take a string, so it is what someone arriving from either
        // will write. Taking tokens instead is what lets rustc report a typo *in* the path at the
        // typo, which re-lexing a string literal would throw away.
        if let [TokenTree::Literal(lit), ..] = path {
            let text = lit.to_string();
            return Err(Error::new(
                lit.span(),
                match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                    Some(inner) => format!(
                        "`#[pod(crate = ...)]` takes a path, not a string. Write \
                         `#[pod(crate = {inner})]` -- without the quotes."
                    ),
                    None => String::from(
                        "`#[pod(crate = ...)]` takes a path, as `#[pod(crate = ::my_crate)]`.",
                    ),
                },
            ));
        }
        if seen.crate_path.is_some() {
            return Err(Error::new(
                key.span(),
                "`crate` is given more than once; keep a single `#[pod(crate = ...)]`.",
            ));
        }
        seen.crate_path = Some(path.iter().cloned().collect());
    }
    Ok(())
}

/// Capture a `size`, `align` or `shape_with` value.
///
/// The value is an expression, re-emitted verbatim, so it may name constants and, on a generic
/// type, the type's own parameters (`size = 4 * N + 4`). The one thing to catch here is a `<`
/// used as an operator. `split_commas` has no way to tell `1 << 4` from the opening of generic
/// arguments, so it reads the shift as two unclosed brackets, and a `, align = 8` after it would
/// be swallowed into the size expression and surface as a baffling parse error in the expansion.
/// Refusing any value whose angle brackets do not balance catches that case and the unclosed
/// comparison alike, and parenthesising is the fix for both: inside a group, `split_commas` never
/// looks.
fn parse_pin(key: &Ident, value: &[TokenTree]) -> Result<Pin, Error> {
    let mut depth = 0usize;
    let mut first_open = None;
    for (k, t) in value.iter().enumerate() {
        if let TokenTree::Punct(p) = t {
            match p.as_char() {
                '<' => {
                    depth += 1;
                    first_open.get_or_insert(p.span());
                }
                '>' if depth > 0 && !is_arrow_tail(k.checked_sub(1).map(|j| &value[j])) => {
                    depth -= 1;
                }
                _ => {}
            }
        }
    }
    if depth != 0 {
        return Err(Error::new(
            first_open.unwrap_or_else(|| key.span()),
            format!(
                "a `<` in a `{key}` value is read as the start of generic arguments. Wrap a shift \
                 or a comparison in parentheses: `#[pod({key} = (1 << 4))]`."
            ),
        ));
    }
    Ok(Pin {
        expr: value.iter().cloned().collect(),
        span: value[0].span(),
    })
}

/// The tokens inside an attribute's brackets. A `#[$m:meta]` written in a `macro_rules!` arrives with
/// the fragment as an invisible group inside the brackets, which is opened.
fn attr_tokens(brackets: &Group) -> Vec<TokenTree> {
    let toks: Vec<TokenTree> = brackets.stream().into_iter().collect();
    match toks.as_slice() {
        [TokenTree::Group(g)] if g.delimiter() == Delimiter::None => {
            g.stream().into_iter().collect()
        }
        _ => toks,
    }
}

fn scan_attrs(toks: &[TokenTree], i: &mut usize) -> Result<Attrs, Error> {
    let mut repr = Repr {
        c_or_transparent: false,
        packed: false,
        span: None,
    };
    let mut pod = PodArgs::default();
    while *i + 1 < toks.len() {
        let TokenTree::Punct(p) = &toks[*i] else {
            break;
        };
        if p.as_char() != '#' {
            break;
        }
        let TokenTree::Group(g) = &toks[*i + 1] else {
            break;
        };
        if g.delimiter() != Delimiter::Bracket {
            break;
        }
        let inner = attr_tokens(g);
        if let Some(TokenTree::Ident(head)) = inner.first() {
            if is(head, "repr") {
                repr.span = Some(head.span());
                if let Some(TokenTree::Group(args)) = inner.get(1) {
                    for t in args.stream() {
                        if let TokenTree::Ident(id) = t {
                            if is(&id, "C") || is(&id, "transparent") {
                                repr.c_or_transparent = true;
                            } else if is(&id, "packed") {
                                repr.packed = true;
                            }
                        }
                    }
                }
            } else if is(head, "pod") {
                parse_pod_attr(head, inner.get(1), &mut pod)?;
            }
        }
        *i += 2;
    }
    Ok(Attrs { repr, pod })
}

/// Split a generic parameter list into declaration form and use form.
///
/// `<const N: usize, T: Copy = u8>` yields `const N: usize, T: Copy` and `N, T`, and `N` as the
/// one const parameter. Defaults are stripped because an `impl` generic list may not carry them.
fn split_generics(inner: &[TokenTree]) -> Result<Generics, Error> {
    let mut decl = TokenStream::new();
    let mut uses = TokenStream::new();
    let mut consts = Vec::new();
    for (n, raw) in split_commas(inner).into_iter().enumerate() {
        if n > 0 {
            decl.extend([TokenTree::Punct(Punct::new(',', Spacing::Alone))]);
            uses.extend([TokenTree::Punct(Punct::new(',', Spacing::Alone))]);
        }
        // A parameter may carry attributes (`<#[cfg(all())] T>`). Drop them: an `impl` generic
        // list cannot hold them, and they say nothing about the layout.
        let mut head = 0usize;
        skip_attrs(&raw, &mut head);
        let param = &raw[head..];

        // Declaration form: everything up to a top-level `=` default.
        let mut depth = 0usize;
        let mut cut = param.len();
        for (k, t) in param.iter().enumerate() {
            if let TokenTree::Punct(p) = t {
                let prev = k.checked_sub(1).map(|j| &param[j]);
                match p.as_char() {
                    '<' => depth += 1,
                    // The same arrow guard the other two depth loops carry. The `>` closing a
                    // `->` inside a bound (`T: Tr<A = Box<dyn Fn() -> u8>, B = u32>`) is not a
                    // closing angle bracket; without this the depth underflows, the next
                    // binding's `=` is mistaken for a top-level default, and the parameter is
                    // truncated mid-bound into unbalanced tokens.
                    '>' if depth > 0 && !is_arrow_tail(prev) => depth -= 1,
                    // A top-level `=` introduces a default, which an `impl` generic list may not
                    // carry. Detect it by looking at the neighbouring punctuation rather than at
                    // this token's spacing: `T: Copy =*const u32` is `Joint` merely because a
                    // punct follows it, so a spacing test silently kept the default. What has to
                    // be excluded is a `=` that is part of `==` or `=>` or the tail of `>=`/`!=`.
                    '=' if depth == 0 && !is_compound_eq(p, prev, param.get(k + 1)) => {
                        cut = k;
                        break;
                    }
                    _ => {}
                }
            }
        }
        decl.extend(param[..cut].iter().cloned());

        // Use form: the bare parameter name.
        match param.first() {
            Some(TokenTree::Punct(p)) if p.as_char() == '\'' => {
                uses.extend(param[..2].iter().cloned());
            }
            Some(TokenTree::Ident(id)) if is(id, "const") => {
                let Some(TokenTree::Ident(name)) = param.get(1) else {
                    return Err(Error::new(id.span(), "malformed const generic parameter"));
                };
                uses.extend([TokenTree::Ident(name.clone())]);
                consts.push(name.clone());
            }
            Some(t @ TokenTree::Ident(_)) => uses.extend([t.clone()]),
            Some(t) => return Err(Error::new(t.span(), "unsupported generic parameter")),
            None => {}
        }
    }
    Ok(Generics { decl, uses, consts })
}

/// A generic parameter list, split by [`split_generics`].
#[derive(Default)]
struct Generics {
    decl: TokenStream,
    uses: TokenStream,
    consts: Vec<Ident>,
}

fn parse_named_fields(g: &Group) -> Result<Vec<Field>, Error> {
    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
    let mut out = Vec::new();
    for f in split_commas(&inner) {
        let mut i = 0usize;
        skip_attrs(&f, &mut i);
        skip_vis(&f, &mut i);
        let Some(TokenTree::Ident(name)) = f.get(i).cloned() else {
            let span = f.first().map_or_else(Span::call_site, TokenTree::span);
            return Err(Error::new(span, "expected a field name"));
        };
        i += 1;
        match f.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == ':' => i += 1,
            _ => return Err(Error::new(name.span(), "expected `:` after the field name")),
        }
        if i >= f.len() {
            return Err(Error::new(name.span(), "field has no type"));
        }
        let label = name.to_string();
        out.push(Field {
            shape_name: label.strip_prefix("r#").unwrap_or(&label).to_owned(),
            label,
            ty: f[i..].iter().cloned().collect(),
        });
    }
    Ok(out)
}

fn parse_tuple_fields(g: &Group) -> Result<Vec<Field>, Error> {
    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
    let mut out = Vec::new();
    for (idx, f) in split_commas(&inner).into_iter().enumerate() {
        let mut i = 0usize;
        skip_attrs(&f, &mut i);
        skip_vis(&f, &mut i);
        if i >= f.len() {
            let span = f.first().map_or_else(Span::call_site, TokenTree::span);
            return Err(Error::new(span, "field has no type"));
        }
        out.push(Field {
            label: idx.to_string(),
            shape_name: idx.to_string(),
            ty: f[i..].iter().cloned().collect(),
        });
    }
    Ok(out)
}

pub fn parse(ts: TokenStream) -> Result<Input, Error> {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    let mut i = 0usize;

    let Attrs { repr, pod } = scan_attrs(&toks, &mut i)?;
    skip_vis(&toks, &mut i);

    let Some(TokenTree::Ident(kw)) = toks.get(i).cloned() else {
        return Err(Error::new(
            Span::call_site(),
            "expected a struct definition",
        ));
    };
    i += 1;
    if is(&kw, "enum") {
        return Err(Error::new(
            kw.span(),
            "`Pod` cannot be derived for an enum: a discriminant makes most bit patterns invalid, \
             so an enum can never satisfy the any-bit-pattern clause. Use an integer field plus \
             accessors, or a `#[repr(transparent)]` newtype over an integer.",
        ));
    }
    if is(&kw, "union") {
        return Err(Error::new(
            kw.span(),
            "`Pod` cannot be derived for a union: its size may exceed the sum of any one \
             variant's fields, so padding cannot be ruled out.",
        ));
    }
    if !is(&kw, "struct") {
        return Err(Error::new(kw.span(), "expected `struct`"));
    }

    let Some(TokenTree::Ident(name)) = toks.get(i).cloned() else {
        return Err(Error::new(kw.span(), "expected a struct name"));
    };
    i += 1;

    let generics = match toks.get(i) {
        Some(TokenTree::Punct(p)) if p.as_char() == '<' => {
            let inner = take_angles(&toks, &mut i);
            split_generics(&inner)?
        }
        _ => Generics::default(),
    };

    // `struct S<T> where …: { … }` and `struct S<T>(…) where …;` put the clause on either side
    // of the body, so look for both.
    let mut where_toks: Vec<TokenTree> = Vec::new();
    let mut take_where = |i: &mut usize| {
        if let Some(TokenTree::Ident(id)) = toks.get(*i)
            && is(id, "where")
        {
            *i += 1;
            let mut depth = 0usize;
            while *i < toks.len() {
                match &toks[*i] {
                    // A brace group ends the clause only at angle-depth 0, where it is the
                    // struct body. Inside `<...>` it is a braced const argument
                    // (`Arr<{ 2 * 2 }>`), which belongs to the clause -- angle brackets are
                    // not token-tree groups, so nothing else tells the two apart.
                    TokenTree::Group(g) if g.delimiter() == Delimiter::Brace && depth == 0 => {
                        break;
                    }
                    TokenTree::Punct(p) if p.as_char() == ';' && depth == 0 => break,
                    t => {
                        if let TokenTree::Punct(p) = t {
                            match p.as_char() {
                                '<' => depth += 1,
                                '>' if depth > 0 && !is_arrow_tail(where_toks.last()) => depth -= 1,
                                _ => {}
                            }
                        }
                        where_toks.push(t.clone());
                        *i += 1;
                    }
                }
            }
        }
    };
    take_where(&mut i);

    let fields = match toks.get(i) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => parse_named_fields(g)?,
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
            let f = parse_tuple_fields(g)?;
            i += 1;
            take_where(&mut i);
            f
        }
        // Unit struct: no fields, size zero, trivially padding-free.
        Some(TokenTree::Punct(p)) if p.as_char() == ';' => Vec::new(),
        None => Vec::new(),
        Some(t) => return Err(Error::new(t.span(), "expected a struct body")),
    };

    if repr.packed {
        return Err(Error::new(
            repr.span.unwrap_or_else(|| name.span()),
            "`Pod` cannot be derived for a `#[repr(packed)]` type: its fields are not necessarily \
             aligned, so references to them are not always valid.",
        ));
    }
    if !repr.c_or_transparent {
        return Err(Error::new(
            repr.span.unwrap_or_else(|| name.span()),
            "`Pod` requires an explicit `#[repr(C)]`, `#[repr(transparent)]`, or \
             `#[repr(C, align(N))]`. The default `repr(Rust)` may reorder fields and its layout \
             is not guaranteed stable across compilations, so its bytes are not portable.",
        ));
    }

    // A written `where` clause may end with a comma before the body. Keep it off, or appending
    // the derive's own bounds produces `where A: B, , C: D` and the expansion will not parse.
    if matches!(where_toks.last(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
        where_toks.pop();
    }

    let is_concrete = generics.decl.is_empty();
    Ok(Input {
        name,
        generics_decl: generics.decl,
        generics_use: generics.uses,
        const_params: generics.consts,
        where_predicates: where_toks.into_iter().collect(),
        fields,
        is_concrete,
        crate_path: pod.crate_path,
        size: pod.size,
        align: pod.align,
        shape_with: pod.shape_with,
    })
}
