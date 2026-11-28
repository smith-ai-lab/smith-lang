//! Checks a parsed program against the rules of the language and lowers it
//! to the scalar form.
//!
//! It collects every signature, then checks each definition `main`
//! reaches: the body of each function, each struct and trait those name,
//! and every implementation of those traits or for those structs. A
//! definition that `main` does not reach is parsed and not checked.
//!
//! It also finds what `smith check` warns of: each definition `main` does
//! not reach, and each binding whose name is never read.
//!
//! A function with type parameters is checked once, with its parameters
//! standing for any type that meets their bounds. It is lowered once for
//! each choice of types a call makes, and a call of a trait's function
//! becomes a call of the implementation the types choose, so no trait is
//! left when the program runs. A struct is lowered to its fields, and a
//! tuple to its elements, one after another, until only texts are left, and
//! an enum to which case it holds, followed by the texts of that case, in as
//! many places as its largest case takes. The scalar form holds integers and
//! addresses alone.
//!
//! The type of each expression is recorded as it is checked, among the
//! type parameters of the definition it is written in, so lowering a copy
//! of a function reads the types of its expressions with its type
//! parameters replaced.
//!
//! A default is filled in where the call or construction that leaves it out
//! is written, so it is lowered as if it were written there, and no copy of
//! a function knows which of its arguments were left out.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

mod arith;
mod core_term;
mod effects_with_and_try;
mod for_arrays;
mod frames;
mod functions_calls_and_lambdas;
mod infer;
mod lists_and_text;
mod mut_parameters;
mod names;
mod numbers_and_operators;
mod ownership;
mod proof;
mod proof_functions;
mod scope;
mod to_scalar;
mod traits;
mod values;

use crate::core;
use crate::diag::{Error, Pos};
use crate::hash::{Map, Set};
use crate::parser::outline::{Kind as PieceKind, Piece, outline};
use crate::parser::{
    Arg, ArithOp, Arm, Bound, Call, Enum, Expr, File, Form, Function, Generic, Impl, Lambda, Loop,
    Match, Over, Part, Pattern, Qualified, Range, Stmt, Struct, Trait, Type, Use,
};
use effects_with_and_try::{
    BUILT, BUILT_IN_EFFECTS, LIBRARY_EFFECTS, declarations, expand_groups, is_arena_call,
    provided_by,
};
use lists_and_text::{as_read, is_freeze, is_push, is_utf8};
use names::{Naming, Resolution, own_final, renamed_tokens};
use numbers_and_operators::{
    Int, Narrowing, Numbers, flexible, is_float_call, literal_blocks, narrowing, own_range,
};
use proof_functions::Held;
use traits::{
    Found, ImplInfo, PRELUDE_TRAITS, Requirement, TRAITS_NOT_BUILT, bounds_named, impl_info,
    needs_named, overlaps, split_needs, withouts,
};

include!(concat!(env!("OUT_DIR"), "/std_modules.rs"));

/// The names that are in scope in every file with no `use`, as the page of
/// the prelude lists them, which a `use` is refused for.
const PRELUDE_LISTED: &[&str] = &[
    "Int",
    "Byte",
    "f16",
    "bf16",
    "f32",
    "f64",
    "bool",
    "str",
    "Nat",
    "List",
    "Slice",
    "Option",
    "Result",
    "Try",
    "then",
    "Order",
    "Eq",
    "Ord",
    "eq",
    "compare",
    "Add",
    "Sub",
    "Mul",
    "Div",
    "Rem",
    "Pow",
    "Neg",
    "add",
    "sub",
    "mul",
    "div",
    "rem",
    "pow",
    "neg",
    "Index",
    "valid",
    "at",
    "Iterate",
    "each",
    "Flow",
    "Exit",
    "exit",
    "Fmt",
    "Repr",
    "fmt",
    "repr",
    "Convert",
    "convert",
    "Hash",
    "Copy",
    "Discard",
    "FromInt",
    "FromFloat",
    "FromString",
    "FromArray",
    "alloc",
    "gpu",
    "diverge",
    "foreign",
    "exec",
];

/// The one type the prelude defines that the compiler builds so far.
const TEXT: &str = "str";

/// The types the language and its library define that the compiler does
/// not build yet.
const TYPES_NOT_BUILT: &[&str] = &["usize", "f16", "bf16", "Pos", "Ptr", "Map"];

/// The names of sizes, which `std.bits` holds, and which the compiler knows
/// as the whole number types of that many bits.
const SIZES: &[&str] = &["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64"];

/// The items of the standard library that the compiler finds by name, as
/// `library_item` does, and so are read with their module.
const LEANED_ON: &[(&str, &str)] = &[
    ("std.deserialize", "ReadError"),
    ("std.deserialize", "Problem"),
    ("std.task", "Frame"),
    ("std.task", "Task"),
    ("std.task", "Poll"),
    ("std.task", "Wait"),
];

/// The names of the types `PRELUDE` defines.
const PRELUDE_TYPES: &[&str] = &[
    TEXT,
    "Option",
    "Result",
    "bool",
    "Order",
    "Flow",
    "Utf8Error",
    "Int",
    "Byte",
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
];

/// Whether a definition is the prelude's.
fn of_prelude(pos: Pos) -> bool {
    pos.line == 0
}

include!(concat!(env!("OUT_DIR"), "/prelude.rs"));

/// An `impl` of the prelude, whole, with the names its header holds, which is
/// all of it that is read before a program reaches it.
pub(crate) struct PreludeImpl<'a> {
    source: &'a str,
    /// The trait it is for.
    name: &'a str,
    /// The names after the trait's: the types it is for, and its generics.
    types: &'a [&'a str],
}

/// A trait of the prelude that a program reads only when it calls for it: it
/// writes the trait's name or the name of one of its functions, or imports a
/// module that does.
pub(crate) struct PreludeNumeric<'a> {
    source: &'a str,
    /// The trait's name, and then the names of its functions.
    names: &'a [&'a str],
}

/// What is read of the prelude's source, `std/prelude.smith`, which
/// `build.rs` divides into its items when the compiler is built. Every program
/// reads the types and traits, which the checker finds by name wherever syntax
/// means them, and an `impl` is read when the program reaches its trait or a
/// type it is for. The text is at line 0, which no file has, so nothing in a
/// file can be confused with it.
pub(crate) struct Prelude<'a> {
    declarations: &'a str,
    /// The traits of numbers, each read for a program that calls for it.
    numeric: &'a [PreludeNumeric<'a>],
    impls: &'a [PreludeImpl<'a>],
    /// The items read so far in the process, when the prelude is the one the
    /// compiler holds, so that a process that checks many programs reads each
    /// item once.
    parsed: Option<&'static Parsed>,
}

/// The prelude's types and traits, shared and not copied.
struct Declared {
    enums: Vec<Arc<Enum>>,
    structs: Vec<Arc<Struct>>,
    traits: Vec<Arc<Trait>>,
}

/// The items of the prelude's own, parsed once in a process.
struct Parsed {
    declarations: OnceLock<File>,
    numeric: [OnceLock<File>; PRELUDE_NUMERIC.len()],
    impls: [OnceLock<Arc<Impl>>; PRELUDE_IMPLS.len()],
}

static PARSED: Parsed = Parsed {
    declarations: OnceLock::new(),
    numeric: [const { OnceLock::new() }; PRELUDE_NUMERIC.len()],
    impls: [const { OnceLock::new() }; PRELUDE_IMPLS.len()],
};

/// The prelude the compiler holds.
const PRELUDE: Prelude<'static> = Prelude {
    declarations: PRELUDE_DECLARATIONS,
    numeric: PRELUDE_NUMERIC,
    impls: PRELUDE_IMPLS,
    parsed: Some(&PARSED),
};

impl Prelude<'_> {
    /// The prelude's types and traits, which every program reads, and the
    /// traits of numbers that `wanted` says the program calls for.
    fn declared(&self, wanted: &[bool]) -> Declared {
        let of = |items: &File, out: &mut Declared| {
            out.enums.extend(items.enums.iter().cloned());
            out.structs.extend(items.structs.iter().cloned());
            out.traits.extend(items.traits.iter().cloned());
        };
        let mut out = Declared {
            enums: Vec::new(),
            structs: Vec::new(),
            traits: Vec::new(),
        };
        let parse = |text: &str| split_needs(prelude_file(text));
        match self.parsed {
            Some(parsed) => {
                of(
                    parsed.declarations.get_or_init(|| parse(self.declarations)),
                    &mut out,
                );
                for (i, n) in self.numeric.iter().enumerate().filter(|(i, _)| wanted[*i]) {
                    of(parsed.numeric[i].get_or_init(|| parse(n.source)), &mut out);
                }
            }
            None => {
                of(&parse(self.declarations), &mut out);
                for n in self
                    .numeric
                    .iter()
                    .zip(wanted)
                    .filter(|(_, w)| **w)
                    .map(|(n, _)| n)
                {
                    of(&parse(n.source), &mut out);
                }
            }
        }
        out
    }

    /// Which traits of numbers a name calls for: the trait, or one of its
    /// functions.
    fn call_for(&self, name: &str, wanted: &mut [bool]) {
        for (i, n) in self.numeric.iter().enumerate() {
            if n.names.contains(&name) {
                wanted[i] = true;
            }
        }
    }

    /// The `impl` at the place in the prelude's source.
    fn impl_at(&self, i: usize) -> Arc<Impl> {
        let parse = || {
            let mut file = prelude_file(self.impls[i].source);
            file.impls.pop().expect("the prelude holds an impl here")
        };
        match self.parsed {
            Some(parsed) => parsed.impls[i].get_or_init(parse).clone(),
            None => parse(),
        }
    }

    /// The `impl`s of the prelude that the items `reached` in the file call
    /// for, which are those of a trait reached and those for a type reached,
    /// without the ones already `loaded`.
    fn wanted(&self, file: &File, reached: &[Item], loaded: &[usize]) -> Vec<usize> {
        let mut traits: Set<&str> = Set::default();
        let mut types: Set<&str> = Set::default();
        for item in reached {
            match *item {
                Item::Trait(k) => {
                    traits.insert(&file.traits[k].name);
                }
                Item::Struct(s) => {
                    types.insert(&file.structs[s].name);
                }
                Item::Enum(e) => {
                    types.insert(&file.enums[e].name);
                }
                Item::Fn(_) | Item::Impl(_) => {}
            }
        }
        (self.impls.iter().enumerate())
            .filter(|(i, im)| {
                !loaded.contains(i)
                    // An `impl` for floats is read for a program that writes one.
                    && (file.floats
                        || im.types.is_empty()
                        || !im.types.iter().all(|t| matches!(*t, "f32" | "f64")))
                    && (traits.contains(im.name) || im.types.iter().any(|t| types.contains(t)))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// The definitions in the source of the prelude, at line 0, which no file has.
fn prelude_file(source: &str) -> File {
    let mut tokens = crate::lexer::lex(source.as_bytes()).expect("the prelude lexes");
    for t in &mut tokens {
        t.pos.line = 0;
    }
    crate::parser::parse(&tokens).expect("the prelude parses")
}

/// The file with the prelude's types and traits before its own, and the
/// `impl`s of the prelude at the places `loaded` gives.
fn with_prelude(mut file: File, prelude: &Prelude, loaded: &[usize], wanted: &[bool]) -> File {
    let declared = prelude.declared(wanted);
    let start = prelude_file(&start_function(&file));
    file.enums.splice(0..0, declared.enums);
    file.structs.splice(0..0, declared.structs);
    file.traits.splice(0..0, declared.traits);
    file.impls
        .splice(0..0, loaded.iter().map(|&i| prelude.impl_at(i)));
    file.functions.splice(0..0, start.functions);
    file
}

/// The name of the function the program starts at when `main` gives a
/// status.
const START: &str = "__main";

/// The function that starts a program whose `main` returns a type: it calls
/// `main` and ends the program with the status that the `Exit` of what
/// `main` returns gives. It lists the effects of `main`, and the two that
/// `Exit` carries. A program whose `main` returns nothing starts at `main`.
fn start_function(file: &File) -> String {
    let Some(main) = file
        .functions
        .iter()
        .find(|f| f.name == "main" && f.result.is_some())
    else {
        return String::new();
    };
    let mut effects: Vec<&str> = vec!["console", "alloc"];
    for (name, _) in &main.effects {
        if !effects.contains(&name.as_str()) {
            effects.push(name);
        }
    }
    let (params, args) = if main.params.is_empty() {
        ("", "")
    } else {
        ("args: List[str]", "args")
    };
    format!(
        "\nfn {START}({params}) <{}> {{\n  __end(Exit.exit(main({args})))\n}}\n",
        effects.join(", ")
    )
}

/// What an expression gives.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Ty {
    Text,
    /// A whole number type.
    Int(Int),
    /// A float type.
    Float(Float),
    /// A struct, by its place in the file's structs, with a type for each
    /// of its type parameters.
    Struct(usize, Vec<Ty>),
    /// An enum, by its place in the file's enums, with a type for each of
    /// its type parameters.
    Enum(usize, Vec<Ty>),
    /// A tuple of two values or more, with the type of each.
    Tuple(Vec<Ty>),
    /// A `List`, which holds its one element type.
    List(Vec<Ty>),
    /// A `Slice`, a run of elements that cannot change, which holds its one
    /// element type.
    Slice(Vec<Ty>),
    /// An array of a size written as a number, which holds its one element
    /// type and its size, and is held in place.
    Array(Vec<Ty>, u64),
    /// A function: the types of its parameters, followed by the type it
    /// gives.
    Fn(Vec<Ty>),
    /// The type whose one value is the function with the name, by its place
    /// among the file's functions, where a function's name is written where a
    /// type is expected.
    Named(usize),
    /// A type parameter of the function, the implementation, the struct,
    /// or the trait the type is written in, by its place among them.
    Param(usize),
    /// The empty tuple, `()`, which a function with no return type gives.
    Nothing,
    /// A whole number bound to a name, whose type the uses of the name have
    /// not yet decided, by its place among those of the function. It exists
    /// only while the uses are looked at, before the body is checked.
    Flex(usize),
    /// The expression holds a mistake already reported, so what it gives
    /// is not known and nothing about it is reported again. A type that
    /// holds an unknown type is itself unknown.
    Unknown,
}

/// The float types that are built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Float {
    F32,
    F64,
}

impl Float {
    /// The float type a name writes.
    fn named(name: &str) -> Option<Float> {
        match name {
            "f32" => Some(Float::F32),
            "f64" => Some(Float::F64),
            _ => None,
        }
    }

    /// How it is written.
    fn written(self) -> &'static str {
        match self {
            Float::F32 => "f32",
            Float::F64 => "f64",
        }
    }

    /// The bits of the float the text of a float literal says, rounded once,
    /// to the nearest, ties to even, with its sign flipped if `negative`. It
    /// is `None` where the number is too large for the type, as the literal
    /// is then an infinity, which no literal is.
    pub(crate) fn bits_of(self, text: &str, negative: bool) -> Option<u64> {
        let digits: String = text.chars().filter(|c| *c != '_').collect();
        let (bits, finite) = match self {
            Float::F32 => {
                let x: f32 = digits.parse().ok()?;
                (u64::from(x.to_bits()), x.is_finite())
            }
            Float::F64 => {
                let x: f64 = digits.parse().ok()?;
                (x.to_bits(), x.is_finite())
            }
        };
        let sign = match self {
            Float::F32 => 1u64 << 31,
            Float::F64 => 1u64 << 63,
        };
        finite.then_some(if negative { bits | sign } else { bits })
    }
}

impl Ty {
    /// The struct `s` with these types, or unknown if one of them is.
    fn structure(s: usize, args: Vec<Ty>) -> Ty {
        if args.contains(&Ty::Unknown) {
            Ty::Unknown
        } else {
            Ty::Struct(s, args)
        }
    }

    /// The enum `e` with these types, or unknown if one of them is.
    fn enumeration(e: usize, args: Vec<Ty>) -> Ty {
        if args.contains(&Ty::Unknown) {
            Ty::Unknown
        } else {
            Ty::Enum(e, args)
        }
    }

    /// The list of this element type, or unknown if it is.
    fn list(element: Ty) -> Ty {
        if element.unknown() {
            Ty::Unknown
        } else {
            Ty::List(vec![element])
        }
    }

    /// The slice of this element type, or unknown if it is.
    fn slice(element: Ty) -> Ty {
        if element.unknown() {
            Ty::Unknown
        } else {
            Ty::Slice(vec![element])
        }
    }

    /// The tuple of these types: the empty tuple if there are none, and
    /// unknown if one of them is.
    fn tuple(items: Vec<Ty>) -> Ty {
        if items.is_empty() {
            Ty::Nothing
        } else if items.contains(&Ty::Unknown) {
            Ty::Unknown
        } else {
            Ty::Tuple(items)
        }
    }

    /// The function with parameters and a result of these types, the
    /// result last, or unknown if one of them is.
    fn func(items: Vec<Ty>) -> Ty {
        if items.contains(&Ty::Unknown) {
            Ty::Unknown
        } else {
            Ty::Fn(items)
        }
    }

    /// The types in its brackets, for a struct or an enum, the types of its
    /// elements, for a tuple, and those of its parameters and its result,
    /// for a function.
    fn args(&self) -> &[Ty] {
        match self {
            Ty::Struct(_, args)
            | Ty::Enum(_, args)
            | Ty::Tuple(args)
            | Ty::Fn(args)
            | Ty::List(args)
            | Ty::Slice(args) => args,
            _ => &[],
        }
    }

    /// The type with each undecided whole number made an `i64`, which is
    /// what it is when nothing decides it.
    fn concrete(&self) -> Ty {
        match self {
            Ty::Flex(_) => Ty::Int(Int::I64),
            t => t.with_args(t.args().iter().map(Ty::concrete).collect()),
        }
    }

    /// Whether it is a function, or holds one.
    /// Whether two types are the same but for the function a part of them
    /// holds, which is what a type of its own for each function makes of
    /// two values that hold different ones.
    fn differs_in_function(&self, other: &Ty) -> bool {
        match (self, other) {
            (Ty::Named(a), Ty::Named(b)) => a != b,
            (Ty::Struct(a, x), Ty::Struct(b, y)) | (Ty::Enum(a, x), Ty::Enum(b, y)) => {
                a == b && x.len() == y.len() && Ty::differ_in_function(x, y)
            }
            (Ty::Tuple(x), Ty::Tuple(y))
            | (Ty::List(x), Ty::List(y))
            | (Ty::Slice(x), Ty::Slice(y)) => x.len() == y.len() && Ty::differ_in_function(x, y),
            (Ty::Array(x, n), Ty::Array(y, m)) => n == m && Ty::differ_in_function(x, y),
            _ => false,
        }
    }

    /// Whether the types in `x` and `y` are each the same but for functions,
    /// and one differs in a function.
    fn differ_in_function(x: &[Ty], y: &[Ty]) -> bool {
        x.iter()
            .zip(y)
            .all(|(a, b)| a == b || a.differs_in_function(b))
            && x.iter().zip(y).any(|(a, b)| a != b)
    }

    fn holds_fn(&self) -> bool {
        matches!(self, Ty::Fn(_)) || self.args().iter().any(Ty::holds_fn)
    }

    /// The same kind of type, with these types in its brackets.
    fn with_args(&self, args: Vec<Ty>) -> Ty {
        match self {
            Ty::Struct(s, _) => Ty::structure(*s, args),
            Ty::Enum(e, _) => Ty::enumeration(*e, args),
            Ty::Tuple(_) => Ty::tuple(args),
            Ty::Fn(_) => Ty::func(args),
            Ty::List(_) => Ty::list(args.into_iter().next().expect("an element type")),
            Ty::Slice(_) => Ty::slice(args.into_iter().next().expect("an element type")),
            t => t.clone(),
        }
    }

    /// The type with each type parameter moved `by` places on.
    fn shifted(&self, by: usize) -> Ty {
        match self {
            Ty::Param(i) => Ty::Param(i + by),
            t => t.with_args(t.args().iter().map(|t| t.shifted(by)).collect()),
        }
    }

    /// The type with each type parameter replaced by the type at its place
    /// in `args`.
    fn subst(&self, args: &[Ty]) -> Ty {
        match self {
            Ty::Param(i) => args[*i].clone(),
            t => t.with_args(t.args().iter().map(|t| t.subst(args)).collect()),
        }
    }

    /// The same, where a parameter with no type in `args` is unknown.
    fn solved(&self, args: &[Option<Ty>]) -> Ty {
        match self {
            Ty::Param(i) => args[*i].clone().unwrap_or(Ty::Unknown),
            t => t.with_args(t.args().iter().map(|t| t.solved(args)).collect()),
        }
    }

    /// The same, where a parameter with no type in `args` is unknown, and
    /// a type that holds an unknown type keeps what is known of it, so that
    /// what is expected of a value can be partly known.
    fn partial(&self, args: &[Option<Ty>]) -> Ty {
        match self {
            Ty::Param(i) => args[*i].clone().unwrap_or(Ty::Unknown),
            Ty::Struct(s, of) => Ty::Struct(*s, of.iter().map(|t| t.partial(args)).collect()),
            Ty::Enum(e, of) => Ty::Enum(*e, of.iter().map(|t| t.partial(args)).collect()),
            Ty::Tuple(of) => Ty::Tuple(of.iter().map(|t| t.partial(args)).collect()),
            Ty::List(of) => Ty::List(of.iter().map(|t| t.partial(args)).collect()),
            Ty::Slice(of) => Ty::Slice(of.iter().map(|t| t.partial(args)).collect()),
            Ty::Fn(of) => Ty::Fn(of.iter().map(|t| t.partial(args)).collect()),
            t => t.clone(),
        }
    }

    /// Whether some part of it is not known.
    fn unknown(&self) -> bool {
        *self == Ty::Unknown || self.args().iter().any(Ty::unknown)
    }

    fn mentions(&self, p: usize) -> bool {
        match self {
            Ty::Param(i) => *i == p,
            t => t.args().iter().any(|t| t.mentions(p)),
        }
    }

    /// How many names it takes to write the type.
    fn size(&self) -> usize {
        1 + self.args().iter().map(Ty::size).sum::<usize>()
    }
}

/// Whether two types, either of which may be partly known, can be the
/// same.
fn agrees(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Unknown, _) | (_, Ty::Unknown) => true,
        (Ty::Struct(s, x), Ty::Struct(t, y)) | (Ty::Enum(s, x), Ty::Enum(t, y)) => {
            s == t && x.iter().zip(y).all(|(a, b)| agrees(a, b))
        }
        (Ty::Tuple(x), Ty::Tuple(y))
        | (Ty::Fn(x), Ty::Fn(y))
        | (Ty::List(x), Ty::List(y))
        | (Ty::Slice(x), Ty::Slice(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| agrees(a, b))
        }
        (a, b) => a == b,
    }
}

/// Matches `pattern`, whose type parameters stand for the types in `sol`,
/// against `given`, filling in `sol`. Gives whether they fit. A type
/// parameter of `given` stands only for itself.
fn unify(pattern: &Ty, given: &Ty, sol: &mut [Option<Ty>]) -> bool {
    match (pattern, given) {
        (_, Ty::Unknown) | (Ty::Unknown, _) => true,
        (Ty::Param(i), _) if *i >= sol.len() => true,
        (Ty::Param(i), given) => match &sol[*i] {
            None => {
                sol[*i] = Some(given.clone());
                true
            }
            Some(t) if *t == Ty::Unknown => true,
            Some(t) if t == given => true,
            Some(t) => {
                let agreed = flex_pair(t, given);
                if agreed && matches!(t, Ty::Flex(_)) {
                    sol[*i] = Some(given.clone());
                }
                agreed
            }
        },
        (a, b) if flex_pair(a, b) => true,
        (Ty::Struct(s, a), Ty::Struct(t, b)) | (Ty::Enum(s, a), Ty::Enum(t, b)) => {
            s == t && a.iter().zip(b).all(|(a, b)| unify(a, b, sol))
        }
        (Ty::Tuple(a), Ty::Tuple(b))
        | (Ty::Fn(a), Ty::Fn(b))
        | (Ty::List(a), Ty::List(b))
        | (Ty::Slice(a), Ty::Slice(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| unify(a, b, sol))
        }
        (a, b) => a == b,
    }
}

/// What the uses of the whole numbers bound to names, whose types are not
/// yet decided, have asked of them while a body is looked at.
#[derive(Default)]
struct Uses {
    /// The place of the use being looked at.
    at: Option<Pos>,
    /// The type a use asked of a number, and where.
    wants: Vec<(usize, Int, Pos)>,
    /// Two numbers that must be of one type.
    same: Vec<(usize, usize)>,
}

thread_local! {
    static USES: std::cell::RefCell<Uses> = std::cell::RefCell::new(Uses::default());
}

/// Notes the place of the use that is looked at next.
fn use_at(pos: Pos) {
    USES.with(|u| u.borrow_mut().at = Some(pos));
}

/// Whether one of two types is an undecided whole number and the other asks
/// of it a whole number type or another undecided one, which is noted.
fn flex_pair(a: &Ty, b: &Ty) -> bool {
    USES.with(|u| {
        let mut u = u.borrow_mut();
        let at = u.at.unwrap_or(Pos::START);
        match (a, b) {
            (Ty::Flex(k), Ty::Int(t)) | (Ty::Int(t), Ty::Flex(k)) => {
                u.wants.push((*k, *t, at));
                true
            }
            (Ty::Flex(j), Ty::Flex(k)) => {
                u.same.push((*j, *k));
                true
            }
            _ => false,
        }
    })
}

/// The type parameters a type is written among: their names, and what the
/// definition requires of them, as `T: Trait` and `requires Trait[T]` do.
#[derive(Clone, Debug, Default)]
struct Scope {
    names: Vec<String>,
    requires: Vec<Requirement>,
    /// Whether a requirement of each type parameter was reported as a
    /// mistake. Whatever it would allow is allowed, so nothing it explains
    /// is reported.
    invalid: Vec<bool>,
    /// Whether a bound or a `requires` line says that each type parameter has
    /// `Copy`, and the same for `Discard`. Nothing else is known of one.
    copy: Vec<bool>,
    discard: Vec<bool>,
    /// Where a struct that holds a function is written without the function,
    /// as `Pricing` is: the place of the first of the type parameters, which no
    /// source writes, that stand for the functions it holds.
    implicit_at: Map<Pos, usize>,
}

/// Whether `expr` is a whole number literal that an `i64` holds.
fn fits_i64(expr: &Expr) -> bool {
    match expr {
        Expr::Number {
            magnitude,
            negative,
            ..
        } if *negative => *magnitude <= Int::I64.min().unsigned_abs(),
        Expr::Number { magnitude, .. } => *magnitude <= Int::I64.max() as u128,
        // A float is an `f64` where nothing decides.
        e => float_written(e),
    }
}

/// Whether the expression is a float written out, or an operation of floats
/// written out, whose type the place it is used decides.
pub(super) fn float_written(e: &Expr) -> bool {
    match e {
        Expr::Float { .. } => true,
        Expr::Arith { left, right, .. } => float_written(left) && float_written(right),
        _ => false,
    }
}

/// Fills in `sol` the type parameters of `pattern` that `given` says, where
/// `given` is wholly known and holds no whole number whose type is not yet
/// decided. Nothing is noted of a use, and a mismatch is left to the call.
fn seed(pattern: &Ty, given: &Ty, sol: &mut [Option<Ty>]) {
    match (pattern, given) {
        (Ty::Param(i), given) if sol[*i].is_none() && !given.unknown() && !has_flex(given) => {
            sol[*i] = Some(given.clone());
        }
        (Ty::Param(_), _) => {}
        (Ty::Struct(s, a), Ty::Struct(t, b)) | (Ty::Enum(s, a), Ty::Enum(t, b)) if s == t => {
            a.iter().zip(b).for_each(|(a, b)| seed(a, b, sol));
        }
        (Ty::Tuple(a), Ty::Tuple(b)) | (Ty::List(a), Ty::List(b)) if a.len() == b.len() => {
            a.iter().zip(b).for_each(|(a, b)| seed(a, b, sol));
        }
        _ => {}
    }
}

/// Whether a whole number whose type is not yet decided is part of `t`.
fn has_flex(t: &Ty) -> bool {
    matches!(t, Ty::Flex(_)) || t.args().iter().any(has_flex)
}

/// What a caller needs to know of a function.
struct Signature {
    /// The type parameters its types are written among.
    scope: Scope,
    params: Vec<(String, Ty)>,
    /// How each parameter's type is written, in a file that has an alias.
    written: Vec<Option<String>>,
    /// Whether each parameter has a default, so that a call may leave it
    /// out.
    defaults: Vec<bool>,
    /// The type each type parameter takes when the arguments of a call do
    /// not settle it, among the type parameters, for those that have a
    /// default.
    param_defaults: Vec<Option<Ty>>,
    /// Where the parameters that a call gives by name only begin: those before
    /// it are given by position or by name, and those from it by name. A
    /// signature that is not a function's own, as a handler's, has none, and
    /// its parameters are given either way.
    named_from: Option<usize>,
    gives: Ty,
    /// The parameter of a function type that the function gives, when its
    /// result is one of its parameters: the function is that of the argument.
    gives_param: Option<usize>,
    /// For a result that is a struct that holds functions, the functions that
    /// are those of parameters: the place among the functions it holds, and the
    /// parameter, whose argument at each call is the function.
    gives_held: Vec<(usize, usize)>,
    /// The effects it carries that a caller must have, which are those it
    /// lists that the compiler builds.
    effects: Vec<String>,
    /// For each parameter that is a function with effects, the effects its
    /// function carries, which a lambda written for it may carry beside those
    /// of the function it is written in. A parameter that is not one has none.
    fn_effects: Vec<Vec<String>>,
}

impl Signature {
    /// How many of the parameters are given by position: those before the `;`.
    fn by_position(&self) -> usize {
        self.named_from.unwrap_or(self.params.len())
    }

    /// What `__end` takes: the status, a byte.
    fn end() -> Signature {
        Signature {
            scope: Scope::default(),
            params: vec![("status".to_string(), Ty::Int(Int::U8))],
            written: Vec::new(),
            defaults: vec![false],
            param_defaults: Vec::new(),
            named_from: Some(1),
            gives: Ty::Nothing,
            gives_param: None,
            gives_held: Vec::new(),
            effects: Vec::new(),
            fn_effects: Vec::new(),
        }
    }

    fn print() -> Signature {
        Signature {
            scope: Scope::default(),
            params: vec![("text".to_string(), Ty::Text)],
            written: Vec::new(),
            defaults: vec![false],
            param_defaults: Vec::new(),
            named_from: Some(1),
            gives: Ty::Nothing,
            gives_param: None,
            gives_held: Vec::new(),
            effects: vec!["console".to_string()],
            fn_effects: Vec::new(),
        }
    }
}

/// The definitions of a file, by name, and what is known of each before
/// any body is checked.
struct Defs<'a> {
    file: &'a File,
    /// For each struct, how many functions it holds that its type does not
    /// write: those of its own fields of a function type, and those of the
    /// structs its fields hold.
    implicit: Vec<usize>,
    /// Whether every item of the library is checked, as a check of the library
    /// does, and not those a program reaches.
    whole: bool,
    /// How each module wrote its names, and what each calls what.
    resolution: Resolution,
    function_names: Map<std::borrow::Cow<'a, str>, usize>,
    /// What a name written in a module is as a function, by the number of the
    /// module, for the names that were asked.
    function_memo: std::cell::RefCell<Vec<Map<Box<str>, Option<usize>>>>,
    /// The structs, enums, and traits, by what the file calls each.
    types: Types<'a>,
    /// The functions of traits with each name, as the trait and the
    /// function's place in it.
    trait_fns: Map<&'a str, Vec<(usize, usize)>>,
    /// Whether each struct is refused as an alias that names itself or as a
    /// type of which no value can be made, as already reported. A type that
    /// names one is not known.
    cyclic: Vec<bool>,
    /// The same, for each enum.
    cyclic_enums: Vec<bool>,
    /// Whether each struct holds itself with no `List` between, directly or
    /// through others, so that a value of it is the address of a node of its
    /// scalars.
    boxed: Vec<bool>,
    /// The same, for each enum.
    boxed_enums: Vec<bool>,
    struct_scopes: Vec<Scope>,
    /// The type of each field of each struct, among its type parameters.
    fields: Vec<Vec<Ty>>,
    /// For each struct that holds a proof, the function its construction is
    /// checked as: the fields are its parameters, and each proof field is a
    /// proof parameter with the default `arith`.
    constructions: Vec<Option<(Function, Signature)>>,
    /// For each field that holds a function, the function type it is written
    /// with, which is what a call of it is checked against while the function
    /// it holds is not known.
    field_items: Vec<Vec<Option<Vec<Ty>>>>,
    /// For each struct the parser made to hold what a lambda reads, the lambda,
    /// and the types of its parameters and of its result, the result last,
    /// among the type parameters of the function it is written in.
    env_lambdas: Vec<EnvLambda<'a>>,
    /// The struct of each lambda it is made for, by where the lambda is written.
    env_at: Map<Pos, usize>,
    /// The function each such struct's lambda is written in.
    env_owner: Vec<Option<usize>>,
    /// The types of the names that a lambda reads that its function defines, which
    /// the signature of a function that gives the lambda is made with, found by
    /// checking the body of the function.
    env_types: Map<usize, Vec<Ty>>,
    /// The calls that gave the parameters of each lambda that leaves types out,
    /// by the struct that holds it, with where each is and how it is written.
    env_calls: Map<usize, Vec<(Pos, String)>>,
    /// The lambdas that leave types out whose calls pass what the lambda gives,
    /// so that the types of its parameters do not settle.
    env_unsettled: Set<usize>,
    enum_scopes: Vec<Scope>,
    /// The type each case of each enum holds, if it holds one, among the
    /// enum's type parameters.
    cases: Vec<Vec<Option<Ty>>>,
    signatures: Vec<Signature>,
    /// The signature of each function of each trait, among the trait's
    /// types.
    trait_sigs: Vec<Vec<Signature>>,
    /// Which types of each trait are `out`.
    outs: Vec<Vec<bool>>,
    impls: Vec<Option<ImplInfo>>,
    /// The implementations of each trait, in the order they are written.
    impls_of: Vec<Vec<usize>>,
    /// The prelude's `Fmt`, `Repr`, and `Eq`, which a type is given
    /// automatically, by their places in the file's traits.
    fmt: usize,
    repr: usize,
    eq: usize,
    /// The prelude's `Exit` and `Iterate`, by their places in the file's
    /// traits.
    exit: usize,
    iterate: usize,
    /// The prelude's `Ord`, `Hash`, `Convert`, and `Bits`, by their places in
    /// the file's traits.
    ord: usize,
    /// The traits of `+`, `-`, and `*`.
    operators: [usize; 3],
    hash: usize,
    convert: usize,
    bits: usize,
    /// The prelude's `Function`, which every function with a name is given.
    function: usize,
    /// The standard library's `Serialize` and `Writer`, which `std.serialize`
    /// defines, and its `Deserialize` and `Reader`, which `std.deserialize`
    /// defines, by their places in the file's traits, when the program has
    /// that module.
    serialize: Option<usize>,
    writer: Option<usize>,
    deserialize: Option<usize>,
    reader: Option<usize>,
    /// The standard library's `Describe` and `Describer`, which `std.describe`
    /// defines, by their places in the file's traits, when the program has
    /// that module.
    describe: Option<usize>,
    describer: Option<usize>,
    /// The standard library's `FromText`, which `std.fromtext` defines, by its
    /// place in the file's traits, when the program has that module.
    from_text: Option<usize>,
    /// The standard library's `FromRequest`, which `std.http` defines, by its
    /// place in the file's traits, when the program has that module.
    from_request: Option<usize>,
    /// The `ReadError` and the `Problem` of `std.deserialize`, by their places
    /// in the file's structs and enums, when the program has that module.
    read_error: Option<usize>,
    problem: Option<usize>,
    /// The `Frame` of `std.task`, by its place in the file's structs, and its
    /// `Poll` and `Wait`, by their places in the file's enums, when the program
    /// has that module.
    frame: Option<usize>,
    task: Option<usize>,
    /// The structs of `std.net` that hold a handle the system opened, by their
    /// places in the file's structs, and the operation of `net` that ends each.
    handles: Vec<(usize, &'static str)>,
    /// The `File` of `std.fs`, by its place in the file's structs, when the program
    /// has that module.
    file_handle: Option<usize>,
    poll: Option<usize>,
    wait: Option<usize>,
    /// The prelude's `bool`, `Order`, `Option`, `Flow`, and `Result`, by their
    /// places in the file's enums.
    boolean: usize,
    order: usize,
    option: usize,
    flow: usize,
    result: usize,
    /// The prelude's `Range`, which `a..b` written as a value builds.
    range: usize,
    /// Each trait taken away from a struct or an enum by `without`.
    withouts: Vec<(usize, Def)>,
    /// How many defaults of type parameters are being resolved inside one
    /// another, so that a default that names its own type ends.
    default_depth: Cell<u8>,
    /// Whether the file has an alias, so that an error says a type as it is
    /// written where it is.
    has_alias: bool,
    /// The type a place needs and how the place writes it, while that place is
    /// checked, so that an error says the type as it is written there.
    written_here: RefCell<Option<(Ty, String)>>,
    /// How the program's own code wrote `i64` and `u8`, so that an error
    /// names a type as the program wrote it.
    spelled: Cell<Spelled>,
    /// The names and types of the parameters of the function whose signature
    /// is being read, which a bound of a range may be.
    bound_params: RefCell<Vec<(String, Type)>>,
}

/// For each whole number type that has two names, whether the first name the
/// program wrote for it is the new one (`Int`, `Byte`), once it wrote one.
#[derive(Clone, Copy, Default)]
struct Spelled {
    int: Option<bool>,
    byte: Option<bool>,
    nat: Option<bool>,
}

impl<'a> Defs<'a> {
    /// The function the name written at `pos` is: what the module that wrote
    /// it calls it, found among the file's functions.
    fn function(&self, pos: Pos, name: &str) -> Option<usize> {
        let here = module_of(pos);
        // The file that is built writes each name as it is, and a name of a
        // module is the same function wherever the module writes it, which a
        // debug build, that is asked what each token was written as, does not
        // hold to.
        if here == 0 || cfg!(debug_assertions) {
            return self.function_at(pos, name);
        }
        if let Some(found) = self
            .function_memo
            .borrow()
            .get(here)
            .and_then(|m| m.get(name))
        {
            return *found;
        }
        let found = self.function_at(pos, name);
        let mut memo = self.function_memo.borrow_mut();
        if memo.len() <= here {
            memo.resize_with(here + 1, Default::default);
        }
        memo[here].insert(name.into(), found);
        found
    }

    fn function_at(&self, pos: Pos, name: &str) -> Option<usize> {
        // The file that is built writes each name as it is.
        if pos.line < STEP && !cfg!(debug_assertions) {
            return self.function_names.get(name).copied();
        }
        let found = (self.resolution).find(pos, name, &|n| self.function_names.get(n).copied());
        // A function another module of the library defines is not what a name
        // means in a module that neither defines nor lists it, though the
        // program lists it, and so the file writes it as it is.
        let f = found?;
        let (here, at) = (module_of(pos), module_of(self.file.functions[f].pos));
        if here == 0 || at == here || at == 0 {
            return found;
        }
        self.lists(here, at, name).then_some(f)
    }

    /// Whether the module `here` lists the name from the module `at`, or imports
    /// that module whole.
    #[cold]
    fn lists(&self, here: usize, at: usize, name: &str) -> bool {
        (self.resolution.modules.get(&here)).is_some_and(|naming| {
            naming.whole.contains(&at)
                || (naming.imports.iter())
                    .any(|(w, to, home)| (w == name || to == name) && *home == at)
        })
    }

    /// The signature of a function, with the types of the names its lambdas read
    /// that it defines, once they are found.
    fn signature_of(&self, f: &Function) -> Signature {
        let mut scope = self.scope(&f.generics, &f.needs);
        self.add_implicit(&mut scope, f.params.iter().map(|p| &p.ty));
        self.signature(f, scope)
    }

    /// The struct that holds what a lambda bound to a name, stored in a field,
    /// or returned reads, if the parser made one for it.
    pub(crate) fn env_of(&self, l: &Lambda) -> Option<usize> {
        self.env_at.get(&l.pos).copied()
    }

    /// The lambda that struct `s` holds the values of what it reads, and the
    /// types of its parameters and of its result, the result last, among the
    /// type parameters of the struct that stand for those of its function.
    pub(crate) fn env_lambda(&self, s: usize) -> Option<&(&'a Lambda, Vec<Ty>)> {
        self.env_lambdas.get(s)?.as_ref()
    }

    /// The types of the parameters and of the result of the lambda that a value
    /// of struct `s`, with the type arguments `args`, holds, if `s` is one that
    /// holds what a lambda reads.
    pub(crate) fn env_items(&self, s: usize, args: &[Ty]) -> Option<Vec<Ty>> {
        let (_, items) = self.env_lambda(s)?;
        let def = &self.file.structs[s];
        let own = def.generics.len() - def.fields.len();
        Some(items.iter().map(|t| t.subst(&args[..own])).collect())
    }

    /// The struct the name written at `pos` is, as `function` finds a function.
    fn struct_at(&self, pos: Pos, name: &str) -> Option<usize> {
        match self.type_at(pos, name) {
            Some(Item::Struct(s)) => Some(s),
            _ => None,
        }
    }

    /// The enum the name written at `pos` is.
    fn enum_at(&self, pos: Pos, name: &str) -> Option<usize> {
        match self.type_at(pos, name) {
            Some(Item::Enum(e)) => Some(e),
            _ => None,
        }
    }

    /// The trait the name written at `pos` is.
    fn trait_at(&self, pos: Pos, name: &str) -> Option<usize> {
        match self.type_at(pos, name) {
            Some(Item::Trait(k)) => Some(k),
            _ => None,
        }
    }

    /// The struct that the file calls `name`.
    fn struct_named(&self, name: &str) -> Option<usize> {
        match self.types.get(name) {
            Some(Item::Struct(s)) => Some(*s),
            _ => None,
        }
    }

    /// The enum that the file calls `name`.
    fn enum_named(&self, name: &str) -> Option<usize> {
        match self.types.get(name) {
            Some(Item::Enum(e)) => Some(*e),
            _ => None,
        }
    }

    /// The trait that the file calls `name`.
    fn trait_named(&self, name: &str) -> Option<usize> {
        match self.types.get(name) {
            Some(Item::Trait(k)) => Some(*k),
            _ => None,
        }
    }

    /// The struct, enum, or trait that the module that wrote `name` at `pos`
    /// calls it.
    fn type_at(&self, pos: Pos, name: &str) -> Option<Item> {
        self.types.at(pos, name)
    }

    /// The functions of traits that the name written at `pos` is.
    fn trait_fns_at(&self, pos: Pos, name: &str) -> Option<&Vec<(usize, usize)>> {
        self.by_scope(&self.trait_fns, pos, name)
    }

    /// The item of `table` that the name written at `pos` is, which in a
    /// debug build is asked of the module that wrote it too.
    fn by_scope<'t, V>(&self, table: &'t Map<&'a str, V>, pos: Pos, name: &str) -> Option<&'t V> {
        let found = table.get(name);
        if cfg!(debug_assertions) {
            let key = self.resolution.key(pos, name, &|n| table.contains_key(n));
            assert_eq!(
                found.map(std::ptr::from_ref),
                table.get(&*key).map(std::ptr::from_ref),
                "`{name}` at {pos:?}, key `{key}`, written `{}`",
                self.resolution.written_at(pos, name)
            );
        }
        found
    }

    /// The number a bound of the range type `t` stands for where only the
    /// range of the bound is known: a number is itself, and a name is the
    /// least number of its parameter if it is the first bound of the range
    /// and the greatest if it is the end.
    fn bound_value(
        &self,
        bound: &Bound,
        first: bool,
        t: &Type,
        scope: &Scope,
        errors: &mut Vec<Error>,
    ) -> Option<i128> {
        let name = match bound {
            Bound::Number(n) => return Some(*n),
            // The size of a name is a `Nat`, whatever the name is.
            Bound::Size(_) => return Some(if first { 0 } else { u64::MAX as i128 }),
            Bound::Name(name) => name,
        };
        let params = self.bound_params.take();
        let found = params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.clone());
        let ty = found.map(|p| self.resolve(&p, scope, errors));
        *self.bound_params.borrow_mut() = params;
        match ty {
            Some(Ty::Int(int)) => Some(if first { int.min() } else { int.max() }),
            Some(Ty::Unknown) | None => None,
            Some(_) => {
                errors.push(Error::new(
                    t.pos,
                    format!("`{name}` is not a whole number, and a bound is one"),
                ));
                None
            }
        }
    }

    /// The type a type written in the source stands for, among the type
    /// parameters of `scope`. Mistakes go in `errors`.
    fn resolve(&self, t: &Type, scope: &Scope, errors: &mut Vec<Error>) -> Ty {
        let name = t.name.as_str();
        if t.unbuilt() {
            return Ty::Unknown;
        }
        // A statement is a type a proof is a value of, and a proof is erased.
        if t.statement.is_some() {
            return Ty::Nothing;
        }
        if let Some((first, end)) = &t.range {
            let (Some(min), Some(end)) = (
                self.bound_value(first, true, t, scope, errors),
                self.bound_value(end, false, t, scope, errors),
            ) else {
                return Ty::Unknown;
            };
            // A range that no number is in is of the least bound.
            return match Int::between(min, (end - 1).max(min - 1)) {
                Some(int) => Ty::Int(int),
                None => {
                    errors.push(Error::new(
                        t.pos,
                        format!(
                            "`{}` needs more than 64 bits, which is not built yet",
                            t.written()
                        ),
                    ));
                    Ty::Unknown
                }
            };
        }
        if let Some(length) = t.length {
            return match self.resolve(&t.args[0], scope, errors) {
                Ty::Unknown => Ty::Unknown,
                Ty::Fn(_) | Ty::Nothing => {
                    errors.push(Error::unsupported(
                        t.pos,
                        "an array of functions or of `()`",
                    ));
                    Ty::Unknown
                }
                element => Ty::Array(vec![element], length),
            };
        }
        if name.is_empty() {
            let items: Vec<Ty> = (t.args.iter())
                .map(|a| self.resolve(a, scope, errors))
                .collect();
            return if t.function {
                Ty::func(items)
            } else {
                Ty::tuple(items)
            };
        }
        let no_args = |what: &str, errors: &mut Vec<Error>| {
            if !t.args.is_empty() {
                errors.push(Error::new(
                    t.args[0].pos,
                    format!("{what} `{name}` takes no type arguments"),
                ));
            }
        };
        if let Some(p) = scope.names.iter().position(|n| n == name) {
            no_args("the type parameter", errors);
            return if t.args.is_empty() {
                Ty::Param(p)
            } else {
                Ty::Unknown
            };
        }
        let named = self.type_at(t.pos, name);
        if let Some(Item::Struct(s)) = named {
            let args: Vec<Ty> = t
                .args
                .iter()
                .map(|a| self.resolve(a, scope, errors))
                .collect();
            let Some(args) =
                self.arguments_of(&self.file.structs[s].generics, args, name, t, errors)
            else {
                return Ty::Unknown;
            };
            // An alias is the type it names, with its arguments in place of its
            // type parameters.
            if let Some(named) = &self.file.structs[s].alias {
                if self.cyclic[s] {
                    return Ty::Unknown;
                }
                let inner = self.quietly(named, &self.scope(&self.file.structs[s].generics, &[]));
                return inner.subst(&args);
            }
            let mut args = args;
            let held = self.implicit[s];
            if held > 0 {
                let Some(&first) = scope.implicit_at.get(&t.pos) else {
                    errors.push(Error::unsupported(
                        t.pos,
                        &format!("`{name}`, which holds a function, written anywhere but in a parameter's type or a field's type"),
                    ));
                    return Ty::Unknown;
                };
                args.extend((first..first + held).map(Ty::Param));
            }
            return if self.cyclic[s] {
                Ty::Unknown
            } else {
                Ty::structure(s, args)
            };
        }
        if let Some(Item::Enum(e)) = named {
            let args: Vec<Ty> = t
                .args
                .iter()
                .map(|a| self.resolve(a, scope, errors))
                .collect();
            let Some(args) = self.arguments_of(&self.file.enums[e].generics, args, name, t, errors)
            else {
                return Ty::Unknown;
            };
            return if self.cyclic_enums[e] {
                Ty::Unknown
            } else {
                Ty::enumeration(e, args)
            };
        }
        if name == "List" {
            if t.args.len() != 1 {
                errors.push(Error::new(
                    t.pos,
                    format!(
                        "`List` takes one type argument, and this gives {}",
                        t.args.len()
                    ),
                ));
                return Ty::Unknown;
            }
            return match self.resolve(&t.args[0], scope, errors) {
                Ty::Unknown => Ty::Unknown,
                Ty::Fn(_) | Ty::Nothing => {
                    errors.push(Error::unsupported(t.pos, "a list of functions or of `()`"));
                    Ty::Unknown
                }
                element => Ty::list(element),
            };
        }
        if name == "Slice" && !matches!(named, Some(Item::Struct(_))) {
            if t.args.len() != 1 {
                errors.push(Error::new(
                    t.pos,
                    format!(
                        "`Slice` takes one type argument, and this gives {}",
                        t.args.len()
                    ),
                ));
                return Ty::Unknown;
            }
            return match self.resolve(&t.args[0], scope, errors) {
                Ty::Unknown => Ty::Unknown,
                Ty::Fn(_) | Ty::Nothing => {
                    errors.push(Error::unsupported(t.pos, "a slice of functions or of `()`"));
                    Ty::Unknown
                }
                element => Ty::slice(element),
            };
        }
        if let Some(float) = Float::named(name) {
            no_args("the type", errors);
            return if t.args.is_empty() {
                Ty::Float(float)
            } else {
                Ty::Unknown
            };
        }
        if name == TEXT || Int::named(name).is_some() {
            no_args("the type", errors);
            if !of_prelude(t.pos) && !of_std(t.pos) {
                self.spell(name);
            }
            return match (t.args.is_empty(), Int::named(name)) {
                (false, _) => Ty::Unknown,
                (_, Some(int)) => Ty::Int(int),
                (_, None) => Ty::Text,
            };
        }
        // The name of a function, where a type is expected, is the type whose
        // one value it is.
        if t.args.is_empty()
            && let Some(f) = self.function(t.pos, name)
        {
            return Ty::Named(f);
        }
        errors.push(if self.trait_at(t.pos, name).is_some() {
            Error::new(t.pos, format!("`{name}` is a trait, which is not a type"))
        } else if TYPES_NOT_BUILT.contains(&name) {
            Error::unsupported(t.pos, &format!("the type `{name}`"))
        } else {
            Error::new(t.pos, format!("there is no type named `{name}`"))
        });
        Ty::Unknown
    }

    /// The fields of a struct that hold data, by their places: those that are
    /// not a proof, which is erased and is in no value.
    fn data_fields(&self, s: usize) -> Vec<usize> {
        (0..self.file.structs[s].fields.len())
            .filter(|&i| self.file.structs[s].fields[i].ty.statement.is_none())
            .collect()
    }

    /// The types of the data fields of a struct, among the types it is given.
    fn data_types(&self, s: usize, args: &[Ty]) -> Vec<Ty> {
        (self.data_fields(s).into_iter())
            .map(|i| self.fields[s][i].subst(args))
            .collect()
    }

    /// The prelude's `bool`.
    fn bool_ty(&self) -> Ty {
        Ty::Enum(self.boolean, Vec::new())
    }

    /// Whether the type is the prelude's `Result`, whose failure must not be
    /// dropped.
    fn is_result(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Enum(e, _) => *e == self.result,
            _ => false,
        }
    }

    /// The types of a type's parameters, given the arguments written for
    /// it: the defaults of the parameters left out are the rest. A count
    /// the type does not take is reported, and gives `None`.
    fn arguments_of(
        &self,
        generics: &[Generic],
        mut args: Vec<Ty>,
        name: &str,
        t: &Type,
        errors: &mut Vec<Error>,
    ) -> Option<Vec<Ty>> {
        // The functions a struct holds are not written.
        let generics = &generics[..generics
            .iter()
            .take_while(|g| !g.name.contains('@'))
            .count()];
        let wanted = generics.len();
        let needed = generics.iter().filter(|g| g.default.is_none()).count();
        if args.len() < needed || args.len() > wanted {
            let takes = if needed == wanted {
                count(wanted, "type argument")
            } else {
                format!("{needed} to {}", count(wanted, "type argument"))
            };
            errors.push(Error::new(
                t.pos,
                format!("`{name}` takes {takes}, and this gives {}", args.len()),
            ));
            return None;
        }
        self.fill_defaults(generics, &mut args);
        Some(args)
    }

    /// Adds the type of each parameter of `generics` that `args` does not
    /// reach, from its default, with the parameters before it as `args` has
    /// them.
    fn fill_defaults(&self, generics: &[Generic], args: &mut Vec<Ty>) {
        if args.len() >= generics.len() {
            return;
        }
        let depth = self.default_depth.get();
        let scope = self.scope(generics, &[]);
        for g in &generics[args.len()..] {
            let ty = match &g.default {
                Some(d) if depth < 8 => {
                    self.default_depth.set(depth + 1);
                    let ty = self.quietly(d, &scope);
                    self.default_depth.set(depth);
                    let mut known = args.clone();
                    known.resize(generics.len(), Ty::Unknown);
                    ty.subst(&known)
                }
                _ => Ty::Unknown,
            };
            args.push(ty);
        }
    }

    /// Notes how the program wrote a whole number type, when it is one of the
    /// two that have two names and the program has not written either yet.
    fn spell(&self, name: &str) {
        let mut spelled = self.spelled.get();
        match name {
            "Int" | "i64" => spelled.int.get_or_insert(name == "Int"),
            "Byte" | "u8" => spelled.byte.get_or_insert(name == "Byte"),
            "Nat" | "u64" => spelled.nat.get_or_insert(name == "Nat"),
            _ => return,
        };
        self.spelled.set(spelled);
    }

    /// How a whole number type is written in an error: `Int` and `Byte` where
    /// the program wrote those names first, and the name of the type
    /// otherwise.
    fn spelling(&self, int: Int) -> String {
        let spelled = self.spelled.get();
        match int {
            Int::I64 if spelled.int == Some(true) => "Int".to_string(),
            Int::U8 if spelled.byte == Some(true) => "Byte".to_string(),
            Int::U64 if spelled.nat == Some(true) => "Nat".to_string(),
            _ => int.name(),
        }
    }

    /// The type, with nothing reported.
    fn quietly(&self, t: &Type, scope: &Scope) -> Ty {
        self.resolve(t, scope, &mut Vec::new())
    }

    /// Whether the name is that of a type, other than a type parameter.
    fn is_type(&self, name: &str) -> bool {
        matches!(self.types.get(name), Some(Item::Struct(_) | Item::Enum(_)))
            || name == TEXT
            || name == "Slice"
            || Int::named(name).is_some()
            || TYPES_NOT_BUILT.contains(&name)
    }

    /// The name of a struct or an enum.
    fn type_name(&self, ty: &Ty) -> &'a str {
        match ty {
            Ty::Struct(s, _) => &self.file.structs[*s].name,
            Ty::Enum(e, _) => &self.file.enums[*e].name,
            _ => unreachable!("a struct or an enum"),
        }
    }

    /// The place of the case named `name` of enum `e`, if it has one.
    fn case_of(&self, e: usize, name: &str) -> Option<usize> {
        self.file.enums[e].cases.iter().position(|c| c.name == name)
    }

    /// The enum and its case, if the expression names a case in full, as
    /// in `Pet.none`, where `names` are the names defined.
    fn qualified_case(&self, of: &Expr, names: &[(String, Ty)]) -> Option<usize> {
        let Expr::Name { name, pos } = of else {
            return None;
        };
        if names.iter().any(|(n, _)| n == name) {
            return None;
        }
        self.enum_at(*pos, name)
    }

    /// A function's signature among the type parameters of `scope`.
    fn signature(&self, f: &Function, scope: Scope) -> Signature {
        *self.bound_params.borrow_mut() = (f.params.iter())
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect();
        let signature = self.signature_read(f, scope);
        self.bound_params.borrow_mut().clear();
        signature
    }

    /// The parameters of trait `k` whose default is an earlier parameter, each
    /// with the one it defaults to, as `B = A` of `Eq` does.
    fn defaulted(&self, k: usize) -> Vec<(usize, usize)> {
        let generics = &self.file.traits[k].generics;
        (generics.iter().enumerate())
            .filter_map(|(i, g)| {
                if self.outs[k][i] {
                    return None;
                }
                let d = g.default.as_ref()?;
                let j = generics.iter().position(|other| other.name == d.name)?;
                (j < i).then_some((i, j))
            })
            .collect()
    }

    /// Runs `check` with errors saying `ty` as `written`, the way the place that
    /// needs it writes it, when the file has an alias.
    fn writing<R>(&self, ty: &Ty, written: Option<String>, check: impl FnOnce() -> R) -> R {
        let Some(written) = written.filter(|_| self.has_alias) else {
            return check();
        };
        let saved = self.written_here.replace(Some((ty.clone(), written)));
        let result = check();
        self.written_here.replace(saved);
        result
    }

    fn signature_read(&self, f: &Function, scope: Scope) -> Signature {
        Signature {
            written: (f.params.iter())
                .map(|p| self.has_alias.then(|| written_type(&p.ty)))
                .collect(),
            params: (f.params.iter())
                .map(|p| (p.name.clone(), self.quietly(&p.ty, &scope)))
                .collect(),
            defaults: f.params.iter().map(|p| p.default.is_some()).collect(),
            named_from: Some(
                (f.params.iter())
                    .position(|p| p.by_name)
                    .unwrap_or(f.params.len()),
            ),
            param_defaults: (f.generics.iter())
                .map(|g| g.default.as_ref().map(|d| self.quietly(d, &scope)))
                .collect(),
            gives: match &f.result {
                Some(t) if t.function => match self.returned_function(f, &scope) {
                    Some(held) => held,
                    // A function that is a parameter gives the function it is given.
                    None if self.returned_parameter(f).is_some() => self.quietly(t, &scope),
                    None => Ty::Unknown,
                },
                Some(t) if self.implicit_result(t, &scope).is_some() => {
                    let u = self.implicit_result(t, &scope).expect("checked");
                    (self.returned_struct(f, u, &scope).map(|(t, _)| t)).unwrap_or(Ty::Unknown)
                }
                Some(t) => self.quietly(t, &scope),
                None => Ty::Nothing,
            },
            gives_param: match &f.result {
                Some(t) if t.function && self.returned_function(f, &scope).is_none() => {
                    self.returned_parameter(f)
                }
                _ => None,
            },
            gives_held: match &f.result {
                Some(t) => match self.implicit_result(t, &scope) {
                    Some(u) => self
                        .returned_struct(f, u, &scope)
                        .map(|(_, from)| from)
                        .unwrap_or_default(),
                    None => Vec::new(),
                },
                None => Vec::new(),
            },
            effects: {
                let mut built: Vec<String> = Vec::new();
                for (e, _) in &f.effects {
                    if (BUILT.contains(&e.as_str()) || self.declares(e)) && !built.contains(e) {
                        built.push(e.clone());
                    }
                }
                built
            },
            // Few parameters are functions that carry an effect, and a function
            // with none has no list to make.
            fn_effects: if f.params.iter().any(|p| !p.effects.is_empty()) {
                (f.params.iter())
                    .map(|p| p.effects.iter().map(|(e, _)| e.clone()).collect())
                    .collect()
            } else {
                Vec::new()
            },
            scope,
        }
    }

    /// The function with a name that `f` gives, when its result is a function
    /// type: the one that each end of its body gives, which is the same in each
    /// block of an `if` or a `match` that ends it. Nothing when that is not one
    /// function, as which function it is would depend on what the program does.
    pub(crate) fn returned_function(&self, f: &Function, scope: &Scope) -> Option<Ty> {
        match self.ends_of(f, scope)?.as_slice() {
            [Holds::Func(t @ (Ty::Named(_) | Ty::Struct(..))), rest @ ..]
                if rest.iter().all(|h| *h == Holds::Func(t.clone())) =>
            {
                Some(t.clone())
            }
            _ => None,
        }
    }

    /// The parameter of a function type that `f` gives, when each end of its
    /// body is that parameter.
    pub(crate) fn returned_parameter(&self, f: &Function) -> Option<usize> {
        let at = |e: &Expr| match e {
            Expr::Name { name, .. } => f
                .params
                .iter()
                .position(|p| &p.name == name && p.ty.function),
            _ => None,
        };
        let ends = tails(&f.body);
        let first = at(ends.first()?)?;
        ends.iter().all(|e| at(e) == Some(first)).then_some(first)
    }

    /// What each end of the body of `f` gives, among what is known of the
    /// functions it holds.
    fn ends_of(&self, f: &Function, scope: &Scope) -> Option<Vec<Holds>> {
        self.ends(&f.body, &mut Vec::new(), Some((f, scope)), 0)
    }

    /// What each end of a block gives, when each is a function with a name, a
    /// struct built of them, or a name bound to one of those, in `known`.
    /// `own` is the function the block is the body of, when its parameters are
    /// read from their types, and not from `known`.
    fn ends(
        &self,
        body: &[Stmt],
        known: &mut Vec<(String, Holds)>,
        own: Option<(&Function, &Scope)>,
        depth: usize,
    ) -> Option<Vec<Holds>> {
        let before = known.len();
        let mut found = Vec::new();
        for (i, stmt) in body.iter().enumerate() {
            let last = i + 1 == body.len();
            match stmt {
                Stmt::Define { name, value, .. } if !last => {
                    if let Some(h) = self.holds(value, known, own, depth) {
                        known.push((name.clone(), h));
                    }
                }
                Stmt::Expr(Expr::Match(m)) if last => {
                    for arm in &m.arms {
                        found.extend(self.ends(&arm.body, known, own, depth)?);
                    }
                }
                Stmt::Expr(e) if last => found.push(self.holds(e, known, own, depth)?),
                _ => {}
            }
        }
        known.truncate(before);
        (!found.is_empty()).then_some(found)
    }

    /// The functions an expression holds, when it is a function with a name, a
    /// construction of a struct that holds functions from such, a name bound to
    /// one, a parameter of a struct type, or a call of a function that gives
    /// one of these.
    fn holds(
        &self,
        e: &Expr,
        known: &[(String, Holds)],
        own: Option<(&Function, &Scope)>,
        depth: usize,
    ) -> Option<Holds> {
        match e {
            Expr::Name { name, pos } => {
                if let Some((_, h)) = known.iter().rfind(|(n, _)| n == name) {
                    return Some(h.clone());
                }
                if let Some((f, scope)) = own
                    && let Some(at) = f.params.iter().position(|p| &p.name == name)
                {
                    let p = &f.params[at];
                    // A function that is a parameter is the function of the argument.
                    if p.ty.function {
                        return Some(Holds::Func(param_slot(at)));
                    }
                    return match self.quietly(&p.ty, scope) {
                        Ty::Struct(u, args) if self.implicit[u] > 0 => {
                            let at = args.len() - self.implicit[u];
                            Some(Holds::Struct(u, args[at..].to_vec()))
                        }
                        _ => None,
                    };
                }
                Some(Holds::Func(Ty::Named(self.function(*pos, name)?)))
            }
            // A lambda that reads names holds the values of those of them that are
            // parameters, whose types are written, which a function that gives it
            // is known by before its body is checked. A lambda that is bound holds
            // what is lent as it is, so only one that reads what is copied is given.
            Expr::Lambda(l) => {
                let s = self.env_of(l)?;
                let (f, scope) = own?;
                let def = &self.file.structs[s];
                let mut args: Vec<Ty> = (0..def.generics.len() - def.fields.len())
                    .map(Ty::Param)
                    .collect();
                for (i, field) in def.fields.iter().enumerate() {
                    let ty = match f.params.iter().find(|p| p.name == field.name) {
                        // What is lent is not given, as a value that is lent is not
                        // returned, unless the function takes it.
                        Some(p)
                            if p.take
                                || ownership::copies(self, scope, &self.quietly(&p.ty, scope)) =>
                        {
                            self.quietly(&p.ty, scope)
                        }
                        Some(_) => return None,
                        // The type the body gives a name it defines, once found.
                        None => self.env_types.get(&s)?.get(i)?.clone(),
                    };
                    if ty.holds_fn() || ty.unknown() {
                        return None;
                    }
                    args.push(ty);
                }
                Some(Holds::Func(Ty::Struct(s, args)))
            }
            Expr::Call(call) if call.qualifier.is_none() && call.through.is_none() => {
                if let Some(u) = self.struct_at(call.name_pos, &call.callee) {
                    return self.built_holds(call, u, known, own, depth);
                }
                let g = self.function(call.name_pos, &call.callee)?;
                if depth >= 4 {
                    return None;
                }
                let def = &self.file.functions[g];
                let mut inner = Vec::new();
                let places = params_of(
                    &def.params
                        .iter()
                        .map(|p| (p.name.clone(), Ty::Unknown))
                        .collect::<Vec<_>>(),
                    (def.params.iter())
                        .position(|p| p.by_name)
                        .unwrap_or(def.params.len()),
                    &call.args,
                )?;
                for (a, &p) in places.iter().enumerate() {
                    if let Some(h) = self.holds(&call.args[a].value, known, own, depth) {
                        inner.push((def.params[p].name.clone(), h));
                    }
                }
                match self
                    .ends(&def.body, &mut inner, None, depth + 1)?
                    .as_slice()
                {
                    [first, rest @ ..] if rest.iter().all(|h| h == first) => Some(first.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The functions a construction of struct `u` holds: those of its fields
    /// of a function type, and then those of the structs its fields hold.
    fn built_holds(
        &self,
        call: &Call,
        u: usize,
        known: &[(String, Holds)],
        own: Option<(&Function, &Scope)>,
        depth: usize,
    ) -> Option<Holds> {
        let def = &self.file.structs[u];
        if def.generics.iter().any(|g| !g.name.contains('@')) {
            return None;
        }
        let places = params_of(
            &def.fields
                .iter()
                .map(|d| (d.name.clone(), Ty::Unknown))
                .collect::<Vec<_>>(),
            def.fields.len(),
            &call.args,
        )?;
        let value_of = |field: usize| {
            let a = places.iter().position(|&p| p == field)?;
            self.holds(&call.args[a].value, known, own, depth)
        };
        let mut tail = Vec::new();
        for g in &def.generics {
            let field = def
                .fields
                .iter()
                .position(|d| d.name == g.name.trim_end_matches("@function"))?;
            match value_of(field)? {
                Holds::Func(t) => tail.push(t),
                Holds::Struct(..) => return None,
            }
        }
        for (field, d) in def.fields.iter().enumerate() {
            if d.function.is_some() {
                continue;
            }
            if matches!(self.type_at(d.ty.pos, &d.ty.name), Some(Item::Struct(v)) if self.implicit[v] > 0)
                && !d.ty.name.is_empty()
            {
                match value_of(field)? {
                    Holds::Struct(_, more) => tail.extend(more),
                    Holds::Func(_) => return None,
                }
            }
        }
        Some(Holds::Struct(u, tail))
    }

    /// The struct, among those that hold a function, that the type `t` written
    /// as a result names, when it is written with no more than its own type
    /// arguments.
    pub(crate) fn implicit_result(&self, t: &Type, scope: &Scope) -> Option<usize> {
        match self.type_at(t.pos, &t.name) {
            Some(Item::Struct(u))
                if !t.name.is_empty()
                    && !t.function
                    && !scope.names.contains(&t.name)
                    && self.implicit[u] > 0 =>
            {
                Some(u)
            }
            _ => None,
        }
    }

    /// The type of struct `u` that `f` gives, when its result is written as `u`,
    /// which holds functions: the functions that each end of the body holds,
    /// the same at each end.
    pub(crate) fn returned_struct(
        &self,
        f: &Function,
        u: usize,
        scope: &Scope,
    ) -> Option<(Ty, Vec<(usize, usize)>)> {
        let def = &self.file.structs[u];
        if def.generics.iter().any(|g| !g.name.contains('@')) {
            return None;
        }
        match self.ends_of(f, scope)?.as_slice() {
            [Holds::Struct(v, tail), rest @ ..]
                if *v == u && rest.iter().all(|h| *h == Holds::Struct(u, tail.clone())) =>
            {
                // A function that a parameter is is the argument's at each call.
                let from: Vec<(usize, usize)> = (tail.iter().enumerate())
                    .filter_map(|(at, t)| slot_param(t).map(|i| (at, i)))
                    .collect();
                let tail = tail
                    .iter()
                    .map(|t| {
                        if slot_param(t).is_some() {
                            Ty::Nothing
                        } else {
                            t.clone()
                        }
                    })
                    .collect();
                Some((Ty::structure(u, tail), from))
            }
            _ => None,
        }
    }

    /// The types of the parameters of function `f`, followed by the type it
    /// gives, as a function type holds them.
    fn function_items(&self, f: usize) -> Vec<Ty> {
        let sig = &self.signatures[f];
        let mut items: Vec<Ty> = sig.params.iter().map(|(_, t)| t.clone()).collect();
        items.push(sig.gives.clone());
        items
    }

    /// Whether a trait other than `Copy` and `Discard` bounds the type
    /// parameter `p` of `scope`.
    fn bounded(&self, scope: &Scope, p: usize) -> bool {
        scope.requires.iter().any(|r| {
            r.types.contains(&Ty::Param(p))
                && !matches!(self.file.traits[r.of].name.as_str(), "Copy" | "Discard")
        })
    }

    /// How a type is written, with the type parameters of `scope`.
    fn text(&self, ty: &Ty, scope: &Scope) -> String {
        if let Some((_, name)) = self.written_here.borrow().as_ref().filter(|(t, _)| t == ty) {
            return name.clone();
        }
        match ty {
            Ty::Text => TEXT.to_string(),
            Ty::Int(int) => self.spelling(*int).to_string(),
            Ty::Float(float) => float.written().to_string(),
            // A lambda that holds the values of what it reads is of the function
            // type it has, as no source writes the struct that holds them.
            Ty::Struct(s, args) if self.env_lambda(*s).is_some() => {
                let items = self.env_items(*s, args).expect("checked");
                let items: Vec<String> = items.iter().map(|t| self.text(t, scope)).collect();
                function_text(&items)
            }
            Ty::Struct(_, args) | Ty::Enum(_, args) => {
                let name = self.type_name(ty);
                if args.is_empty() {
                    name.to_string()
                } else {
                    let args: Vec<String> = args.iter().map(|t| self.text(t, scope)).collect();
                    format!("{name}[{}]", args.join(", "))
                }
            }
            Ty::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|t| self.text(t, scope)).collect();
                format!("({})", items.join(", "))
            }
            Ty::List(items) => format!("List[{}]", self.text(&items[0], scope)),
            Ty::Slice(items) => format!("Slice[{}]", self.text(&items[0], scope)),
            Ty::Array(items, n) => format!("[{n}]{}", self.text(&items[0], scope)),
            Ty::Fn(items) => {
                let items: Vec<String> = items.iter().map(|t| self.text(t, scope)).collect();
                function_text(&items)
            }
            Ty::Param(p) => scope.names[*p].clone(),
            Ty::Named(f) => written_name(&self.file.functions[*f].name).to_string(),
            Ty::Nothing => "()".to_string(),
            Ty::Unknown => "nothing".to_string(),
            Ty::Flex(_) => "number".to_string(),
        }
    }

    /// What a value of the type is, as an error describes it.
    fn describe(&self, ty: &Ty, scope: &Scope) -> String {
        match ty {
            Ty::Text => "text".to_string(),
            Ty::Nothing | Ty::Unknown => "nothing".to_string(),
            Ty::Struct(s, _) if self.env_lambda(*s).is_some() => {
                "a lambda that reads names of its function".to_string()
            }
            ty => format!("a `{}`", self.text(ty, scope)),
        }
    }

    /// What a value of `pattern` is, where the type parameters of `vars`
    /// that have a type in `sol` stand for it.
    fn describe_pattern(
        &self,
        pattern: &Ty,
        sol: &[Option<Ty>],
        vars: &Scope,
        scope: &Scope,
    ) -> String {
        fn text(defs: &Defs, t: &Ty, sol: &[Option<Ty>], vars: &Scope, scope: &Scope) -> String {
            match t {
                Ty::Param(i) => match &sol[*i] {
                    Some(t) => defs.text(t, scope),
                    None => vars.names[*i].clone(),
                },
                Ty::Tuple(items) => {
                    let items: Vec<String> = (items.iter())
                        .map(|a| text(defs, a, sol, vars, scope))
                        .collect();
                    format!("({})", items.join(", "))
                }
                Ty::Fn(items) => {
                    let items: Vec<String> = (items.iter())
                        .map(|a| text(defs, a, sol, vars, scope))
                        .collect();
                    function_text(&items)
                }
                Ty::Struct(_, args) | Ty::Enum(_, args) if !args.is_empty() => {
                    let args: Vec<String> = args
                        .iter()
                        .map(|a| text(defs, a, sol, vars, scope))
                        .collect();
                    format!("{}[{}]", defs.type_name(t), args.join(", "))
                }
                Ty::List(args) => format!("List[{}]", text(defs, &args[0], sol, vars, scope)),
                Ty::Slice(args) => format!("Slice[{}]", text(defs, &args[0], sol, vars, scope)),
                t => defs.text(t, scope),
            }
        }
        match pattern.solved(sol) {
            Ty::Text => "text".to_string(),
            Ty::Nothing => "nothing".to_string(),
            _ => format!("a `{}`", text(self, pattern, sol, vars, scope)),
        }
    }
}

/// A count of things, as a sentence gives it.
fn count(n: usize, thing: &str) -> String {
    if n == 1 {
        format!("1 {thing}")
    } else {
        format!("{n} {thing}s")
    }
}

/// A list of names, each in backticks, joined as a sentence joins them.
fn names(list: &[&str]) -> String {
    sentence(list.iter().map(|n| format!("`{n}`")).collect())
}

/// How a parameter is spoken of in an error: by its name, in backticks, or
/// by its place when it is written as a pattern and has no name of its own,
/// as the parser gives it one that holds an `@`.
fn param_words(name: &str, place: usize) -> String {
    const ORDINALS: [&str; 10] = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
        "tenth",
    ];
    if !name.contains('@') {
        return format!("`{name}`");
    }
    match ORDINALS.get(place) {
        Some(ordinal) => format!("the {ordinal} parameter"),
        None => format!("parameter {}", place + 1),
    }
}

/// Parts of a sentence, joined as a sentence joins them.
fn sentence(quoted: Vec<String>) -> String {
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// A function type written from the text of its parameters and its result,
/// the result last: the result is left out when it is `()`.
fn function_text(items: &[String]) -> String {
    let (gives, params) = items.split_last().expect("a result");
    if gives == "()" {
        format!("fn({})", params.join(", "))
    } else {
        format!("fn({}) -> {gives}", params.join(", "))
    }
}

/// How a type is written in the source.
fn written_type(t: &Type) -> String {
    t.written()
}

/// A function's signature as it is written, with `effects` in place of
/// its list of effects.
fn written(f: &Function, effects: &[&str]) -> String {
    let generics: Vec<String> = (f.generics.iter())
        .map(|g| match &g.bound {
            Some(b) => format!("{}: {}", g.name, written_type(b)),
            None => g.name.clone(),
        })
        .collect();
    let params: Vec<String> = (f.params.iter())
        .map(|p| {
            let mutable = if p.mutable { "mut " } else { "" };
            let mut text = format!("{}: {mutable}{}", p.name, written_type(&p.ty));
            if !p.effects.is_empty() {
                let effects: Vec<&str> = p.effects.iter().map(|(e, _)| e.as_str()).collect();
                text.push_str(&format!(" <{}>", effects.join(", ")));
            }
            if let Some(value) = &p.default {
                text.push_str(&format!(" = {}", written_expr(value)));
            }
            text
        })
        .collect();
    let mut text = format!("fn {}", f.name);
    if !generics.is_empty() {
        text.push_str(&format!("[{}]", generics.join(", ")));
    }
    // The parameters given by name come after a `;`.
    let split = f
        .params
        .iter()
        .position(|p| p.by_name)
        .unwrap_or(params.len());
    let (by_position, by_name) = params.split_at(split);
    let mut list = by_position.join(", ");
    if !by_name.is_empty() {
        list.push_str("; ");
        list.push_str(&by_name.join(", "));
    }
    text.push_str(&format!("({list})"));
    // The result a function that changes its parameters is given when it is
    // lowered is not written, and what it gives besides them is.
    let result = match crate::parser::gives_back(f) {
        true => crate::parser::own_result(f),
        false => f.result.as_ref(),
    };
    if let Some(t) = result {
        text.push_str(&format!(" -> {}", written_type(t)));
    }
    if !effects.is_empty() {
        text.push_str(&format!(" <{}>", effects.join(", ")));
    }
    text
}

/// How a call is written in the source.
fn written_call(call: &Call) -> String {
    let args: Vec<String> = (call.args.iter())
        .filter(|a| !a.after)
        .map(|a| match &a.name {
            Some((name, _)) => format!("{name}: {}", written_expr(&a.value)),
            None => written_expr(&a.value),
        })
        .collect();
    format!("{}({})", call.callee, args.join(", "))
}

/// How an expression is written in the source, with each text written with
/// an escape for every character that needs one.
fn written_expr(e: &Expr) -> String {
    written_with(&|_| None, e)
}

/// How `e` is written, where a name that `s` gives text for is written as it.
fn written_with(s: &dyn Fn(&str) -> Option<String>, e: &Expr) -> String {
    match e {
        Expr::Number { text, negative, .. } | Expr::Float { text, negative, .. } => {
            format!("{}{text}", if *negative { "-" } else { "" })
        }
        Expr::Text { text, .. } => {
            let mut out = String::from("\"");
            for c in String::from_utf8_lossy(text).chars() {
                match c {
                    '"' | '\\' | '{' | '}' => {
                        out.push('\\');
                        out.push(c);
                    }
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        Expr::Interp { parts, .. } => {
            let mut out = String::from("\"");
            for part in parts {
                match part {
                    Part::Text(text) => {
                        let quoted = written_with(
                            s,
                            &Expr::Text {
                                text: text.clone(),
                                pos: Pos::START,
                            },
                        );
                        out.push_str(&quoted[1..quoted.len() - 1]);
                    }
                    Part::Expr(e) => {
                        let Expr::Call(call) = e else {
                            unreachable!("the call of `Fmt.fmt`")
                        };
                        out.push_str(&format!("{{{}}}", written_with(s, &call.args[0].value)));
                    }
                }
            }
            out.push('"');
            out
        }
        Expr::Name { name, .. } => s(name).unwrap_or_else(|| name.clone()),
        Expr::Call(call) => {
            let args: Vec<String> = (call.args.iter())
                .filter(|a| !a.after)
                .map(|a| match &a.name {
                    Some((name, _)) => format!("{name}: {}", written_with(s, &a.value)),
                    None => written_with(s, &a.value),
                })
                .collect();
            let mut text = match &call.qualifier {
                Some((q, _)) => format!("{q}.{}({})", call.callee, args.join(", ")),
                None => format!("{}({})", call.callee, args.join(", ")),
            };
            for a in call.args.iter().filter(|a| a.after) {
                text.push_str(&format!(" {}", written_with(s, &a.value)));
            }
            text
        }
        Expr::Field { of, name, .. } => format!("{}.{name}", written_with(s, of)),
        Expr::Arith {
            op, left, right, ..
        } => {
            // An operand that groups looser than the operator, and a right
            // operand that groups as tightly, was in parentheses.
            let binds =
                |op: &ArithOp| matches!(op, ArithOp::Mul | ArithOp::Div | ArithOp::Rem) as u8;
            let side = |e: &Expr, right: bool| match e {
                Expr::Arith { op: inner, .. }
                    if binds(inner) < binds(op) || (right && binds(inner) == binds(op)) =>
                {
                    format!("({})", written_with(s, e))
                }
                _ => written_with(s, e),
            };
            format!(
                "{} {} {}",
                side(left, false),
                op.symbol(),
                side(right, true)
            )
        }
        Expr::Case { name, payload, .. } => match payload {
            Some(p) => format!(".{name}({})", written_with(s, p)),
            None => format!(".{name}"),
        },
        Expr::Match(m) => {
            // The side of `and` or `or` that runs only sometimes is what one
            // of the blocks gives.
            let side = |arm: usize| match m.arms[arm].body.as_slice() {
                [Stmt::Expr(e)] => written_with(s, e),
                _ => "...".to_string(),
            };
            match m.form {
                Form::Match => format!("match {} {{ ... }}", written_with(s, &m.of)),
                Form::If { .. } => format!("if {} {{ ... }}", written_with(s, &m.of)),
                Form::Not => format!("not ({})", written_with(s, &m.of)),
                Form::And => format!("({}) and ({})", written_with(s, &m.of), side(0)),
                Form::Or => format!("({}) or ({})", written_with(s, &m.of), side(1)),
                Form::Try => match &m.of {
                    Expr::Call(split) => format!("try {}", written_with(s, &split.args[0].value)),
                    of => format!("try {}", written_with(s, of)),
                },
            }
        }
        Expr::Tuple { items, .. } => {
            let items: Vec<String> = items.iter().map(written_expr).collect();
            format!("({})", items.join(", "))
        }
        Expr::List { items, .. } => {
            let items: Vec<String> = items.iter().map(|i| written_with(s, i)).collect();
            format!("[{}]", items.join(", "))
        }
        Expr::Index { of, index, .. } => {
            format!("{}[{}]", written_with(s, of), written_with(s, index))
        }
        Expr::Slice { of, from, to, .. } => {
            let end = |e: &Option<Box<Expr>>| e.as_ref().map(|e| written_with(s, e));
            format!(
                "{}[{}..{}]",
                written_with(s, of),
                end(from).unwrap_or_default(),
                end(to).unwrap_or_default()
            )
        }
        Expr::Lambda(l) if l.params.is_empty() => "{ ... }".to_string(),
        Expr::Lambda(l) => {
            let params: Vec<&str> = l.params.iter().map(|(p, _)| p.as_str()).collect();
            format!("fn({}) {{ ... }}", params.join(", "))
        }
        Expr::Loop(l) => match (&l.range, &l.over) {
            (Some(r), _) => format!(
                "loop {} in {}..{} {{ ... }}",
                r.name,
                written_with(s, &r.from),
                written_with(s, &r.to)
            ),
            (None, Some(o)) => {
                format!("loop {} in {} {{ ... }}", o.name, written_with(s, &o.value))
            }
            (None, None) => match &l.decreases {
                Some(e) => format!("loop decreases {} {{ ... }}", written_with(s, e)),
                None => "loop { ... }".to_string(),
            },
        },
        Expr::For(f) => format!("for {} {{ ... }}", f.index().0),
        Expr::Break { .. } => "break".to_string(),
        Expr::Unbuilt { .. } => "what is not built".to_string(),
        Expr::Return { value: None, .. } => "return".to_string(),
        Expr::Return { value: Some(v), .. } => format!("return {}", written_with(s, v)),
    }
}

/// A definition of the file, by its place among those of its kind.
// Equality is the derived one, with a count of its calls, so it agrees with `Hash`.
#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Clone, Copy, Debug, Eq, Hash)]
enum Item {
    Fn(usize),
    Struct(usize),
    Enum(usize),
    Trait(usize),
    Impl(usize),
}

impl PartialEq for Item {
    fn eq(&self, other: &Item) -> bool {
        crate::work::item();
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && match (self, other) {
                (Item::Fn(a), Item::Fn(b))
                | (Item::Struct(a), Item::Struct(b))
                | (Item::Enum(a), Item::Enum(b))
                | (Item::Trait(a), Item::Trait(b))
                | (Item::Impl(a), Item::Impl(b)) => a == b,
                _ => false,
            }
    }
}

/// The kinds of definition that claim a name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Claim {
    Fn(usize),
    Struct(usize),
    Enum(usize),
    Trait(usize),
    /// A function of a trait: the trait, and its place there.
    TraitFn(usize, usize),
}

/// A file that has no error: the program it describes, and the warnings
/// `smith check` reports about it.
pub struct Checked {
    pub program: crate::scalar::Program,
    pub warnings: Vec<Error>,
    /// How many functions and implementations of the prelude the program
    /// reaches, which are the ones the checker checks.
    pub prelude_reached: (usize, usize),
    /// How many `impl`s of the prelude were read for the program.
    pub prelude_loaded: usize,
    /// The traits of numbers of the prelude that were read: those the program,
    /// or a module it imports, calls for.
    pub prelude_numeric: Vec<String>,
    /// The core term of the functions of the file that have one, when it was
    /// asked for.
    pub core: Vec<crate::core::Function>,
}

/// `check`, and the core term of the functions of the file that have one.
pub fn check_core(file: File, library: &Library) -> Result<Checked, Vec<Error>> {
    check_reading(file, library, &PRELUDE, None, true)
}

/// Checks the file, and lowers what `main` reaches to the scalar form. Every
/// error is reported, except one that a mistake already reported would
/// explain.
pub fn check(file: File, library: &Library) -> Result<Checked, Vec<Error>> {
    check_with(file, library, &PRELUDE)
}

/// `check`, with the text of the prelude given, which is divided now.
pub(crate) fn check_with_source(
    file: File,
    library: &Library,
    prelude: &str,
) -> Result<Checked, Vec<Error>> {
    let divided = crate::divide::divide(prelude);
    let impls: Vec<PreludeImpl> = (divided.impls.iter())
        .map(|im| PreludeImpl {
            source: im.source,
            name: im.name,
            types: &im.types,
        })
        .collect();
    let numeric: Vec<PreludeNumeric> = (divided.numeric.iter())
        .map(|n| PreludeNumeric {
            source: n.source,
            names: &n.names,
        })
        .collect();
    let prelude = Prelude {
        declarations: &divided.declarations,
        numeric: &numeric,
        impls: &impls,
        parsed: None,
    };
    check_with(file, library, &prelude)
}

/// `check`, with the prelude given.
fn check_with(file: File, library: &Library, prelude: &Prelude) -> Result<Checked, Vec<Error>> {
    check_reading(file, library, prelude, None, false)
}

/// Which of the prelude's traits of numbers the module writes a name of: a
/// module of the standard library is looked up in a table made when the
/// compiler was built, and any other is read.
fn calls_for_numbers(prelude: &Prelude, (name, source): (&str, &str), wanted: &mut [bool]) {
    if let Some(name) = name.strip_prefix("std.") {
        for (i, users) in NUMERIC_USERS.iter().enumerate().take(wanted.len()) {
            wanted[i] |= users.contains(&name);
        }
        return;
    }
    for word in source.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        prelude.call_for(word, wanted);
    }
}

/// `check_with`, in which `whole` names a module of the standard library that
/// is read whole, and everything in the library is reached.
fn check_reading(
    mut file: File,
    library: &Library,
    prelude: &Prelude,
    whole: Option<&str>,
    want_core: bool,
) -> Result<Checked, Vec<Error>> {
    // Whether a function of a module is given a struct of arguments, which the
    // file that is built asks for by writing `Function`.
    let file_asks = crate::function_args::asks(&file);
    let asks_function = file_asks;
    let asks_notes = crate::item_notes::asks(&file);
    let mut resolver = Resolver::new(library);
    resolver.whole = whole.is_some();
    let defined: HashSet<String> = defined_names(&file).into_iter().map(String::from).collect();
    resolver.resolve(
        Reader::Entry,
        &file.uses,
        &defined,
        &file.written,
        &file.qualified,
        &scope::used_names(&file),
    );
    if let Some(module) = whole {
        resolver.module(module, Pos::START);
    }
    let whole = whole.is_some();
    // The traits of numbers are read when the file, or a module it imports,
    // writes the name of one or of one of its functions.
    let mut numeric = vec![false; prelude.numeric.len()];
    for module in &resolver.read {
        calls_for_numbers(prelude, (&module.name, &module.source), &mut numeric);
    }
    for (name, _) in &file.idents {
        if numeric.iter().all(|w| *w) {
            break;
        }
        prelude.call_for(name, &mut numeric);
    }
    let Resolver {
        read,
        labels,
        mut errors,
        warnings,
        refused,
        unimported,
        ..
    } = resolver;
    // What the file uses that is not built is reported with its other
    // errors, as the parser read past it.
    errors.append(&mut file.unbuilt);
    placement(&file, &mut errors);
    for module in &read {
        placement(&module.file, &mut errors);
    }
    // The names the file writes, which name what it takes from the modules.
    let mut written: Vec<String> = std::mem::take(&mut file.idents)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    written.extend((file.qualified.iter()).map(|q| format!("{}@{}", q.name.0, q.module)));
    // A module that the file imports may write `Function` for the functions of
    // the file, as the file may for those of a module.
    let asks_function = asks_function
        || read.iter().any(|m| match m.name.strip_prefix("std.") {
            Some(name) => FUNCTION_USERS.contains(&name),
            None => crate::function_args::asks(&m.file),
        });
    if asks_function && !file_asks {
        let text = file.text.clone().unwrap_or_else(|| "".into());
        crate::function_args::add(&mut file, &text, 0);
        // The structs just made have comments to note as the others have.
        if asks_notes {
            crate::item_notes::add(&mut file, &text);
        }
    }
    file.floats |= read.iter().any(|m| m.file.floats);
    let (mut file, items, mut lazy) = splice(file, read, &mut errors, asks_function);
    lazy.written = written;
    file.modules = module_names(&labels, library);
    let checked = check_file(
        file,
        items,
        lazy,
        errors,
        warnings,
        (prelude, &numeric),
        (whole, want_core),
    );
    if unimported.is_empty() {
        return checked
            .map(|mut checked| {
                checked.warnings = localize(checked.warnings, &labels);
                checked
            })
            .map_err(|errors| {
                let errors = errors.into_iter().filter(|e| !refused.contains(&e.pos));
                localize(errors.collect(), &labels)
            });
    }
    let mut errors: Vec<Error> = (checked.err().into_iter().flatten())
        .filter(|e| !refused.contains(&e.pos))
        .chain(unimported)
        .collect();
    errors.sort_by_key(|e| e.pos);
    Err(localize(errors, &labels))
}

/// The errors in the part `part` of `parts` of the modules of the standard
/// library, each read whole and checked whole: every function, type, trait, and
/// `impl` in it, whether or not a program reaches it, as no program is held to
/// reach what it does not. The parts, numbered from 0, hold every module once.
pub fn check_standard_library_part(part: usize, parts: usize) -> Vec<Error> {
    check_library_part(&Library::standard(), part, parts)
}

/// The errors in every module of the library, each read whole and checked
/// whole.
pub fn check_library(library: &Library) -> Vec<Error> {
    check_library_part(library, 0, 1)
}

/// The errors in the modules of the library whose place is `part` modulo
/// `parts`, each read whole and checked whole.
fn check_library_part(library: &Library, part: usize, parts: usize) -> Vec<Error> {
    let mut found = Vec::new();
    for (name, _) in library.modules.iter().skip(part).step_by(parts) {
        let tokens = crate::lexer::lex(b"fn main() {}\n").expect("the file lexes");
        let file = crate::parser::parse(&tokens).expect("the file parses");
        if let Err(errors) =
            check_reading(file, library, &PRELUDE, Some(&format!("std.{name}")), false)
        {
            found.extend(errors);
        }
    }
    found
}

/// The names of the modules of the standard library where what a program that
/// reads the module in pieces takes the compiler to hold is not what the module
/// read whole holds, which is so when `may_be_wrapper` lets a wrapper by.
pub fn modules_whose_held_names_differ() -> Vec<String> {
    let library = Library::standard();
    let held = |name: &str, whole: bool| {
        let mut resolver = Resolver::new(&library);
        resolver.whole = whole;
        let slot = resolver.module(name, Pos::START);
        let mut held: Vec<String> = slot
            .map(|slot| resolver.read[slot].held.iter().cloned().collect())
            .unwrap_or_default();
        held.sort();
        held
    };
    (library.modules.iter())
        .map(|(name, _)| format!("std.{name}"))
        .filter(|name| held(name, false) != held(name, true))
        .collect()
}

/// The errors, or else the warnings, found in reading a module and the
/// modules it imports: what its syntax and its `use` lines say, and where its
/// `impl`s are. Nothing is built, so what no `main` reaches is not checked.
pub fn check_module(name: &str, library: &Library) -> Result<Vec<Error>, Vec<Error>> {
    let mut resolver = Resolver::new(library);
    resolver.whole = true;
    resolver.module(name, Pos::START);
    let Resolver {
        read,
        labels,
        mut errors,
        warnings,
        ..
    } = resolver;
    for module in &read {
        placement(&module.file, &mut errors);
    }
    if errors.is_empty() {
        Ok(localize(warnings, &labels))
    } else {
        Err(localize(errors, &labels))
    }
}

/// Checks the file, with the items of the modules it imports among its own.
fn check_file(
    file: File,
    items: Items,
    mut lazy: Lazy,
    mut errors: Vec<Error>,
    import_warnings: Vec<Error>,
    (prelude, numeric): (&Prelude, &[bool]),
    (whole, want_core): (bool, bool),
) -> Result<Checked, Vec<Error>> {
    let mut loaded: Vec<usize> = Vec::new();
    let mut file = split_needs(with_prelude(file, prelude, &loaded, numeric));
    expand_groups(&mut file, &mut errors);
    let trait_names: Vec<String> = file.traits.iter().map(|t| t.name.clone()).collect();
    let base_impls: Vec<Arc<Impl>> = std::mem::take(&mut file.impls);
    let (base_functions, base_structs, base_enums) =
        (file.functions.len(), file.structs.len(), file.enums.len());
    // What the file itself names of the modules is read first, and then what
    // that names.
    if !lazy.modules.is_empty() {
        let impls = base_impls.clone();
        let written = std::mem::take(&mut lazy.written);
        lazy.load(
            &[],
            &written,
            |out| names_in_items(&file, &impls, out),
            &trait_names,
            &mut errors,
        );
    }
    // The `impl`s of the prelude and the items of the modules that what the
    // program reaches calls for, which reach more, until no more are called
    // for.
    loop {
        file.functions.truncate(base_functions);
        file.structs.truncate(base_structs);
        file.enums.truncate(base_enums);
        file.impls = (loaded.iter().map(|&i| prelude.impl_at(i)))
            .chain(base_impls.iter().cloned())
            .collect();
        lazy.append(&mut file);
        let names = Names::of(&file, &lazy.resolution);
        let Some(&start_at) = names
            .functions
            .get(START)
            .or_else(|| names.functions.get("main"))
        else {
            break;
        };
        let found = reach(
            &file,
            (&names, &lazy.resolution),
            &roots_of(&file, start_at, whole),
            true,
        );
        let wanted = prelude.wanted(&file, &found.items, &loaded);
        let from_modules = lazy.wanted(&file, &names, &found);
        // An operation the target answers through a function of the library
        // calls for it when the program reaches the operation, though no line
        // writes its name.
        let throughs: Vec<(&str, Pos)> = (found.items.iter())
            .filter_map(|item| match item {
                Item::Fn(f) => {
                    let f = &file.functions[*f];
                    provided_by(f).and_then(|p| p.through).map(|t| (t, f.pos))
                }
                _ => None,
            })
            .collect();
        let before = lazy.reads;
        loaded.extend(wanted.iter().copied());
        loaded.sort_unstable();
        lazy.load(
            &from_modules,
            &[],
            |out| {
                for &(through, pos) in &throughs {
                    out(through, pos, Ask::Any);
                }
            },
            &trait_names,
            &mut errors,
        );
        if wanted.is_empty() && from_modules.is_empty() && lazy.reads == before {
            break;
        }
    }

    // The first definition of a name holds it, and a later one is a
    // mistake. Types and functions have names of their own, and the
    // functions of different traits may share one.
    let mut defined: Vec<(Pos, &str, Claim)> = Vec::new();
    defined.extend(
        (file.functions.iter().enumerate()).map(|(i, f)| (f.pos, f.name.as_str(), Claim::Fn(i))),
    );
    defined.extend(
        (file.structs.iter().enumerate()).map(|(i, s)| (s.pos, s.name.as_str(), Claim::Struct(i))),
    );
    defined.extend(
        (file.enums.iter().enumerate()).map(|(i, e)| (e.pos, e.name.as_str(), Claim::Enum(i))),
    );
    for (k, t) in file.traits.iter().enumerate() {
        defined.push((t.pos, t.name.as_str(), Claim::Trait(k)));
        for (i, f) in t.functions.iter().enumerate() {
            defined.push((f.pos, f.name.as_str(), Claim::TraitFn(k, i)));
        }
    }
    defined.sort_by_key(|(pos, _, _)| *pos);
    let resolution = std::mem::take(&mut lazy.resolution);
    let mut function_names: Map<std::borrow::Cow<str>, usize> =
        Map::with_capacity_and_hasher(file.functions.len(), Default::default());
    let mut types = Types::with_capacity(file.structs.len() + file.enums.len() + file.traits.len());
    let mut trait_fns: Map<&str, Vec<(usize, usize)>> = Map::default();
    for (pos, name, claim) in defined {
        let is_type = matches!(claim, Claim::Struct(_) | Claim::Enum(_) | Claim::Trait(_));
        // What the file calls the item, which a struct, an enum, or a trait
        // does not write: the module that defines it does.
        let key = if matches!(claim, Claim::TraitFn(..)) {
            std::borrow::Cow::Borrowed(name)
        } else {
            resolution.def_key(pos, name)
        };
        let prelude = |what: &str| {
            Error::unsupported(
                pos,
                &format!("defining `{what}`, which the prelude defines,"),
            )
        };
        let prelude_name = match claim {
            Claim::Fn(_) | Claim::TraitFn(..) => false,
            Claim::Struct(_) | Claim::Enum(_) => PRELUDE_TYPES.contains(&name),
            Claim::Trait(_) => {
                name == TEXT || TRAITS_NOT_BUILT.contains(&name) || PRELUDE_TRAITS.contains(&name)
            }
        };
        if prelude_name && !of_prelude(pos) {
            errors.push(prelude(name));
        } else if let Some(first) = if is_type { types.get(&key) } else { None } {
            let what = match first {
                Item::Struct(_) => "a struct",
                Item::Enum(_) => "an enum",
                _ => "a trait",
            };
            errors.push(Error::new(
                pos,
                format!("there is already {what} named `{name}`"),
            ));
        } else if !is_type
            && function_names.get(&key).is_some_and(|&f| match claim {
                // A function of a trait is no rival of a function in a module
                // that cannot write the trait.
                Claim::TraitFn(k, _) => (file.scopes.iter())
                    .find(|(m, _)| *m == module_of(file.functions[f].pos))
                    .is_some_and(|(_, traits)| traits.contains(&file.traits[k].name)),
                _ => true,
            })
        {
            errors.push(Error::new(
                pos,
                format!("there is already a function named `{name}`"),
            ));
        } else {
            match claim {
                Claim::Fn(i) => {
                    // A function of the prelude's trait that no call needs a
                    // trait for is not a name the program may not use.
                    // A trait the module that defines the function cannot write
                    // is no rival of it.
                    let writes = |k: usize| {
                        (file.scopes.iter())
                            .find(|(m, _)| *m == module_of(pos))
                            .is_some_and(|(_, traits)| traits.contains(&file.traits[k].name))
                    };
                    let taken = (trait_fns.get(&*key).and_then(|f| f.first()))
                        .filter(|(k, _)| !of_prelude(file.traits[*k].pos) && writes(*k));
                    if let Some(&(k, _)) = taken {
                        errors.push(Error::new(
                            pos,
                            format!(
                                "there is already a function named `{name}`, in the trait `{}`",
                                file.traits[k].name
                            ),
                        ));
                    } else {
                        function_names.insert(key, i);
                    }
                }
                Claim::Struct(i) => {
                    types.insert(pos, name, key, Item::Struct(i));
                }
                Claim::Enum(i) => {
                    types.insert(pos, name, key, Item::Enum(i));
                }
                Claim::Trait(k) => {
                    types.insert(pos, name, key, Item::Trait(k));
                }
                Claim::TraitFn(k, i) => {
                    let list = trait_fns.entry(name).or_default();
                    // A second function of one trait with the name is
                    // reported with the trait.
                    if !list.iter().any(|(t, _)| *t == k) {
                        list.push((k, i));
                    }
                }
            }
        }
    }
    let Some(&main) = function_names.get("main") else {
        errors.push(Error::new(
            Pos::START,
            "there is no function named `main`, where the program starts",
        ));
        return Err(errors);
    };

    // Where the program starts: at `main`, or at the function that ends the
    // program with the status `main`'s result gives.
    declarations(&file, main, &mut errors);
    let start_at = function_names.get(START).copied().unwrap_or(main);
    types.link(&resolution);
    let names = Names {
        functions: function_names,
        types,
        trait_fns,
    };
    let roots = roots_of(&file, start_at, whole);
    let Reached {
        items: reached,
        unbuilt,
        ..
    } = reach(&file, (&names, &resolution), &roots, true);
    let reached_types: Vec<Def> = (reached.iter())
        .filter_map(|r| match r {
            Item::Struct(s) => Some(Def::Struct(*s)),
            Item::Enum(e) => Some(Def::Enum(*e)),
            _ => None,
        })
        .collect();

    let (function_names, trait_fns, types) = (names.functions, names.trait_fns, names.types);
    let mut defs = Defs {
        file: &file,
        whole,
        resolution,
        function_names,
        function_memo: Default::default(),
        types,
        trait_fns,
        cyclic: vec![false; file.structs.len()],
        cyclic_enums: vec![false; file.enums.len()],
        boxed: vec![false; file.structs.len()],
        boxed_enums: vec![false; file.enums.len()],
        implicit: vec![0; file.structs.len()],
        constructions: Vec::new(),
        field_items: Vec::new(),
        env_lambdas: Vec::new(),
        env_at: Map::default(),
        env_owner: Vec::new(),
        env_types: Map::default(),
        env_calls: Map::default(),
        env_unsettled: Set::default(),
        struct_scopes: Vec::new(),
        fields: Vec::new(),
        enum_scopes: Vec::new(),
        cases: Vec::new(),
        signatures: Vec::new(),
        trait_sigs: Vec::new(),
        outs: (file.traits.iter())
            .map(|t| t.generics.iter().map(|g| g.out.is_some()).collect())
            .collect(),
        impls: Vec::new(),
        impls_of: vec![Vec::new(); file.traits.len()],
        fmt: prelude_item(&file, Kind::Trait, "Fmt"),
        repr: prelude_item(&file, Kind::Trait, "Repr"),
        eq: prelude_item(&file, Kind::Trait, "Eq"),
        exit: prelude_item(&file, Kind::Trait, "Exit"),
        iterate: prelude_item(&file, Kind::Trait, "Iterate"),
        ord: prelude_item(&file, Kind::Trait, "Ord"),
        operators: ["Add", "Sub", "Mul"].map(|n| prelude_item(&file, Kind::Trait, n)),
        hash: prelude_item(&file, Kind::Trait, "Hash"),
        convert: prelude_item(&file, Kind::Trait, "Convert"),
        bits: try_prelude_trait(&file, "Bits").unwrap_or(usize::MAX),
        function: prelude_item(&file, Kind::Trait, "Function"),
        serialize: library_item(&file, &items, "std.serialize", Kind::Trait, "Serialize"),
        writer: library_item(&file, &items, "std.serialize", Kind::Trait, "Writer"),
        deserialize: library_item(&file, &items, "std.deserialize", Kind::Trait, "Deserialize"),
        reader: library_item(&file, &items, "std.deserialize", Kind::Trait, "Reader"),
        describe: library_item(&file, &items, "std.describe", Kind::Trait, "Describe"),
        describer: library_item(&file, &items, "std.describe", Kind::Trait, "Describer"),
        from_text: library_item(&file, &items, "std.fromtext", Kind::Trait, "FromText"),
        from_request: library_item(&file, &items, "std.http", Kind::Trait, "FromRequest"),
        read_error: library_item(&file, &items, "std.deserialize", Kind::Struct, "ReadError"),
        problem: library_item(&file, &items, "std.deserialize", Kind::Enum, "Problem"),
        frame: library_item(&file, &items, "std.task", Kind::Struct, "Frame"),
        task: library_item(&file, &items, "std.task", Kind::Struct, "Task"),
        handles: [
            ("Listener", "stop"),
            ("Connection", "close"),
            ("Socket", "unbind"),
        ]
        .into_iter()
        .filter_map(|(name, end)| {
            library_item(&file, &items, "std.net", Kind::Struct, name).map(|s| (s, end))
        })
        .collect(),
        file_handle: library_item(&file, &items, "std.fs", Kind::Struct, "File"),
        poll: library_item(&file, &items, "std.task", Kind::Enum, "Poll"),
        wait: library_item(&file, &items, "std.task", Kind::Enum, "Wait"),
        boolean: prelude_item(&file, Kind::Enum, "bool"),
        order: prelude_item(&file, Kind::Enum, "Order"),
        option: prelude_item(&file, Kind::Enum, "Option"),
        flow: prelude_item(&file, Kind::Enum, "Flow"),
        result: prelude_item(&file, Kind::Enum, "Result"),
        range: prelude_item(&file, Kind::Struct, "Range"),
        withouts: Vec::new(),
        default_depth: Cell::new(0),
        has_alias: file.structs.iter().any(|s| s.alias.is_some()),
        written_here: RefCell::new(None),
        spelled: Cell::new(Spelled::default()),
        bound_params: RefCell::new(Vec::new()),
    };
    alias_cycles(&mut defs, &mut errors);
    cycles(&mut defs, &reached_types, &mut errors);
    defs.implicit = (0..file.structs.len())
        .map(|s| defs.implicit_arity(s, &mut Vec::new()))
        .collect();
    defs.struct_scopes = file
        .structs
        .iter()
        .map(|s| {
            let mut scope = defs.scope(&s.generics, &[]);
            defs.add_implicit(&mut scope, s.fields.iter().map(|f| &f.ty));
            scope
        })
        .collect();
    defs.fields = (file.structs.iter().enumerate())
        .map(|(i, s)| {
            (s.fields.iter())
                .map(|f| defs.quietly(&f.ty, &defs.struct_scopes[i]))
                .collect()
        })
        .collect();
    defs.constructions = (0..file.structs.len())
        .map(|s| {
            (file.structs[s]
                .fields
                .iter()
                .any(|f| f.ty.statement.is_some()))
            .then(|| construction_as_call(&defs, s))
        })
        .collect();
    defs.field_items = (file.structs.iter().enumerate())
        .map(|(i, s)| {
            (s.fields.iter())
                .map(|f| {
                    let written = f.function.as_ref()?;
                    match defs.quietly(written, &defs.struct_scopes[i]) {
                        Ty::Fn(items) => Some(items),
                        _ => None,
                    }
                })
                .collect()
        })
        .collect();
    (defs.env_lambdas, defs.env_owner) = env_lambdas(&defs);
    defs.env_at = (defs.env_lambdas.iter().enumerate())
        .filter_map(|(s, l)| Some((l.as_ref()?.0.pos, s)))
        .collect();
    defs.enum_scopes = file
        .enums
        .iter()
        .map(|e| defs.scope(&e.generics, &[]))
        .collect();
    defs.cases = (file.enums.iter().enumerate())
        .map(|(i, e)| {
            (e.cases.iter())
                .map(|c| c.ty.as_ref().map(|t| defs.quietly(t, &defs.enum_scopes[i])))
                .collect()
        })
        .collect();
    defs.signatures = (file.functions.iter())
        .map(|f| defs.signature_of(f))
        .collect();
    defs.trait_sigs = (file.traits.iter())
        .map(|t| {
            (t.functions.iter())
                .map(|f| {
                    // A function's own type parameters come after the trait's.
                    let generics: Vec<Generic> =
                        t.generics.iter().chain(&f.generics).cloned().collect();
                    let mut sig = defs.signature(f, defs.scope(&generics, &[]));
                    sig.param_defaults = Vec::new();
                    // A comparison has the two sides of one type, so a
                    // parameter that defaults to the first is the first.
                    if matches!(t.name.as_str(), "Eq" | "Ord") {
                        for (_, ty) in &mut sig.params {
                            if *ty == Ty::Param(1) {
                                *ty = Ty::Param(0);
                            }
                        }
                    }
                    sig
                })
                .collect()
        })
        .collect();
    for (i, im) in file.impls.iter().enumerate() {
        let info = impl_info(&defs, im);
        if let Some(info) = &info {
            defs.impls_of[info.of].push(i);
        }
        defs.impls.push(info);
    }
    // An implementation for types an earlier one covers is not chosen.
    for k in 0..file.traits.len() {
        for (at, &j) in defs.impls_of[k].iter().enumerate() {
            let covered = defs.impls_of[k][..at].iter().any(|&i| {
                let a = defs.impls[i].as_ref().expect("listed");
                let b = defs.impls[j].as_ref().expect("listed");
                // An implementation that requires something stands for fewer
                // types than it is written for, so a use decides.
                a.chosen
                    && a.scope.requires.is_empty()
                    && b.scope.requires.is_empty()
                    && overlaps(&defs.outs[k], a, b)
                    // Of two that overlap, the one for fewer types is used
                    // where both fit.
                    && !defs.more_specific(i, j)
                    && !defs.more_specific(j, i)
            });
            if covered && let Some(info) = &mut defs.impls[j] {
                info.chosen = false;
            }
        }
    }

    defs.withouts = withouts(&defs, &mut errors);

    settle_env_types(&mut defs);
    let mut checker = Checker::new(&defs, errors);
    checker.main(main);
    for &item in &reached {
        match item {
            Item::Fn(f) => checker.signature(f),
            Item::Struct(s) => checker.structure(s),
            Item::Enum(e) => checker.enumeration(e),
            Item::Trait(k) => checker.declared_trait(k),
            Item::Impl(i) => checker.implementation(i),
        }
    }
    for &item in &reached {
        match item {
            Item::Fn(f) if defs.is_operation(f) => {}
            Item::Fn(f) => checker.body(Template::Free(f)),
            Item::Impl(i) if !unbuilt.contains(&i) => {
                if let Some(info) = &defs.impls[i] {
                    for (fi, at) in info.fns.iter().enumerate() {
                        if at.is_some() {
                            checker.body(Template::Impl(i, fi));
                        }
                    }
                }
            }
            Item::Impl(_) | Item::Struct(_) | Item::Enum(_) | Item::Trait(_) => {}
        }
    }
    checker.recursion(start_at, &mut Set::default(), &mut Set::default());
    // A proof function is called by no code, so it is where its own walk starts.
    for (k, f) in file.functions.iter().enumerate() {
        if is_proof(f) && !of_prelude(f.pos) {
            checker.recursion(k, &mut Set::default(), &mut Set::default());
        }
    }
    if whole {
        // What a function of the library lists and does not need is as much a
        // mistake there as in a program, and is read as one.
        let as_value: Set<Pos> = (checker.fn_values.values())
            .map(|&i| file.functions[i].pos)
            .collect();
        let unused = unused_effect_warnings(&checker.unused_effects, &as_value, of_std);
        checker.errors.extend(unused);
        return Err(checker.errors);
    }
    if !checker.errors.is_empty() {
        // What the function that starts the program says is wrong follows from
        // what `main` says, which is at a place the reader can see.
        if checker.errors.iter().any(|e| !of_prelude(e.pos)) {
            checker.errors.retain(|e| !of_prelude(e.pos));
        }
        return Err(checker.errors);
    }
    let start = Instance {
        template: Template::Free(start_at),
        args: Vec::new(),
    };
    let kernel_calls = std::mem::take(&mut checker.kernel_calls);
    let unused_effects = std::mem::take(&mut checker.unused_effects);
    let as_value: Set<Pos> = (checker.fn_values.values())
        .map(|&i| file.functions[i].pos)
        .collect();
    let typed = Typed {
        defs: &defs,
        types: checker.types,
        drops: checker.drops,
        widened: checker.widened,
        value_calls: checker.value_calls,
        fn_values: checker.fn_values,
        named: checker.named,
        named_calls: checker.named_calls,
        proof_names: checker.proof_names,
        field_receivers: checker.field_receivers,
        type_args: checker.type_args,
        core_vars: RefCell::new(Vec::new()),
        extended: checker
            .extended
            .iter()
            .map(|(f, s)| (*f, s.names.len()))
            .collect(),
        lambda_base: checker.lambda_base,
    };
    let mut errors = Vec::new();
    typed.copies(
        &start,
        Pos::START,
        &mut Map::default(),
        &mut Set::default(),
        &mut errors,
    );
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut errors = typed.prove_by_kernel(&kernel_calls);
    errors.extend(typed.prove_proofs());
    if !errors.is_empty() {
        return Err(errors);
    }

    let program = to_scalar::lower(&typed, start)?;
    let core = if want_core {
        typed.core_terms()
    } else {
        Vec::new()
    };
    let prelude_reached = reached.iter().fold((0, 0), |(f, i), item| match *item {
        Item::Fn(k) if of_prelude(file.functions[k].pos) => (f + 1, i),
        Item::Impl(k) if of_prelude(file.impls[k].pos) => (f, i + 1),
        _ => (f, i),
    });
    Ok(Checked {
        program,
        prelude_reached,
        prelude_loaded: loaded.len(),
        core,
        prelude_numeric: (prelude.numeric.iter().zip(numeric))
            .filter(|(_, wanted)| **wanted)
            .map(|(n, _)| n.names[0].to_string())
            .collect(),
        warnings: {
            // An `impl` that is not built is dead code, as a function nothing calls is.
            let built: Vec<Item> = (reached.iter().copied())
                .filter(|item| !matches!(item, Item::Impl(i) if unbuilt.contains(i)))
                .collect();
            let mut all = warnings(&file, &built);
            all.extend(unused_effect_warnings(&unused_effects, &as_value, |at| {
                !of_std(at)
            }));
            all.extend(import_warnings);
            all
        },
    })
}

/// The warnings for the effects functions list and nothing in them needs, at
/// the places `wanted` says, leaving out the functions used as values, which
/// may list an effect for the type they are given.
fn unused_effect_warnings(
    unused: &[(Pos, String, String, Pos, String, bool)],
    as_value: &Set<Pos>,
    wanted: impl Fn(Pos) -> bool,
) -> Vec<Error> {
    (unused.iter())
        .filter(|(pos, _, _, at, _, _)| wanted(*at) && !of_prelude(*at) && !as_value.contains(pos))
        .map(|(_, name, effect, at, fix, group)| {
            let message = if *group {
                format!(
                    "`{name}` lists `{}`, and nothing in it needs any of its effects",
                    written_name(effect)
                )
            } else {
                format!(
                    "`{name}` lists the `{}` effect, and nothing in it needs it",
                    written_name(effect)
                )
            };
            Error::new(*at, message).with_fix(format!("write `{fix}`"))
        })
        .collect()
}

/// Where reaching starts: at the function the program starts at, and, in a
/// check of the library, at everything in it.
fn roots_of(file: &File, start_at: usize, whole: bool) -> Vec<Item> {
    let mut roots = vec![Item::Fn(start_at)];
    // A proof function is called by no code, so nothing reaches it.
    roots.extend(
        (file.functions.iter().enumerate())
            .filter(|(_, f)| is_proof(f))
            .map(|(i, _)| Item::Fn(i)),
    );
    if whole {
        let in_std = |pos: Pos| of_std(pos);
        roots.extend(
            (file.functions.iter().enumerate())
                .filter(|(_, f)| in_std(f.pos))
                .map(|(i, _)| Item::Fn(i)),
        );
        roots.extend(
            (file.structs.iter().enumerate())
                .filter(|(_, s)| in_std(s.pos))
                .map(|(i, _)| Item::Struct(i)),
        );
        roots.extend(
            (file.enums.iter().enumerate())
                .filter(|(_, e)| in_std(e.pos))
                .map(|(i, _)| Item::Enum(i)),
        );
        roots.extend(
            (file.traits.iter().enumerate())
                .filter(|(_, t)| in_std(t.pos))
                .map(|(i, _)| Item::Trait(i)),
        );
        roots.extend(
            (file.impls.iter().enumerate())
                .filter(|(_, im)| in_std(im.pos))
                .map(|(i, _)| Item::Impl(i)),
        );
    }
    let mut seen: Set<Item> = Set::default();
    roots.retain(|r| seen.insert(*r));
    roots
}

/// What a name stands for in a file: the item that first defines it.
struct Names<'a> {
    functions: Map<std::borrow::Cow<'a, str>, usize>,
    types: Types<'a>,
    trait_fns: Map<&'a str, Vec<(usize, usize)>>,
}

/// The structs, enums, and traits of a file, by what the file calls each, and by
/// what the module that wrote each calls it, which a name written in another
/// module may not be.
struct Types<'a> {
    names: Map<std::borrow::Cow<'a, str>, Named>,
}

/// What a name is: the item the file calls it, and the items that the modules
/// that write it, and are not the file, call it. A name listed whose item is
/// not in the file is `None`.
#[derive(Default)]
struct Named {
    file: Option<Item>,
    modules: Vec<(usize, Option<Item>)>,
}

impl<'a> Types<'a> {
    fn with_capacity(n: usize) -> Types<'a> {
        Types {
            names: Map::with_capacity_and_hasher(n, Default::default()),
        }
    }

    /// Adds the item, which the module that defines it writes as `name` and the
    /// file calls `key`. The first item of a name holds it.
    fn insert(&mut self, pos: Pos, name: &'a str, key: std::borrow::Cow<'a, str>, item: Item) {
        let module = module_of(pos);
        let same = key == name;
        let named = self.names.entry(key).or_default();
        let found = *named.file.get_or_insert(item);
        if module == 0 {
            return;
        }
        if same {
            // The module writes the name as the file calls it.
            named.modules.push((module, Some(found)));
        } else {
            let named = self
                .names
                .entry(std::borrow::Cow::Borrowed(name))
                .or_default();
            named.modules.push((module, Some(found)));
        }
    }

    /// Adds what each module lists of the structs, enums, and traits of the
    /// others, once every item is in.
    fn link(&mut self, resolution: &Resolution) {
        for (module, naming) in &resolution.modules {
            for (written, to, _) in &naming.imports {
                if naming.types.contains(written) {
                    let item = self.names.get(to.as_str()).and_then(|n| n.file);
                    let name = std::borrow::Cow::Owned(written.clone());
                    self.names
                        .entry(name)
                        .or_default()
                        .modules
                        .push((module, item));
                }
            }
        }
    }

    /// The item that the module that wrote `name` at `pos` calls it.
    #[inline]
    fn at(&self, pos: Pos, name: &str) -> Option<Item> {
        let named = self.names.get(name)?;
        // The file and the prelude write every name as it is.
        let module = module_of(pos);
        if module != 0
            && let Some((_, found)) = named.modules.iter().find(|(m, _)| *m == module)
        {
            return *found;
        }
        named.file
    }

    /// The item the file calls `name`.
    #[inline]
    fn get(&self, name: &str) -> Option<&Item> {
        self.names.get(name)?.file.as_ref()
    }

    fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

impl<'a> Names<'a> {
    /// The struct, enum, or trait that the module that wrote `name` at `pos`
    /// calls it.
    fn type_item(&self, pos: Pos, name: &str) -> Option<Item> {
        self.types.at(pos, name)
    }

    /// The function that the module that wrote `name` at `pos` calls it.
    fn function_at(&self, resolution: &Resolution, pos: Pos, name: &str) -> Option<usize> {
        // The file that is built writes each name as it is.
        if pos.line < STEP && !cfg!(debug_assertions) {
            return self.functions.get(name).copied();
        }
        resolution.find(pos, name, &|n| self.functions.get(n).copied())
    }

    /// The names of the items of the file, with no mistake reported.
    fn of(file: &'a File, resolution: &Resolution) -> Names<'a> {
        let mut names = Names {
            functions: Map::with_capacity_and_hasher(file.functions.len(), Default::default()),
            types: Types::with_capacity(file.structs.len() + file.enums.len() + file.traits.len()),
            trait_fns: Map::default(),
        };
        for (i, f) in file.functions.iter().enumerate() {
            names
                .functions
                .entry(resolution.def_key(f.pos, &f.name))
                .or_insert(i);
        }
        for (i, s) in file.structs.iter().enumerate() {
            let key = resolution.def_key(s.pos, &s.name);
            names.types.insert(s.pos, &s.name, key, Item::Struct(i));
        }
        for (i, e) in file.enums.iter().enumerate() {
            let key = resolution.def_key(e.pos, &e.name);
            names.types.insert(e.pos, &e.name, key, Item::Enum(i));
        }
        for (k, t) in file.traits.iter().enumerate() {
            let key = resolution.def_key(t.pos, &t.name);
            names.types.insert(t.pos, &t.name, key, Item::Trait(k));
            for (i, f) in t.functions.iter().enumerate() {
                let list = names.trait_fns.entry(f.name.as_str()).or_default();
                if !list.iter().any(|(t, _)| *t == k) {
                    list.push((k, i));
                }
            }
        }
        names.types.link(resolution);
        names
    }
}

/// What the function the program starts at reaches, each item in the order
/// it is first reached. A contract is not run, so what only a `requires` or an
/// `ensures` names is reached only when `contracts` says to look in them, as
/// choosing the prelude's `impl`s does: `by <= 30` there needs `Ord`.
///
/// An `impl` of a trait that is reached, for a type that is not, is in the
/// items but is not walked, and is in the set the call gives beside them: its
/// head is checked, and nothing it holds is, until its type is reached.
fn reach(
    file: &File,
    (names, resolution): (&Names, &Resolution),
    roots: &[Item],
    contracts: bool,
) -> Reached {
    let fmt = prelude_item(file, Kind::Trait, "Fmt");
    let mut reached: Vec<Item> = roots.to_vec();
    let mut queue: Vec<Item> = roots.to_vec();
    let mut unbuilt: Set<usize> = Set::default();
    // The structs and enums each `impl` is for.
    let mut for_types: Vec<Vec<Item>> = Vec::with_capacity(file.impls.len());
    let mut seen: Set<Item> = roots.iter().copied().collect();
    // The `impl`s that name a type in their arguments, and those of a trait,
    // each in order, so that an item finds its own without a walk of all.
    // A struct is told from an enum by the `bool`.
    let mut for_type: Map<(bool, usize), Vec<usize>> = Map::default();
    let mut of_trait: Map<usize, Vec<usize>> = Map::default();
    for (i, im) in file.impls.iter().enumerate() {
        if let Some(Item::Trait(k)) = names.type_item(im.pos, &im.name) {
            of_trait.entry(k).or_default().push(i);
        }
        let mut named: Vec<(bool, usize)> = (im.args.iter())
            .flat_map(type_names)
            .filter_map(|(n, pos)| match names.type_item(pos, n) {
                Some(Item::Struct(s)) => Some((false, s)),
                Some(Item::Enum(e)) => Some((true, e)),
                _ => None,
            })
            .collect();
        named.sort_unstable();
        named.dedup();
        for item in named {
            for_type.entry(item).or_default().push(i);
        }
        // The types the `impl` is chosen by, which are those of the arguments
        // the trait does not write `out` for: an `out` type is the answer of
        // a call that chooses the `impl`, so it is reached with it.
        let outs = match names.type_item(im.pos, &im.name) {
            Some(Item::Trait(k)) => Some(&file.traits[k].generics),
            _ => None,
        };
        let by = im
            .args
            .iter()
            .enumerate()
            .filter(|(j, _)| !outs.is_some_and(|g| g.get(*j).is_some_and(|g| g.out.is_some())));
        let mut types = Vec::new();
        for (name, pos) in by.flat_map(|(_, t)| type_names(t)) {
            if let Some(item @ (Item::Struct(_) | Item::Enum(_))) = names.type_item(pos, name) {
                types.push(item);
            }
        }
        for_types.push(types);
    }
    let mut i = 0;
    while i < queue.len() {
        let mut found: Vec<Item> = Vec::new();
        let built = |name: &str, found: &mut Vec<Item>| {
            found.extend(names.types.get(name));
        };
        let type_name = |name: &str, pos: Pos, found: &mut Vec<Item>| {
            found.extend(names.type_item(pos, name));
        };
        // A case written in full names its enum, and a function may be
        // passed as a value by its name.
        // A name of a parameter is the parameter in the function, so it is no
        // function of the module that has one of that name.
        let called =
            |values: Vec<&Expr>, params: &[crate::parser::Param], found: &mut Vec<Item>| {
                let all = exprs(values.iter().copied());
                for &e in &all {
                    if let Expr::Field { of, .. } = e
                        && let Expr::Name { name, pos } = &**of
                    {
                        type_name(name, *pos, found);
                    }
                    if let Expr::Name { name, pos } = e
                        && let Some(f) = names.function_at(resolution, *pos, name)
                        && !params.iter().any(|p| p.name == *name)
                    {
                        found.push(Item::Fn(f));
                    }
                    // A `loop` over a value runs the `each` of its `Iterate`.
                    if matches!(e, Expr::Loop(l) if l.over.is_some()) {
                        built("Iterate", found);
                    }
                }
                let calls = all.iter().filter_map(|e| match e {
                    Expr::Call(call) => Some(call),
                    _ => None,
                });
                for call in calls {
                    // The types a call writes after its trait are named.
                    for t in call.type_args.iter().filter_map(traits::expr_type) {
                        type_names_in(&t, &mut |name, pos| type_name(name, pos, found));
                    }
                    if let Some((q, pos)) = &call.qualifier {
                        type_name(q, *pos, found);
                        continue;
                    }
                    let name = call.callee.as_str();
                    if let Some(f) = names.function_at(resolution, call.name_pos, name)
                        && !params.iter().any(|p| p.name == name)
                    {
                        found.push(Item::Fn(f));
                        type_name(name, call.name_pos, found);
                        continue;
                    }
                    type_name(name, call.name_pos, found);
                    // A handler of an effect is given values of the types its
                    // operations write, which no name in the program may say.
                    if let Some(effect) = file.effects.iter().find(|e| e.name == name) {
                        for f in file.functions.iter().filter(|f| {
                            crate::parser::is_operation(f) && effect.operations.contains(&f.name)
                        }) {
                            types_named(f, &mut |name, pos| type_name(name, pos, found));
                        }
                    }
                    for &(k, _) in names.trait_fns.get(name).into_iter().flatten() {
                        found.push(Item::Trait(k));
                    }
                }
            };
        let function = |f: &Function, found: &mut Vec<Item>| {
            types_named(f, &mut |name, pos| type_name(name, pos, found));
            let defaults = f.params.iter().filter_map(|p| p.default.as_ref());
            let statement = f.result.as_ref().and_then(|t| t.statement.as_deref());
            let contracts = (f.requires.iter().chain(&f.ensures))
                .chain(&f.decreases)
                .chain(statement)
                .filter(|_| contracts);
            called(
                values(&f.body).chain(defaults).chain(contracts).collect(),
                &f.params,
                found,
            );
        };
        match queue[i] {
            Item::Fn(f) => {
                function(&file.functions[f], &mut found);
                if let Some(through) = provided_by(&file.functions[f]).and_then(|p| p.through)
                    && let Some(g) = names.function_at(resolution, file.functions[f].pos, through)
                {
                    found.push(Item::Fn(g));
                }
                // The struct of its arguments is reached with a function
                // that has one, which `Function` is given to.
                let pos = file.functions[f].pos;
                if let Some((_, made, _)) = (file.args.iter()).find(|(at, ..)| *at == pos) {
                    found.extend(names.type_item(pos, made));
                }
            }
            Item::Struct(si) => {
                let s = &file.structs[si];
                for ty in s.fields.iter().map(|f| &f.ty).chain(&s.alias) {
                    type_names_in(ty, &mut |name, pos| type_name(name, pos, &mut found));
                }
                bounds_named(&s.generics, &mut |name, pos| {
                    type_name(name, pos, &mut found)
                });
                let defaults = s.fields.iter().filter_map(|f| f.default.as_ref());
                // What a proof field says is a contract, which is reached
                // only where contracts are looked in.
                let statements = (s.fields.iter())
                    .filter_map(|f| f.ty.statement.as_deref())
                    .filter(|_| contracts);
                called(defaults.chain(statements).collect(), &[], &mut found);
                found.extend(
                    for_type
                        .get(&(false, si))
                        .into_iter()
                        .flatten()
                        .map(|&i| Item::Impl(i)),
                );
            }
            Item::Enum(ei) => {
                let e = &file.enums[ei];
                for t in e.cases.iter().filter_map(|c| c.ty.as_ref()) {
                    type_names_in(t, &mut |name, pos| type_name(name, pos, &mut found));
                }
                bounds_named(&e.generics, &mut |name, pos| {
                    type_name(name, pos, &mut found)
                });
                found.extend(
                    for_type
                        .get(&(true, ei))
                        .into_iter()
                        .flatten()
                        .map(|&i| Item::Impl(i)),
                );
            }
            Item::Trait(k) => {
                let t = &file.traits[k];
                // The `Fmt` a type is given runs its `Repr`.
                if k == fmt {
                    built("Repr", &mut found);
                }
                for f in &t.functions {
                    function(f, &mut found);
                }
                bounds_named(&t.generics, &mut |name, pos| {
                    type_name(name, pos, &mut found)
                });
                for &i in of_trait.get(&k).into_iter().flatten() {
                    found.push(Item::Impl(i));
                }
            }
            Item::Impl(i) => {
                let im = &file.impls[i];
                type_name(&im.name, im.pos, &mut found);
                for t in &im.args {
                    type_names_in(t, &mut |name, pos| type_name(name, pos, &mut found));
                }
                bounds_named(&im.generics, &mut |name, pos| {
                    type_name(name, pos, &mut found)
                });
                needs_named(&im.requires, &mut |name, pos| {
                    type_name(name, pos, &mut found)
                });
                for f in &im.functions {
                    function(f, &mut found);
                }
            }
        }
        for item in found {
            let held = match item {
                Item::Impl(k) => for_types[k].iter().all(|t| seen.contains(t)),
                _ => true,
            };
            if seen.insert(item) {
                reached.push(item);
                match item {
                    Item::Impl(k) if !held => {
                        unbuilt.insert(k);
                    }
                    _ => queue.push(item),
                }
            } else if let Item::Impl(k) = item
                && held
                && unbuilt.remove(&k)
            {
                queue.push(item);
            }
        }
        i += 1;
    }
    Reached {
        items: reached,
        unbuilt,
    }
}

/// What the program reaches.
struct Reached {
    /// The items, in the order each is first reached.
    items: Vec<Item>,
    /// The `impl`s whose heads are checked and nothing more.
    unbuilt: Set<usize>,
}

/// The modules of a library, each with its name and its source, and the files
/// of a project, which give the modules the standard library does not hold.
/// The standard library is the files of `std/`.
pub struct Library<'a> {
    modules: Vec<(&'a str, &'a str)>,
    project: Option<Project<'a>>,
}

/// The files a project's modules are in.
struct Project<'a> {
    /// The source of the module with a name written as in a `use`, as
    /// `db.transaction`, when a file holds it.
    files: &'a dyn crate::Source,
    /// The module the file that is built is, when its path gives it a name.
    /// Nothing imports from it.
    entry: Option<&'a str>,
}

impl<'a> Library<'a> {
    /// The library of the modules, given as a name and a source each.
    pub fn new(modules: &[(&'a str, &'a str)]) -> Library<'a> {
        Library {
            modules: modules.to_vec(),
            project: None,
        }
    }

    /// The standard library, which the executable holds.
    pub fn standard() -> Library<'static> {
        Library {
            modules: LIBRARY_MODULES
                .iter()
                .filter_map(|m| library_source(m).map(|s| (*m, s)))
                .collect(),
            project: None,
        }
    }

    /// The standard library, and the modules of a project, which `files` gives
    /// by their names. `entry` is the module the built file is.
    pub fn in_project(files: &'a dyn crate::Source, entry: Option<&'a str>) -> Library<'a> {
        Library {
            project: Some(Project { files, entry }),
            ..Library::standard()
        }
    }

    /// `in_project`, with `std.net` as `smith --watch` builds it, which keeps the
    /// socket a run listens on, as `watched_net` says.
    pub fn in_project_watched(files: &'a dyn crate::Source, entry: Option<&'a str>) -> Library<'a> {
        let mut library = Library::in_project(files, entry);
        let text = watched_net();
        for module in &mut library.modules {
            if module.0 == "net" {
                module.1 = text;
            }
        }
        library
    }

    /// The source of the module with a name written as in a `use`.
    pub(crate) fn source(&self, module: &str) -> Option<std::borrow::Cow<'a, str>> {
        match module.split_once('.') {
            Some(("std", short)) => self
                .modules
                .iter()
                .find(|(m, _)| *m == short)
                .map(|(_, s)| (*s).into()),
            _ if module == "std" => None,
            _ => self.project.as_ref()?.files.source(module).map(Into::into),
        }
    }
}

/// `std.net` as `smith --watch` builds it into the programs it runs, and into no
/// other. The handler of `net` there asks `smith --watch` for the socket it
/// listens on, and gives `.closed` from `accept` when `smith --watch` closes the
/// control socket. It is the module with three lines added to the two functions
/// that listen and accept, and the functions of `net_watched.smith` after it, so
/// that the module every other program is built with is not changed by it.
fn watched_net() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let mut text = library_text("net").to_string();
        let stop = "if told_to_stop() { return .err(.closed(address_text(l.host, l.port))) }";
        // What each place is, and what it is made.
        let additions = [
            (
                "fn system_listen_on(host: str, wanted: Port) -> Result[Listener, NetError] <alloc> {\n  port = number_of(wanted)\n".to_string(),
                "fn system_listen_on(host: str, wanted: Port) -> Result[Listener, NetError] <alloc> {\n  port = number_of(wanted)\n  if watched() { return watched_listen_on(host, port) }\n".to_string(),
            ),
            (
                "      if not on_windows() { wait_to_read(l.handle) }\n".to_string(),
                format!("      if not on_windows() {{ wait_to_read(l.handle) }}\n      {stop}\n"),
            ),
            (
                "  loop attempt in 0..64 {\n    fd = __system(3, l.handle, 0, 0, 0, 0, 0)\n".to_string(),
                format!("  loop attempt in 0..64 {{\n    {stop}\n    fd = __system(3, l.handle, 0, 0, 0, 0, 0)\n"),
            ),
        ];
        for (at, made) in additions {
            assert_eq!(
                text.matches(at.as_str()).count(),
                1,
                "std.net has changed under smith --watch: {at}"
            );
            text = text.replace(at.as_str(), &made);
        }
        text.push('\n');
        text.push_str(include_str!("net_watched.smith"));
        text
    })
}

/// The source of a module of the standard library, which errors in it show a
/// line of.
pub fn library_text(module: &str) -> &'static str {
    library_source(module).unwrap_or("")
}

/// How far apart the lines of two modules are, so that a place in a module is
/// told from one in the file or the prelude, and from one in another module,
/// by its line alone. The prelude is at line 0, the file below the first step,
/// and the module numbered `n` is at `n * STEP` and the lines after it.
pub(crate) const STEP: usize = 1 << 32;

/// The number of the first module of a project. Those of the standard library
/// are numbered below it, so that a place says which kind of module it is in.
const FIRST_PROJECT: usize = 1 << 16;

/// The module that holds the prelude, whose items are written as they are.
const PRELUDE_MODULE: &str = "std.prelude";

/// What kind of item of a module a lookup is for.
#[derive(Clone, Copy)]
enum Kind {
    Trait,
    Struct,
    Enum,
}

/// The name each item of each module of the standard library that the program
/// reads has in the file, by the module and the name the module gives it. The
/// file writes an item as the program lists it, as another name when the
/// program lists it with `as`, and with its module after it when the program
/// lists it not at all, so what the file calls an item says nothing of what
/// the item is.
#[derive(Clone, Default)]
struct Items(Vec<std::rc::Rc<Naming>>);

/// The place, among the file's traits, structs, or enums, of the item that the
/// library defines with the name `name` in the module `module`, written as
/// `std.serialize`, or `None` when the program does not read the module.
/// Every lookup of an item of the library goes through this function, so that
/// what the compiler leans on is in the calls of it, and a name that `std/`
/// no longer defines stops the compiler at once, saying which.
fn library_item(file: &File, items: &Items, module: &str, kind: Kind, name: &str) -> Option<usize> {
    // The module the item is in, or `None` for the prelude.
    let number = if module == PRELUDE_MODULE {
        None
    } else {
        let naming = items.0.iter().find(|n| n.name == module)?;
        if name.starts_with("__") {
            not_defined(module, kind, name)
        }
        Some(naming.number)
    };
    let home = |pos: Pos| match number {
        None => of_prelude(pos),
        Some(number) => module_of(pos) == number,
    };
    let at = match kind {
        Kind::Trait => (file.traits.iter()).position(|t| t.name == name && home(t.pos)),
        Kind::Struct => (file.structs.iter()).position(|s| s.name == name && home(s.pos)),
        Kind::Enum => (file.enums.iter()).position(|e| e.name == name && home(e.pos)),
    };
    Some(at.unwrap_or_else(|| not_defined(module, kind, name)))
}

/// An item of the prelude, which every program has.
/// A trait of the prelude that is read only for a program that calls for it,
/// or `None` when it is not read.
fn try_prelude_trait(file: &File, name: &str) -> Option<usize> {
    (file.traits.iter()).position(|t| t.name == name && of_prelude(t.pos))
}

fn prelude_item(file: &File, kind: Kind, name: &str) -> usize {
    library_item(file, &Items::default(), PRELUDE_MODULE, kind, name).expect("the prelude is read")
}

fn not_defined(module: &str, kind: Kind, name: &str) -> ! {
    let kind = match kind {
        Kind::Trait => "trait",
        Kind::Struct => "struct",
        Kind::Enum => "enum",
    };
    panic!(
        "internal error: the compiler needs the {kind} `{name}` of the module `{module}`, and the module does not define it"
    )
}

/// The module of the standard library, written as after `std.`, that a name
/// the prelude does not hold is imported from: the module that exports it,
/// the one that holds an effect the compiler builds, or `bits` for a size.
fn std_home(name: &str) -> Option<&'static str> {
    if PRELUDE_LISTED.contains(&name) {
        return None;
    }
    if SIZES.contains(&name) {
        return Some("bits");
    }
    STD_EXPORTS
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|(module, _)| *module)
}

/// Whether a name is one of the sizes, and the module is `std.bits`.
fn of_bits(module: &str, name: &str) -> bool {
    module == "std.bits" && SIZES.contains(&name)
}

/// Whether a definition is in a module of the library or of the project.
fn of_library(pos: Pos) -> bool {
    pos.line >= STEP
}

/// Whether a definition is in a module of the standard library, which the
/// compiler builds functions for.
fn of_std(pos: Pos) -> bool {
    (STEP..FIRST_PROJECT * STEP).contains(&pos.line)
}

/// The number of the module a place is in: 0 for the file and the prelude.
fn module_of(pos: Pos) -> usize {
    pos.line / STEP
}

/// Whether the field of a struct is private to the module that defines the
/// struct, and `at` is in another module. The prelude and the file are one
/// module, so a field of a struct of the prelude is read by any file.
fn hidden(def: &crate::parser::Struct, field: &crate::parser::Field, at: Pos) -> bool {
    field.public.is_none() && of_library(def.pos) && module_of(def.pos) != module_of(at)
}

/// A name as its module wrote it, without the module that renaming adds.
fn written_name(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

/// The error for a field that code outside its module reads or assigns. A
/// program cannot change a module of the library, so the fix there names the
/// function of that module that gives the field, as `raw_text` gives `text`,
/// when it has one.
fn private_field(def: &crate::parser::Struct, field: &str, at: Pos) -> Error {
    let name = written_name(&def.name);
    let error = Error::new(
        at,
        format!("the field `{field}` of `{name}` is private to the module that defines it"),
    );
    if !of_library(def.pos) {
        return error.with_fix(format!(
            "write `pub` before `{field}` where `{name}` is defined"
        ));
    }
    // The module that defines the struct writes the function in its source.
    let gives = LIBRARY_MODULES.iter().find_map(|m| {
        let source = library_text(m);
        if !source.contains(&format!("pub struct {name} {{")) {
            return None;
        }
        source.lines().find_map(|line| {
            let head = line.strip_prefix("pub fn ")?;
            let (called, rest) = head.split_once('(')?;
            let own = rest.split_once(')')?.0;
            let takes = own.split_once(": ").is_some_and(|(_, ty)| ty == name);
            (called.ends_with(&format!("_{field}")) && takes).then(|| called.to_string())
        })
    });
    match gives {
        Some(f) => error.with_fix(format!(
            "read it with `{f}`, which the module that defines `{name}` gives"
        )),
        None => error.with_fix(format!(
            "`{field}` is the module's own, and the module gives no function for it"
        )),
    }
}

/// The name of each module the places of a file say, by its number, as the map
/// of an executable writes it: `std.console`, `db.transaction`, and for the
/// file that is built, which is number 0, the module its path gives it, or
/// `main` when it has none.
fn module_names(labels: &[(usize, String)], library: &Library) -> Vec<(usize, String)> {
    let entry = library.project.as_ref().and_then(|p| p.entry);
    let mut names = vec![(0, entry.unwrap_or("main").to_string())];
    for (number, label) in labels {
        let name = if *number < FIRST_PROJECT {
            format!("std.{label}")
        } else {
            label.trim_end_matches(".smith").replace('/', ".")
        };
        names.push((*number, name));
    }
    names
}

/// The errors with a place in a module, given as that module and the place
/// in its file. `labels` are the modules read, each by its number.
fn localize(errors: Vec<Error>, labels: &[(usize, String)]) -> Vec<Error> {
    errors
        .into_iter()
        .map(|mut e| {
            if of_library(e.pos)
                && let Some((_, label)) = labels.iter().find(|(n, _)| *n == module_of(e.pos))
            {
                e.module = Some(label.clone());
                e.pos.line %= STEP;
            }
            e
        })
        .collect()
}

/// The items of a module of the standard library, which are read when the
/// program reaches them: where each is in the source, and which are read.
struct Pieces {
    list: Vec<Piece>,
    /// The places in `list` of the items that are neither a `use` nor an
    /// `impl`, in the order of their names.
    named: Vec<u32>,
    /// The places of the `impl`s, in the order of their traits' names.
    impls: Vec<u32>,
    /// Which are read.
    loaded: Vec<bool>,
}

impl Pieces {
    fn new(source: &str, list: Vec<Piece>) -> Pieces {
        let name = |i: &u32| &source[list[*i as usize].name.0..list[*i as usize].name.1];
        let mut named: Vec<u32> = Vec::with_capacity(list.len());
        let mut impls: Vec<u32> = Vec::new();
        for (i, p) in list.iter().enumerate() {
            match p.kind {
                PieceKind::Use | PieceKind::Without => {}
                PieceKind::Impl => impls.push(i as u32),
                _ => named.push(i as u32),
            }
        }
        named.sort_by(|a, b| name(a).cmp(name(b)));
        impls.sort_by(|a, b| name(a).cmp(name(b)));
        let loaded = vec![false; list.len()];
        Pieces {
            list,
            named,
            impls,
            loaded,
        }
    }

    /// The items with the name, among `of`, which is `named` or `impls`.
    fn find<'p>(&'p self, source: &str, of: &'p [u32], name: &str) -> &'p [u32] {
        let at = |i: &u32| {
            let p = &self.list[*i as usize];
            &source[p.name.0..p.name.1]
        };
        let from = of.partition_point(|i| at(i) < name);
        let to = from + of[from..].partition_point(|i| at(i) == name);
        &of[from..to]
    }

    /// The items with the name, among `of`, that are of one of the kinds.
    fn find_kind<'p>(
        &'p self,
        source: &str,
        of: &'p [u32],
        name: &str,
        kinds: &'p [PieceKind],
    ) -> impl Iterator<Item = usize> + 'p {
        (self.find(source, of, name).iter())
            .map(|&i| i as usize)
            .filter(|&i| kinds.contains(&self.list[i].kind))
    }
}

/// What a name that is looked up may be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    /// A struct or an enum, where the name is written as a type.
    Type,
    /// A function, or a struct or an enum, where a call or a value names it.
    Any,
}

impl Ask {
    /// The kinds of item the name may be.
    fn kinds(self) -> &'static [PieceKind] {
        self.kinds_not(0)
    }

    /// Which kinds of item are asked for, as bits.
    fn bits(self) -> u8 {
        match self {
            Ask::Type => 2,
            Ask::Any => 3,
        }
    }

    /// The kinds of item the name may be that the asks in `done` did not ask for.
    fn kinds_not(self, done: u8) -> &'static [PieceKind] {
        match self.bits() & !done {
            1 => &[PieceKind::Fn],
            2 => &[PieceKind::Struct, PieceKind::Enum],
            3 => &[PieceKind::Fn, PieceKind::Struct, PieceKind::Enum],
            _ => &[],
        }
    }
}

/// A module that a `use` names, read.
struct Module<'a> {
    /// The source, which the places of `pieces` are in.
    source: std::borrow::Cow<'a, str>,
    /// The items that are read when they are reached, when the module is
    /// divided into them.
    pieces: Option<Pieces>,
    /// The names the module defines that are read with it, which are all of
    /// them unless it is divided.
    defined: HashSet<String>,
    /// The names the compiler builds, which the module's source only wraps.
    held: HashSet<String>,
    /// The name as a `use` writes it, such as `std.list` or `db.transaction`.
    name: String,
    /// What an error in it says the module is: the name of a module of the
    /// standard library, and the path of the file of a module of a project.
    label: String,
    /// The tokens of the file, at the lines of the module's number.
    tokens: Vec<crate::lexer::Token>,
    file: File,
    /// The names a `use` may list: those its source writes with `pub`, the
    /// functions of the traits it writes with `pub`, and the operations of the
    /// effects it writes with `pub`.
    exports: Vec<String>,
    /// The names the module lists from others, each with the module it takes
    /// it from, by its place among those read.
    imports: Vec<Import>,
    /// The modules the module imports itself, each by the name it is written
    /// as, with the module by its place among those read.
    aliases: Vec<(String, usize)>,
    /// The names the file that is built lists from it, as they are.
    entry_imported: Vec<String>,
    /// The names the file that is built lists from it with `as`, each with
    /// the name it is given.
    entry_aliased: Vec<(String, String)>,
    /// The number the module's places are at, as `STEP` says.
    number: usize,
}

impl Module<'_> {
    /// Whether the module defines the name as a struct, an enum, or a trait,
    /// and not as a function.
    fn defines_type(&self, name: &str) -> bool {
        let mut ty = (self.file.structs.iter().any(|s| s.name == name))
            || (self.file.enums.iter().any(|e| e.name == name))
            || (self.file.traits.iter().any(|t| t.name == name));
        let mut function = self.file.functions.iter().any(|f| f.name == name);
        if let Some(p) = &self.pieces {
            for &i in p.find(&self.source, &p.named, name) {
                match p.list[i as usize].kind {
                    PieceKind::Fn => function = true,
                    PieceKind::Struct | PieceKind::Enum | PieceKind::Trait => ty = true,
                    PieceKind::Use | PieceKind::Impl | PieceKind::Effect | PieceKind::Without => {}
                }
            }
        }
        ty && !function
    }

    /// The names of the structs, enums, and traits the module defines that are
    /// read with it and are not the names of functions.
    fn own_types(&self) -> Vec<String> {
        let functions: Set<&str> = self
            .file
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        (self.file.structs.iter().map(|s| &s.name))
            .chain(self.file.enums.iter().map(|e| &e.name))
            .chain(self.file.traits.iter().map(|t| &t.name))
            .filter(|n| !functions.contains(n.as_str()))
            .cloned()
            .collect()
    }

    /// Whether the module defines the name.
    fn defines(&self, name: &str) -> bool {
        self.defined.contains(name)
            || self
                .pieces
                .as_ref()
                .is_some_and(|p| !p.find(&self.source, &p.named, name).is_empty())
    }

    /// Whether the module exports the name.
    fn exports(&self, name: &str) -> bool {
        self.exports.iter().any(|e| e == name)
            || self.pieces.as_ref().is_some_and(|p| {
                p.find(&self.source, &p.named, name)
                    .iter()
                    .any(|&i| p.list[i as usize].public)
            })
    }
}

/// A name a module lists from another.
struct Import {
    /// The name in the module that lists it, which `as` may have given.
    written: String,
    /// The name in the module it is from.
    name: String,
    /// The module it is from, by its place among those read.
    from: usize,
}

/// The tokens with each place moved to the module's lines.
fn shifted(tokens: &[crate::lexer::Token], base: usize) -> Vec<crate::lexer::Token> {
    use crate::lexer::{Piece, Tok};
    tokens
        .iter()
        .map(|t| {
            let tok = match &t.tok {
                Tok::Interp(pieces) => Tok::Interp(
                    pieces
                        .iter()
                        .map(|p| match p {
                            Piece::Expr(inner) => Piece::Expr(shifted(inner, base)),
                            text => text.clone(),
                        })
                        .collect(),
                ),
                other => other.clone(),
            };
            let mut pos = t.pos;
            pos.line += base;
            crate::lexer::Token {
                tok,
                pos,
                note: t.note.clone(),
            }
        })
        .collect()
}

/// The names a file defines: its functions, types, and traits, and the
/// functions of its traits.
fn defined_names(file: &File) -> HashSet<&str> {
    (file.functions.iter().map(|f| f.name.as_str()))
        .chain(file.structs.iter().map(|s| s.name.as_str()))
        .chain(file.enums.iter().map(|e| e.name.as_str()))
        .chain(file.effects.iter().map(|e| e.name.as_str()))
        .chain(file.traits.iter().flat_map(|t| {
            std::iter::once(t.name.as_str()).chain(t.functions.iter().map(|f| f.name.as_str()))
        }))
        .collect()
}

/// The modules a file imports, read once each, and what each file lists from
/// them.
struct Resolver<'l, 'a> {
    library: &'l Library<'a>,
    /// Whether every module is read whole, as a check of the library does.
    whole: bool,
    read: Vec<Module<'a>>,
    /// Every module a read was tried for, by number, with its label.
    labels: Vec<(usize, String)>,
    /// The place among `read` of each module read, by name. `None` is one
    /// whose file is wrong, which is said once.
    by_name: HashMap<String, Option<usize>>,
    /// The modules whose own `use` lines are being read, the last the newest.
    open: Vec<String>,
    errors: Vec<Error>,
    warnings: Vec<Error>,
    /// Where a name written with a module is refused, which the checker
    /// would refuse again as a call of a name that is not there.
    refused: Vec<Pos>,
    /// The errors for names of the standard library that a file uses and does
    /// not import, which `refused` must not remove.
    unimported: Vec<Error>,
    next_std: usize,
    next_project: usize,
    /// The module of the standard library that exports each name, the first of
    /// them where two do, which is worked out when an error first needs it.
    homes: std::cell::OnceCell<HashMap<String, &'a str>>,
}

/// Whose `use` lines are being read.
#[derive(Clone, Copy)]
enum Reader {
    /// The file that is built.
    Entry,
    /// A module, by its place among those read.
    Module(usize),
}

impl<'l, 'a> Resolver<'l, 'a> {
    fn new(library: &'l Library<'a>) -> Self {
        Resolver {
            library,
            whole: false,
            read: Vec::new(),
            labels: Vec::new(),
            by_name: HashMap::new(),
            open: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            refused: Vec::new(),
            unimported: Vec::new(),
            next_std: 1,
            next_project: FIRST_PROJECT,
            homes: std::cell::OnceCell::new(),
        }
    }

    /// The module of the standard library that exports each name. A module's
    /// source is not lexed or parsed for it: an item that a module exports is
    /// written with `pub` at the start of a line, as `fn`, `struct`, `enum`,
    /// `trait`, or `effect`, and the operations of an effect that are exported
    /// are its indented `pub fn` lines, so the names are read off those lines.
    fn homes(&self) -> &HashMap<String, &'a str> {
        self.homes.get_or_init(|| {
            let mut homes = HashMap::new();
            for (module, _) in &self.library.modules {
                let Some(source) = self.library.source(&format!("std.{module}")) else {
                    continue;
                };
                let mut in_effect = false;
                for line in source.lines() {
                    let operation = in_effect.then(|| line.trim_start().strip_prefix("pub fn "));
                    in_effect &= line != "}";
                    let Some(rest) = operation.flatten().or_else(|| {
                        let rest = [
                            "pub fn ",
                            "pub struct ",
                            "pub enum ",
                            "pub trait ",
                            "pub effect ",
                        ]
                        .into_iter()
                        .find_map(|start| line.strip_prefix(start))?;
                        in_effect = line.starts_with("pub effect ");
                        Some(rest)
                    }) else {
                        continue;
                    };
                    let end = rest
                        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .unwrap_or(rest.len());
                    homes.entry(rest[..end].to_string()).or_insert(*module);
                }
            }
            homes
        })
    }

    /// Reads the module with the name, or says what is wrong in it. A module
    /// of the standard library is divided into its items, and an item is read
    /// when the program reaches it.
    fn read(&mut self, name: &str) -> Option<usize> {
        let source = self.library.source(name)?;
        let standard = name.starts_with("std.");
        let (label, number) = if standard {
            self.next_std += 1;
            (name[4..].to_string(), self.next_std - 1)
        } else {
            self.next_project += 1;
            (
                format!("{}.smith", name.replace('.', "/")),
                self.next_project - 1,
            )
        };
        self.labels.push((number, label.clone()));
        // A module that has items with whole number parameters, or uses those of
        // another, has its copies of them among its tokens, whole.
        let mut items = crate::values::exported_items(name);
        let mut made = crate::values::module_tokens(name, number * STEP);
        // A module read by itself, as the whole library is checked, has its items
        // with whole number parameters left out.
        if made.is_none()
            && let Some((tokens, errors, names)) =
                crate::values::module_alone(&source, number * STEP)
        {
            items = names;
            made = Some((tokens, errors));
        }
        let divided = if standard && !self.whole && made.is_none() {
            outline(&source)
        } else {
            None
        };
        if let Some(list) = divided {
            return self.read_divided(name, source, list, label, number);
        }
        let tokens = match made {
            Some((tokens, errors)) => {
                self.errors
                    .extend(errors.into_iter().map(|e| e.in_module(&label)));
                tokens
            }
            None => match crate::lexer::lex(source.as_bytes()) {
                Ok(tokens) => shifted(&tokens, number * STEP),
                Err(errors) => {
                    self.errors
                        .extend(errors.into_iter().map(|e| e.in_module(&label)));
                    self.by_name.insert(name.to_string(), None);
                    return None;
                }
            },
        };
        let mut file = match crate::parser::parse_library(&tokens) {
            Ok(file) => file,
            Err(errors) => {
                self.errors.extend(errors);
                self.by_name.insert(name.to_string(), None);
                return None;
            }
        };
        self.errors.append(&mut file.unbuilt);
        let defined: HashSet<String> = defined_names(&file).into_iter().map(String::from).collect();
        let held = held_by_the_compiler(&file)
            .into_iter()
            .map(String::from)
            .collect();
        let mut exports = file.exports.clone();
        exports.extend(
            (file.traits.iter())
                .filter(|t| file.exports.contains(&t.name))
                .flat_map(|t| t.functions.iter().map(|f| f.name.clone())),
        );
        exports.extend(items);
        let module = Module {
            source,
            pieces: None,
            defined,
            held,
            name: name.to_string(),
            label,
            tokens,
            file,
            exports,
            imports: Vec::new(),
            aliases: Vec::new(),
            entry_imported: Vec::new(),
            entry_aliased: Vec::new(),
            number,
        };
        self.open_module(module)
    }

    /// Reads a module of the standard library that is divided into items: its
    /// `use` lines, its traits, and what the compiler leans on, and the places
    /// of the rest.
    fn read_divided(
        &mut self,
        name: &str,
        source: std::borrow::Cow<'a, str>,
        list: Vec<Piece>,
        label: String,
        number: usize,
    ) -> Option<usize> {
        let mut file = File::default();
        let mut held: HashSet<String> = HashSet::new();
        for p in &list {
            let text = &source[p.text.0..p.text.1];
            let word = &source[p.name.0..p.name.1];
            let read_now = match p.kind {
                PieceKind::Use | PieceKind::Trait | PieceKind::Effect | PieceKind::Without => true,
                PieceKind::Struct | PieceKind::Enum => LEANED_ON.contains(&(name, word)),
                PieceKind::Fn | PieceKind::Impl => false,
            };
            // A function whose whole body calls what the compiler builds is
            // read to tell whether it is one.
            let wrapper = p.kind == PieceKind::Fn && may_be_wrapper(text);
            if p.kind != PieceKind::Use
                && p.kind != PieceKind::Impl
                && PRELUDE_TYPES.contains(&word)
                && matches!(p.kind, PieceKind::Struct | PieceKind::Enum)
            {
                held.insert(word.to_string());
            }
            if !read_now && !wrapper {
                continue;
            }
            let tokens = match crate::lexer::lex(text.as_bytes()) {
                Ok(tokens) => shifted(&tokens, number * STEP + p.line),
                Err(errors) => {
                    self.errors
                        .extend(errors.into_iter().map(|e| e.in_module(&label)));
                    self.by_name.insert(name.to_string(), None);
                    return None;
                }
            };
            let mut found = match crate::parser::parse_library(&tokens) {
                Ok(found) => found,
                Err(errors) => {
                    self.errors.extend(errors);
                    self.by_name.insert(name.to_string(), None);
                    return None;
                }
            };
            if wrapper {
                held.extend(
                    found
                        .functions
                        .iter()
                        .filter(|f| is_wrapper(f))
                        .map(|f| f.name.clone()),
                );
            }
            if read_now {
                file.uses.append(&mut found.uses);
                file.traits.append(&mut found.traits);
                file.effects.append(&mut found.effects);
                file.withouts.append(&mut found.withouts);
                if p.kind == PieceKind::Effect {
                    file.functions.append(&mut found.functions);
                }
                file.structs.append(&mut found.structs);
                file.enums.append(&mut found.enums);
                file.exports.append(&mut found.exports);
                self.errors.append(&mut found.unbuilt);
            }
        }
        held.extend(held_by_the_compiler(&file).into_iter().map(String::from));
        let defined: HashSet<String> = defined_names(&file).into_iter().map(String::from).collect();
        let mut exports = file.exports.clone();
        exports.extend(
            (file.traits.iter())
                .filter(|t| file.exports.contains(&t.name))
                .flat_map(|t| t.functions.iter().map(|f| f.name.clone())),
        );
        let pieces = Pieces::new(&source, list);
        let module = Module {
            source,
            pieces: Some(pieces),
            defined,
            held,
            name: name.to_string(),
            label,
            tokens: Vec::new(),
            file,
            exports,
            imports: Vec::new(),
            aliases: Vec::new(),
            entry_imported: Vec::new(),
            entry_aliased: Vec::new(),
            number,
        };
        self.open_module(module)
    }

    /// Adds the module read, and reads the modules its own `use` lines name,
    /// while it is open, so that a module that reaches itself again is told.
    fn open_module(&mut self, module: Module<'a>) -> Option<usize> {
        let name = module.name.clone();
        let divided = module.pieces.is_some();
        let slot = self.read.len();
        self.read.push(module);
        self.by_name.insert(name.clone(), Some(slot));
        self.open.push(name);
        let uses = self.read[slot].file.uses.clone();
        if divided {
            // Nothing in a module of the standard library is wrong, and
            // what it writes is not looked for.
            let written: HashSet<String> = uses
                .iter()
                .flat_map(|u| &u.names)
                .map(|n| n.alias.as_ref().map_or(n.name.clone(), |(a, _)| a.clone()))
                .collect();
            self.resolve(
                Reader::Module(slot),
                &uses,
                &HashSet::new(),
                &written,
                &[],
                &[],
            );
        } else {
            let defined = self.read[slot].defined.clone();
            let written = self.read[slot].file.written.clone();
            let qualified = self.read[slot].file.qualified.clone();
            let used = scope::used_names(&self.read[slot].file);
            self.resolve(
                Reader::Module(slot),
                &uses,
                &defined,
                &written,
                &qualified,
                &used,
            );
        }
        self.open.pop();
        Some(slot)
    }

    /// The place among `read` of the module a `use` names, or an error at the
    /// module's name.
    fn module(&mut self, module: &str, at: Pos) -> Option<usize> {
        if let Some(project) = &self.library.project
            && project.entry == Some(module)
        {
            self.errors.push(
                Error::new(
                    at,
                    format!("`{module}` is the file being built, and nothing imports from it"),
                )
                .with_fix("move what the other module needs into a module of its own"),
            );
            return None;
        }
        match self.by_name.get(module) {
            Some(Some(_)) if self.open.iter().any(|m| m == module) => {
                let from = self.open.iter().position(|m| m == module).unwrap_or(0);
                let ring: Vec<String> =
                    self.open[from..].iter().map(|m| format!("`{m}`")).collect();
                self.errors.push(
                    Error::new(
                        at,
                        format!(
                            "{} import each other in a ring, so no order builds them",
                            ring.join(", ")
                        ),
                    )
                    .with_fix("move what they all need into a module that imports none of them"),
                );
                None
            }
            Some(slot) => *slot,
            None => {
                let found = self.read(module);
                if found.is_none() && !self.by_name.contains_key(module) {
                    let standard = module.split('.').next() == Some("std");
                    let mut error = Error::new(at, format!("there is no module `{module}`"));
                    error = if standard {
                        let known: Vec<String> = self
                            .library
                            .modules
                            .iter()
                            .map(|(m, _)| format!("`std.{m}`"))
                            .collect();
                        error.with_fix(format!("import from one of {}", known.join(", ")))
                    } else {
                        error.with_fix(format!(
                            "write the module in the file `{}.smith`",
                            module.replace('.', "/")
                        ))
                    };
                    self.errors.push(error);
                }
                found
            }
        }
    }

    /// Checks the `use` lines of a file: each names a module and lists names it
    /// exports, once each, none of which the file defines or the prelude holds,
    /// and no name comes from two modules. A name is the one `as` gives it
    /// when it has one, and a module imported itself is a name too, which
    /// is its last segment unless `as` gives another. Each module named is
    /// read, once, and its own `use` lines are checked in turn. A name that is
    /// listed and never written in the file is a warning, as is a module
    /// imported and never written before a name.
    fn resolve(
        &mut self,
        reader: Reader,
        uses: &[Use],
        defined: &HashSet<String>,
        written: &HashSet<String>,
        qualified: &[Qualified],
        used: &[(String, Pos)],
    ) {
        let mut modules: Vec<String> = Vec::new();
        let mut taken: Vec<(String, String)> = Vec::new();
        // The modules imported themselves: the name each is written as, the
        // module, and its place among those read.
        let mut named: Vec<(String, String, usize)> = Vec::new();
        for u in uses {
            let parts: Vec<&str> = u.module.iter().map(|(n, _)| n.as_str()).collect();
            let module = parts.join(".");
            let at = u.module[0].1;
            let Some(slot) = self.module(&module, at) else {
                continue;
            };
            if modules.contains(&module) {
                self.errors.push(
                    Error::new(
                        at,
                        format!("`{module}` is already listed by an earlier `use`"),
                    )
                    .with_fix(format!(
                        "list every name from `{module}` in its first `use`"
                    )),
                );
                continue;
            }
            modules.push(module.clone());
            if !u.listed {
                let (called, pos) = u.called();
                let error = if let Some((_, other, _)) = named.iter().find(|(n, ..)| n == called) {
                    Some(
                        Error::new(
                            pos,
                            format!("`{module}` and `{other}` are both `{called}`: a name means one thing"),
                        )
                        .with_fix(format!("write `as` after `{module}` to give it another name")),
                    )
                } else {
                    self.clash(called, pos, defined, &taken, "a module")
                };
                match error {
                    Some(error) => self.errors.push(error),
                    None => {
                        named.push((called.to_string(), module.clone(), slot));
                        if let Reader::Module(of) = reader {
                            self.read[of].aliases.push((called.to_string(), slot));
                        }
                        // What a divided module writes is not looked for, so
                        // a module it imports is not found to be unused.
                        let divided =
                            matches!(reader, Reader::Module(of) if self.read[of].pieces.is_some());
                        if !divided && !qualified.iter().any(|q| q.written.0 == called) {
                            self.warnings.push(
                                Error::new(pos, format!("`{module}` is imported and never used"))
                                    .with_fix("delete the `use`"),
                            );
                        }
                    }
                }
                continue;
            }
            let mut listed: Vec<&str> = Vec::new();
            for item in &u.names {
                let (name, pos) = (item.name.as_str(), item.pos);
                let written_as = item.alias.as_ref().map_or(name, |(a, _)| a.as_str());
                let at_name = item.alias.as_ref().map_or(pos, |(_, at)| *at);
                let error = if listed.contains(&name) {
                    Some(
                        Error::new(pos, format!("`{name}` is listed twice from `{module}`"))
                            .with_fix(format!("list `{name}` once")),
                    )
                } else if item.alias.is_none() && defined.contains(name) {
                    Some(
                        Error::new(
                            pos,
                            format!(
                                "`{name}` is imported, and this file defines it: a name means one thing"
                            ),
                        )
                        .with_fix(format!(
                            "rename the definition, or delete `{name}` from the `use`"
                        )),
                    )
                } else if item.alias.is_none() && PRELUDE_LISTED.contains(&name) {
                    Some(
                        Error::new(
                            pos,
                            format!("`{name}` is in the prelude, so it needs no `use`"),
                        )
                        .with_fix(format!("delete `{name}` from the `use`")),
                    )
                } else if !self.read[slot].exports(name) && !of_bits(&module, name) {
                    Some(self.missing(&module, slot, name, pos))
                } else if item.alias.is_some() && (self.fixed(slot, name) || of_bits(&module, name))
                {
                    Some(Error::unsupported(
                        at_name,
                        "`as` on a name the compiler builds",
                    ))
                } else if item.alias.is_some() && self.of_trait(slot, name) {
                    Some(Error::unsupported(at_name, "`as` on a function of a trait"))
                } else if item.alias.is_some()
                    && let Some(error) = self.clash(written_as, at_name, defined, &taken, "a name")
                {
                    Some(error)
                } else if let Some((_, from)) = taken.iter().find(|(n, _)| n == written_as) {
                    Some(
                        Error::new(
                            at_name,
                            format!("`{written_as}` is listed from `{from}` and from `{module}`: a name means one thing"),
                        )
                        .with_fix(format!("delete `{name}` from one of them, or give it another name with `as`")),
                    )
                } else if let Some((_, other, _)) = named.iter().find(|(n, ..)| n == written_as) {
                    Some(Error::new(
                        at_name,
                        format!(
                            "`{written_as}` is the name of the module `{other}`: a name means one thing"
                        ),
                    ))
                } else {
                    None
                };
                listed.push(name);
                match error {
                    Some(error) => self.errors.push(error),
                    None => {
                        match reader {
                            Reader::Entry => match &item.alias {
                                Some((alias, _)) => self.read[slot]
                                    .entry_aliased
                                    .push((name.to_string(), alias.clone())),
                                None => self.read[slot].entry_imported.push(name.to_string()),
                            },
                            Reader::Module(of) => self.read[of].imports.push(Import {
                                written: written_as.to_string(),
                                name: name.to_string(),
                                from: slot,
                            }),
                        }
                        taken.push((written_as.to_string(), module.clone()));
                        if !written.contains(written_as) {
                            self.warnings.push(
                                Error::new(
                                    at_name,
                                    format!(
                                        "`{written_as}` is imported from `{module}` and never used"
                                    ),
                                )
                                .with_fix(format!("delete `{name}` from the `use`")),
                            );
                        }
                    }
                }
            }
        }
        self.unimported(reader, uses, defined, &named, used);
        for q in qualified {
            let Some((_, module, slot)) = named.iter().find(|(n, ..)| *n == q.written.0) else {
                continue;
            };
            let (name, pos) = (q.name.0.as_str(), q.name.1);
            if !self.read[*slot].exports(name) {
                let error = self.missing(module, *slot, name, pos);
                self.errors.push(error);
                self.refused.push(q.written.1);
            } else if self.fixed(*slot, name) {
                self.errors.push(Error::unsupported(
                    pos,
                    "calling a name the compiler builds through its module",
                ));
                self.refused.push(q.written.1);
            }
        }
    }

    /// An error at the first use of each name of the standard library that
    /// the file writes, and neither defines nor lists nor is a module's own
    /// export. The fix names the module to import it from. Every use of the
    /// name is refused, as the checker would say of the others that there is
    /// no such name.
    fn unimported(
        &mut self,
        reader: Reader,
        uses: &[Use],
        defined: &HashSet<String>,
        named: &[(String, String, usize)],
        used: &[(String, Pos)],
    ) {
        let own: Vec<&str> = match reader {
            Reader::Entry => Vec::new(),
            Reader::Module(of) => (self.read[of].exports.iter())
                .map(String::as_str)
                .chain(
                    if self.read[of].name == "std.bits" {
                        SIZES
                    } else {
                        &[]
                    }
                    .iter()
                    .copied(),
                )
                .collect(),
        };
        let listed: Vec<&str> = (uses.iter().flat_map(|u| &u.names))
            .map(|n| {
                n.alias
                    .as_ref()
                    .map_or(n.name.as_str(), |(a, _)| a.as_str())
            })
            .collect();
        let mut first: Vec<&str> = Vec::new();
        let mut errors = Vec::new();
        for (name, pos) in used {
            let Some(module) = std_home(name) else {
                continue;
            };
            if defined.contains(name.as_str())
                || own.contains(&name.as_str())
                || listed.contains(&name.as_str())
                || named.iter().any(|(n, ..)| n == name)
            {
                continue;
            }
            self.refused.push(*pos);
            if first.contains(&name.as_str()) {
                continue;
            }
            first.push(name);
            errors.push(
                Error::new(
                    *pos,
                    format!("`{name}` is in `std.{module}`, and this file does not import it"),
                )
                .with_fix(format!("add `use std.{module} ({name})`")),
            );
        }
        self.unimported.extend(errors);
    }

    /// The error for a name that a file gives to a module or to another
    /// name, when the file already has the name: it defines it, the prelude
    /// holds it, or an earlier `use` took it.
    fn clash(
        &self,
        name: &str,
        pos: Pos,
        defined: &HashSet<String>,
        taken: &[(String, String)],
        what: &str,
    ) -> Option<Error> {
        if defined.contains(name) {
            Some(
                Error::new(
                    pos,
                    format!("`{name}` is given to {what}, and this file defines it: a name means one thing"),
                )
                .with_fix("write `as` to give it another name"),
            )
        } else if PRELUDE_LISTED.contains(&name) {
            Some(
                Error::new(pos, format!("`{name}` is given to {what}, and the prelude holds it: a name means one thing"))
                    .with_fix("write `as` to give it another name"),
            )
        } else {
            taken.iter().find(|(n, _)| n == name).map(|(_, from)| {
                Error::new(
                    pos,
                    format!("`{name}` is given to {what}, and is already listed from `{from}`: a name means one thing"),
                )
                .with_fix("write `as` to give it another name")
            })
        }
    }

    /// Whether a name of a module is one the compiler builds, and writes as
    /// it is: an effect, a function of a trait of the library, and a name the
    /// module only wraps.
    fn fixed(&self, slot: usize, name: &str) -> bool {
        let module = &self.read[slot];
        module.name.starts_with("std.")
            && ((module.file.traits.iter()).any(|t| t.functions.iter().any(|f| f.name == name))
                || module.held.contains(name))
    }

    /// Whether a name of a module is a function of one of its traits.
    fn of_trait(&self, slot: usize, name: &str) -> bool {
        (self.read[slot].file.traits.iter()).any(|t| t.functions.iter().any(|f| f.name == name))
    }

    /// The error for a name that a module does not export: one it keeps
    /// private, or one it does not have.
    fn missing(&self, module: &str, slot: usize, name: &str, pos: Pos) -> Error {
        let read = &self.read[slot];
        if read.defines(name) {
            return Error::new(pos, format!("`{name}` is private to `{module}`"))
                .with_fix(format!("write `pub` before `{name}` in `{}`", read.label));
        }
        let mut error = Error::new(pos, format!("`{module}` has no name `{name}`"));
        let home = self.homes().get(name).copied();
        if let Some(home) = home {
            error = error.with_fix(format!("write `use std.{home} ({name})`"));
        } else if SIZES.contains(&name) {
            error = error.with_fix(format!("write `use std.bits ({name})`"));
        }
        error
    }
}

/// Whether the text of a function may be one whose whole body is one call of a
/// function the compiler builds, which is told without lexing it: the last line
/// of its body begins with the name of such a function. `is_wrapper` decides,
/// for what this lets by.
fn may_be_wrapper(text: &str) -> bool {
    let Some(body) = text.trim_end().strip_suffix('}') else {
        return false;
    };
    let call = body.trim_end().rsplit(['{', '\n']).next().unwrap_or("");
    call.trim_start().starts_with("__")
}

/// Whether a function's whole body is one call of a function the compiler
/// builds, with the function's own parameters in order.
fn is_wrapper(f: &Arc<Function>) -> bool {
    matches!(&f.body[..], [Stmt::Expr(Expr::Call(c))]
    if c.qualifier.is_none()
        && c.callee.starts_with("__")
        && c.args.len() == f.params.len()
        && c.args.iter().zip(&f.params).all(|(a, p)| {
            a.name.is_none()
                && !a.after
                && matches!(&a.value, Expr::Name { name, .. } if *name == p.name)
        }))
}

/// Whether the compiler holds the item of a module already, and builds it
/// itself: a function whose body is one call of a function the compiler
/// builds with its parameters, in order, and a type or a trait the prelude defines. The module's source
/// names these, and the compiler supplies them as it did before the bodies
/// of a module were compiled.
fn held_by_the_compiler(module: &File) -> HashSet<&str> {
    (module
        .functions
        .iter()
        .filter(|f| is_wrapper(f))
        .map(|f| f.name.as_str()))
    .chain(
        (module.structs.iter().map(|s| s.name.as_str()))
            .chain(module.enums.iter().map(|e| e.name.as_str()))
            .filter(|n| PRELUDE_TYPES.contains(n)),
    )
    .chain(
        (module.traits.iter().map(|t| t.name.as_str()))
            .filter(|n| PRELUDE_TRAITS.contains(n) || TRAITS_NOT_BUILT.contains(n)),
    )
    .collect()
}

/// The file with the items of the modules it imports among its own. A name
/// the file lists keeps its name. Every other name of a module is its own,
/// written with the module after it, so that it is not the name of anything in
/// the file or in another module, and the file cannot write it. A module that
/// lists names of another writes them as that module does. Items are checked
/// and compiled only when the program reaches them, as the file's are, and an
/// item of a module of the standard library is not read until then.
fn splice<'a>(
    mut file: File,
    mut modules: Vec<Module<'a>>,
    errors: &mut Vec<Error>,
    asks_function: bool,
) -> (File, Items, Lazy<'a>) {
    // The names each module writes as they are, since the file lists them, or
    // since the compiler builds them.
    let kept: Vec<Set<String>> = modules
        .iter()
        .map(|module| {
            let standard = module.name.starts_with("std.");
            let mut kept: Set<String> = module.entry_imported.iter().cloned().collect();
            if standard {
                kept.extend(
                    (module.file.traits.iter())
                        .filter(|t| module.exports.contains(&t.name))
                        .flat_map(|t| t.functions.iter().map(|f| f.name.clone())),
                );
                kept.extend(module.held.iter().cloned());
            }
            kept
        })
        .collect();
    // What a name a module defines is written as in the file: as it is, as
    // the file that is built names it with `as`, or with the module after it.
    let final_of = |from: usize, name: &str| -> Option<String> {
        let module = &modules[from];
        let standard = module.name.starts_with("std.");
        if !module.defines(name) || (standard && name.starts_with("__")) {
            return None;
        }
        Some(own_final(
            &module.name,
            &module.entry_aliased,
            &kept[from],
            name,
        ))
    };
    let written =
        |from: usize, name: &str| final_of(from, name).unwrap_or_else(|| name.to_string());
    // The traits each file can name, which a call of one of their functions
    // written without the trait is told from by: those the file defines and
    // those it lists.
    let mut scopes: Vec<(usize, Vec<String>)> = Vec::new();
    let entry_traits: Vec<String> = (file.traits.iter())
        .map(|t| t.name.clone())
        .chain(modules.iter().enumerate().flat_map(|(i, m)| {
            (m.file.traits.iter())
                .filter(|t| {
                    m.entry_imported.contains(&t.name)
                        || m.entry_aliased.iter().any(|(n, _)| *n == t.name)
                })
                .filter_map(|t| final_of(i, &t.name))
                .collect::<Vec<_>>()
        }))
        .collect();
    scopes.push((0, entry_traits));
    for (i, module) in modules.iter().enumerate() {
        let own = (module.file.traits.iter()).filter_map(|t| final_of(i, &t.name));
        let listed = module.imports.iter().filter_map(|i| {
            let t = modules[i.from]
                .file
                .traits
                .iter()
                .find(|t| t.name == i.name)?;
            Some(written(i.from, &t.name))
        });
        scopes.push((module.number, own.chain(listed).collect()));
    }
    // What each module writes of other modules' names, which is the same
    // for what is read now and for what is read later.
    struct Plan {
        imports: Vec<(String, String, usize)>,
        through: HashMap<String, HashMap<String, String>>,
        /// Whether the module is read whole, as one that is not of the
        /// standard library is.
        whole: bool,
    }
    let mut plans: Vec<Plan> = (0..modules.len())
        .map(|i| {
            // The modules it imports itself are written before a name of
            // their own, as `transaction.begin`, which is the name written
            // alone.
            let imports: Vec<(String, String, usize)> = modules[i]
                .imports
                .iter()
                .map(|import| {
                    let home = modules[import.from].number;
                    (
                        import.written.clone(),
                        written(import.from, &import.name),
                        home,
                    )
                })
                .collect();
            let through: HashMap<String, HashMap<String, String>> = modules[i]
                .aliases
                .iter()
                .map(|(alias, from)| {
                    let names = all_names(&modules[*from])
                        .into_iter()
                        .filter(|name| final_of(*from, name).is_some())
                        .map(|name| (name.clone(), written(*from, &name)))
                        .collect();
                    (alias.clone(), names)
                })
                .collect();
            let whole = modules[i].pieces.is_none();
            Plan {
                imports,
                through,
                whole,
            }
        })
        .collect();
    let mut resolution = Resolution::default();
    let namings: Vec<std::rc::Rc<Naming>> = plans
        .iter_mut()
        .zip(kept)
        .enumerate()
        .map(|(i, (plan, kept))| {
            // The names it lists that are types, and the types it defines when
            // it is read whole, which are written as they are.
            let mut types: Set<String> = (modules[i].imports.iter())
                .filter(|import| modules[import.from].defines_type(&import.name))
                .map(|import| import.written.clone())
                .collect();
            if plan.whole {
                types.extend(modules[i].own_types());
            }
            let mut effects: Set<String> = (modules[i].imports.iter())
                .filter(|import| {
                    (modules[import.from].file.effects.iter()).any(|e| e.name == import.name)
                })
                .map(|import| import.written.clone())
                .collect();
            effects.extend(modules[i].file.effects.iter().map(|e| e.name.clone()));
            let naming = std::rc::Rc::new(Naming {
                name: modules[i].name.clone(),
                number: modules[i].number,
                standard: modules[i].name.starts_with("std."),
                kept,
                aliased: modules[i].entry_aliased.clone(),
                imports: std::mem::take(&mut plan.imports),
                whole: (modules[i].aliases.iter())
                    .map(|(_, from)| modules[*from].number)
                    .collect(),
                types,
                effects,
            });
            resolution.modules.insert(modules[i].number, naming.clone());
            naming
        })
        .collect();
    let mut rest: Vec<LazyModule<'a>> = Vec::new();
    for (i, plan) in plans.into_iter().enumerate() {
        let standard = modules[i].name.starts_with("std.");
        let Plan {
            imports: _,
            through,
            whole,
        } = plan;
        if whole {
            let tokens = std::mem::take(&mut modules[i].tokens);
            let naming = &namings[i];
            let tokens = renamed_tokens(
                &tokens,
                &|n| {
                    naming.rename(n, || {
                        (modules[i].defined.contains(n), naming.types.contains(n))
                    })
                },
                &through,
                &mut |pos, name, to| {
                    if cfg!(debug_assertions) {
                        resolution
                            .written
                            .insert((pos, to.to_string()), name.to_string());
                    }
                },
            );
            let mut found = match crate::parser::parse_library(&tokens) {
                Ok(found) => found,
                Err(found) => {
                    errors.extend(found);
                    continue;
                }
            };
            if asks_function {
                crate::function_args::add(&mut found, &modules[i].source, modules[i].number);
            }
            let held = if standard {
                modules[i].held.clone()
            } else {
                HashSet::new()
            };
            add_items(&mut file, found, &held);
            continue;
        }
        let module = &mut modules[i];
        let mut lazy = LazyModule {
            naming: namings[i].clone(),
            number: module.number,
            source: std::mem::take(&mut module.source),
            pieces: module.pieces.take().unwrap_or_else(|| unreachable!()),
            defined: std::mem::take(&mut module.defined).into_iter().collect(),
            held: std::mem::take(&mut module.held).into_iter().collect(),
            through,
            parsed: Vec::new(),
            written: HashMap::new(),
            asks_function,
        };
        lazy.parsed = (0..lazy.pieces.list.len()).map(|_| None).collect();
        // The traits, and what the compiler leans on, are read at once.
        for at in 0..lazy.pieces.list.len() {
            let piece = lazy.pieces.list[at];
            let text = &lazy.source[piece.name.0..piece.name.1];
            let now = match piece.kind {
                PieceKind::Trait | PieceKind::Effect | PieceKind::Without => true,
                PieceKind::Struct | PieceKind::Enum => {
                    LEANED_ON.contains(&(lazy.naming.name.as_str(), text))
                }
                _ => false,
            };
            if now
                && !lazy.held.contains(text)
                && let Some(found) = lazy.read(at, errors)
            {
                add_items(&mut file, found, &HashSet::new());
            }
        }
        rest.push(lazy);
    }
    file.scopes = scopes;
    let items = Items(
        (namings.iter())
            .filter(|naming| naming.standard)
            .cloned()
            .collect(),
    );
    (file, items, {
        let mut filter = Filter {
            bits: vec![0; 1024],
        };
        for module in &rest {
            module.mark(&mut filter);
        }
        for naming in &namings {
            for name in &naming.types {
                filter.add(name, 0);
            }
            // What a module that is read whole lists is written as it lists it.
            for (written, _, _) in &naming.imports {
                filter.add(written, 0);
            }
        }
        Lazy {
            resolution,
            index: (rest.iter().enumerate())
                .map(|(m, module)| (module.number, m))
                .collect(),
            modules: rest,
            done: Map::default(),
            keys: Vec::new(),
            filter,
            written: Vec::new(),
            reads: 0,
        }
    })
}

/// Every name a module defines.
fn all_names(module: &Module) -> Vec<String> {
    let mut names: Vec<String> = module.defined.iter().cloned().collect();
    if let Some(p) = &module.pieces {
        names.extend(p.named.iter().map(|&i| {
            let piece = &p.list[i as usize];
            module.source[piece.name.0..piece.name.1].to_string()
        }));
    }
    names
}

/// The items of `found` among the file's, but for the names the compiler
/// builds, which are in `held`.
fn add_items(file: &mut File, found: File, held: &HashSet<String>) {
    let keep = |n: &str| !held.contains(n);
    file.functions
        .extend(found.functions.into_iter().filter(|f| keep(&f.name)));
    file.structs
        .extend(found.structs.into_iter().filter(|s| keep(&s.name)));
    file.enums
        .extend(found.enums.into_iter().filter(|e| keep(&e.name)));
    file.traits
        .extend(found.traits.into_iter().filter(|t| keep(&t.name)));
    file.effects.extend(found.effects);
    file.impls.extend(found.impls);
    file.withouts.extend(found.withouts);
    file.args.extend(found.args);
}

/// The modules of the standard library whose items are read as the program
/// reaches them.
struct Lazy<'a> {
    /// How each module wrote its names, and what each calls what.
    resolution: Resolution,
    modules: Vec<LazyModule<'a>>,
    /// The names looked up, and for each the asks whose items are all read.
    done: Map<String, u8>,
    /// The place among `modules` of each, by the module's number.
    index: Map<usize, usize>,
    /// What the file calls each name a module wrote, by the module's number,
    /// for the names that were asked.
    keys: Vec<Map<Box<str>, std::rc::Rc<str>>>,
    /// What the names of the items not read may look like.
    filter: Filter,
    /// The names the file that is built writes.
    written: Vec<String>,
    /// How many items `load` has read.
    reads: usize,
}

/// A set of the lengths and first letters that names have, so that a name
/// that is not one of a module's is told from one that may be without looking
/// it up.
struct Filter {
    bits: Vec<u64>,
}

impl Filter {
    fn at(len: usize, first: u8) -> (usize, u64) {
        let k = (len.min(255) << 8) | first as usize;
        (k / 64, 1 << (k % 64))
    }

    fn add(&mut self, name: &str, extra: usize) {
        let (word, bit) = Filter::at(
            name.len() + extra,
            name.as_bytes().first().copied().unwrap_or(0),
        );
        self.bits[word] |= bit;
    }

    fn maybe(&self, name: &str) -> bool {
        let (word, bit) = Filter::at(name.len(), name.as_bytes().first().copied().unwrap_or(0));
        self.bits[word] & bit != 0
    }
}

/// A module of the standard library whose items are read when they are
/// reached.
struct LazyModule<'a> {
    /// What the module calls what.
    naming: std::rc::Rc<Naming>,
    number: usize,
    source: std::borrow::Cow<'a, str>,
    pieces: Pieces,
    /// The names of what is read with the module.
    defined: Set<String>,
    /// The names the compiler builds, which are never read.
    held: Set<String>,
    through: HashMap<String, HashMap<String, String>>,
    /// The items read since, by their place among the module's.
    parsed: Vec<Option<File>>,
    /// The name each token of the items read was written as, by its place,
    /// where the tokens were rewritten.
    written: HashMap<(Pos, String), String>,
    /// Whether the file that is built writes `Function`, which a function of
    /// the module read is made a struct of arguments for.
    asks_function: bool,
}

impl LazyModule<'_> {
    /// Marks what the names of the items not read may look like: as the file
    /// writes them, with the module after them or as they are.
    fn mark(&self, filter: &mut Filter) {
        for p in &self.pieces.list {
            if matches!(p.kind, PieceKind::Fn | PieceKind::Struct | PieceKind::Enum) {
                let name = &self.source[p.name.0..p.name.1];
                filter.add(name, 1 + self.naming.name.len());
                // A name is written as it is in the module, and in the others
                // that list it.
                filter.add(name, 0);
            }
        }
        // What the module lists is written as it lists it.
        for (written, _, _) in &self.naming.imports {
            filter.add(written, 0);
        }
        for name in &self.naming.kept {
            filter.add(name, 0);
        }
        for (_, alias) in &self.naming.aliased {
            filter.add(alias, 0);
        }
    }

    fn defines(&self, name: &str) -> bool {
        self.defined.contains(name)
            || !self
                .pieces
                .find(&self.source, &self.pieces.named, name)
                .is_empty()
    }

    /// Whether the module defines the name as a function that is read when it
    /// is reached.
    fn defines_function(&self, name: &str) -> bool {
        !self.held.contains(name)
            && (self.pieces)
                .find_kind(&self.source, &self.pieces.named, name, &[PieceKind::Fn])
                .next()
                .is_some()
    }

    /// What the module's name is written as in the file, when it is not as the
    /// module writes it.
    fn rename(&self, name: &str) -> Option<String> {
        self.naming.rename(name, || {
            let found = self.pieces.find(&self.source, &self.pieces.named, name);
            let own_type = !found.is_empty() && self.all_types(found);
            (self.defined.contains(name) || !found.is_empty(), own_type)
        })
    }

    /// Whether the items at these places are all structs, enums, or traits.
    fn all_types(&self, found: &[u32]) -> bool {
        (found.iter()).all(|&i| {
            matches!(
                self.pieces.list[i as usize].kind,
                PieceKind::Struct | PieceKind::Enum | PieceKind::Trait
            )
        })
    }

    /// Whether the module defines the name as a struct, an enum, or a trait,
    /// and not as a function.
    fn defines_type(&self, name: &str) -> bool {
        let found = self.pieces.find(&self.source, &self.pieces.named, name);
        !found.is_empty() && self.all_types(found)
    }

    /// Reads the item at the place among the module's, with its names written
    /// as the file writes them.
    fn read(&mut self, at: usize, errors: &mut Vec<Error>) -> Option<File> {
        self.pieces.loaded[at] = true;
        let piece = self.pieces.list[at];
        let text = &self.source[piece.text.0..piece.text.1];
        let tokens = crate::lexer::lex(text.as_bytes()).ok()?;
        let tokens = shifted(&tokens, self.number * STEP + piece.line);
        let mut renamed: Vec<((Pos, String), String)> = Vec::new();
        let tokens = renamed_tokens(
            &tokens,
            &|n| self.rename(n),
            &self.through,
            &mut |pos, name, to| {
                if cfg!(debug_assertions) {
                    renamed.push(((pos, to.to_string()), name.to_string()));
                }
            },
        );
        self.written.extend(renamed);
        match crate::parser::parse_library(&tokens) {
            Ok(mut found) => {
                if self.asks_function {
                    crate::function_args::add(&mut found, &self.source, self.number);
                }
                Some(found)
            }
            Err(found) => {
                errors.extend(found);
                None
            }
        }
    }

    /// The names, as the module writes them, that the name as the file writes
    /// it may be, among those the module defines, and among those it lists
    /// from other modules when `listed` says so.
    fn originals<'n>(&'n self, written: &'n str, listed: bool) -> Vec<&'n str> {
        self.naming.originals(written, listed)
    }

    /// The places of the items that are not read, which are not traits or
    /// `impl`s, that the file may write as `written`, where a name is wanted
    /// as `want` says.
    fn provided(&self, written: &str, kinds: &[PieceKind]) -> Vec<usize> {
        let mut out = Vec::new();
        match written.split_once('@') {
            Some((name, module)) => {
                if module == self.naming.name {
                    self.provides(name, written, kinds, &mut out);
                }
            }
            None => {
                if self.naming.kept.contains(written) {
                    self.provides(written, written, kinds, &mut out);
                }
            }
        }
        for (from, alias) in &self.naming.aliased {
            if alias == written {
                self.provides(from, written, kinds, &mut out);
            }
        }
        out
    }

    /// The places of the items not read that the module writes as `name` and
    /// the file writes as `written`.
    fn provides(&self, name: &str, written: &str, kinds: &[PieceKind], out: &mut Vec<usize>) {
        if self.held.contains(name) {
            return;
        }
        let items = (self.pieces).find_kind(&self.source, &self.pieces.named, name, kinds);
        for i in items {
            if !self.pieces.loaded[i] && self.final_of(name, self.pieces.list[i].kind) == written {
                out.push(i);
            }
        }
    }

    /// What the file calls the item of the module that is written as `name` in
    /// it, of the kind the piece is.
    fn final_of<'n>(&self, name: &'n str, kind: PieceKind) -> std::borrow::Cow<'n, str> {
        match kind {
            PieceKind::Struct | PieceKind::Enum | PieceKind::Trait => self.naming.own_key(name),
            _ => std::borrow::Cow::Owned(self.naming.final_name(name)),
        }
    }

    /// Whether a struct or an enum that is not read is written as this.
    fn is_type(&self, written: &str) -> bool {
        self.originals(written, false).into_iter().any(|name| {
            (self.pieces)
                .find_kind(&self.source, &self.pieces.named, name, Ask::Type.kinds())
                .next()
                .is_some()
        })
    }

    /// The places of the `impl`s of the trait, written as `written`, that
    /// are not read and are for types that are all `reached`, except those
    /// the trait writes `out` for. `own` is the names the module writes of
    /// what is reached of its own, and `is_type` says whether a name that is
    /// not the module's own is a struct or an enum.
    fn impls_of(
        &self,
        written: &str,
        outs: &[bool],
        reached: &Set<&str>,
        own: &Set<&str>,
        loaded_type: &dyn Fn(&str) -> bool,
        lazy_type: &dyn Fn(&str) -> bool,
    ) -> Vec<usize> {
        let mut names: Vec<&str> = self.originals(written, true);
        // A trait the module does not define or list is written as it is.
        if !self.defines(written)
            && !self
                .naming
                .imports
                .iter()
                .any(|(from, _, _)| from == written)
        {
            names.push(written);
        }
        let mut out = Vec::new();
        for name in names {
            for &i in self.pieces.find(&self.source, &self.pieces.impls, name) {
                let i = i as usize;
                if self.pieces.loaded[i] {
                    continue;
                }
                let text = &self.source[self.pieces.list[i].text.0..self.pieces.list[i].text.1];
                let first = text.lines().next().unwrap_or("");
                let all = impl_arguments_all(first, name, |j, arg| {
                    outs.get(j).copied().unwrap_or(false)
                        || arg
                            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                            .filter(|w| !w.is_empty())
                            .all(|word| {
                                self.type_is_reached(word, reached, own, loaded_type, lazy_type)
                            })
                });
                if all {
                    out.push(i);
                }
            }
        }
        out
    }

    /// Whether a word in the head of an `impl` of the module is no struct or
    /// enum, or one that is reached.
    fn type_is_reached(
        &self,
        word: &str,
        reached: &Set<&str>,
        own: &Set<&str>,
        loaded_type: &dyn Fn(&str) -> bool,
        lazy_type: &dyn Fn(&str) -> bool,
    ) -> bool {
        if let Some((_, to, _)) = self.naming.imports.iter().find(|(from, _, _)| from == word) {
            return !(loaded_type(to) || lazy_type(to)) || reached.contains(to.as_str());
        }
        if self.defined.contains(word) {
            let to = self.naming.final_name(word);
            return !loaded_type(&to) || reached.contains(to.as_str());
        }
        if !self
            .pieces
            .find(&self.source, &self.pieces.named, word)
            .is_empty()
        {
            let kinds = Ask::Type.kinds();
            let is = (self.pieces)
                .find_kind(&self.source, &self.pieces.named, word, kinds)
                .next()
                .is_some();
            if !is {
                return true;
            }
            return own.contains(word)
                || (self.naming.kept.contains(word) && reached.contains(word))
                || self
                    .naming
                    .aliased
                    .iter()
                    .any(|(from, alias)| from == word && reached.contains(alias.as_str()));
        }
        !loaded_type(word) || reached.contains(word)
    }
}

/// Every name the items of a file look up, as `reach` does, but for those of
/// the prelude, which name nothing of a module, and those of the file that is
/// built, whose names the parser has.
fn names_in_items(file: &File, impls: &[Arc<Impl>], out: &mut dyn FnMut(&str, Pos, Ask)) {
    for f in file.functions.iter().filter(|f| of_library(f.pos)) {
        names_in_function(f, out);
    }
    for s in file.structs.iter().filter(|s| of_library(s.pos)) {
        for ty in s.fields.iter().map(|f| &f.ty).chain(&s.alias) {
            type_names_in(ty, &mut |n, pos| out(n, pos, Ask::Type));
        }
        bounds_named(&s.generics, &mut |n, pos| out(n, pos, Ask::Type));
        let defaults = s.fields.iter().filter_map(|f| f.default.as_ref());
        names_in_expressions(defaults.collect(), out);
    }
    for e in file.enums.iter().filter(|e| of_library(e.pos)) {
        for t in e.cases.iter().filter_map(|c| c.ty.as_ref()) {
            type_names_in(t, &mut |n, pos| out(n, pos, Ask::Type));
        }
        bounds_named(&e.generics, &mut |n, pos| out(n, pos, Ask::Type));
    }
    for t in file.traits.iter().filter(|t| of_library(t.pos)) {
        for f in &t.functions {
            names_in_function(f, out);
        }
        bounds_named(&t.generics, &mut |n, pos| out(n, pos, Ask::Type));
    }
    for im in impls.iter().filter(|im| of_library(im.pos)) {
        out(&im.name, im.pos, Ask::Type);
        for t in &im.args {
            type_names_in(t, &mut |n, pos| out(n, pos, Ask::Type));
        }
        bounds_named(&im.generics, &mut |n, pos| out(n, pos, Ask::Type));
        needs_named(&im.requires, &mut |n, pos| out(n, pos, Ask::Type));
        for f in &im.functions {
            names_in_function(f, out);
        }
    }
}

/// Every name a function looks up, as `reach` does.
fn names_in_function(f: &Function, out: &mut dyn FnMut(&str, Pos, Ask)) {
    types_named(f, &mut |n, pos| out(n, pos, Ask::Type));
    let defaults = f.params.iter().filter_map(|p| p.default.as_ref());
    let statement = f.result.as_ref().and_then(|t| t.statement.as_deref());
    let contracts = (f.requires.iter().chain(&f.ensures))
        .chain(&f.decreases)
        .chain(statement);
    // A name of a parameter is the parameter in the function, so it calls for
    // no function of the module that has one of that name.
    let params: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
    names_in_expressions(
        values(&f.body).chain(defaults).chain(contracts).collect(),
        &mut |name, pos, ask| {
            if !params.contains(&name) {
                out(name, pos, ask)
            }
        },
    );
    needs_named(&f.needs, &mut |n, pos| out(n, pos, Ask::Type));
}

/// Every name expressions look up, as `reach` does.
fn names_in_expressions(values: Vec<&Expr>, out: &mut dyn FnMut(&str, Pos, Ask)) {
    for e in exprs(values.iter().copied()) {
        match e {
            Expr::Field { of, .. } => {
                if let Expr::Name { name, pos } = &**of {
                    out(name, *pos, Ask::Any);
                }
            }
            Expr::Name { name, pos } => out(name, *pos, Ask::Any),
            Expr::Call(call) => match &call.qualifier {
                Some((q, pos)) => out(q, *pos, Ask::Any),
                None => out(&call.callee, call.name_pos, Ask::Any),
            },
            Expr::Loop(l) if l.over.is_some() => out("Iterate", Pos::START, Ask::Any),
            _ => {}
        }
    }
}

/// Whether `ok` holds of each argument of the trait in the first line of an
/// `impl`, as written, with its place among them.
fn impl_arguments_all(line: &str, name: &str, mut ok: impl FnMut(usize, &str) -> bool) -> bool {
    let Some(at) = line.find(name) else {
        return true;
    };
    let rest = &line[at + name.len()..];
    let Some(rest) = rest.strip_prefix('[') else {
        return true;
    };
    let mut depth = 0;
    let mut from = 0;
    let mut place = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth == 0 => return ok(place, &rest[from..i]),
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                if !ok(place, &rest[from..i]) {
                    return false;
                }
                place += 1;
                from = i + 1;
            }
            _ => {}
        }
    }
    true
}

impl<'a> Lazy<'a> {
    /// The places, among the items of the modules, of those that the program
    /// reaches and that are not read: those written by a name looked up, and
    /// the `impl`s of the traits reached for the types reached.
    fn wanted(&self, file: &File, names: &Names, reached: &Reached) -> Vec<(usize, usize)> {
        if self.modules.is_empty() {
            return Vec::new();
        }
        // What the file calls the traits, structs, and enums that are reached.
        let mut trait_keys = Vec::new();
        let mut type_keys = Vec::new();
        for item in &reached.items {
            match *item {
                Item::Trait(k) => {
                    let t = &file.traits[k];
                    trait_keys.push(self.resolution.def_key(t.pos, &t.name));
                }
                Item::Struct(s) => {
                    let s = &file.structs[s];
                    type_keys.push(self.resolution.def_key(s.pos, &s.name));
                }
                Item::Enum(e) => {
                    let e = &file.enums[e];
                    type_keys.push(self.resolution.def_key(e.pos, &e.name));
                }
                Item::Fn(_) | Item::Impl(_) => {}
            }
        }
        let traits: Set<&str> = trait_keys.iter().map(|k| &**k).collect();
        let types: Set<&str> = type_keys.iter().map(|k| &**k).collect();
        let loaded_type =
            |n: &str| matches!(names.types.get(n), Some(Item::Struct(_) | Item::Enum(_)));
        let mut out: Vec<(usize, usize)> = Vec::new();
        for (m, module) in self.modules.iter().enumerate() {
            let own: Set<&str> = types
                .iter()
                .filter_map(|t| match t.split_once('@') {
                    Some((name, home)) if home == module.naming.name => Some(name),
                    _ => None,
                })
                .collect();
            for &name in &traits {
                let outs: Vec<bool> = match names.types.get(name) {
                    Some(&Item::Trait(k)) => (file.traits[k].generics.iter())
                        .map(|g| g.out.is_some())
                        .collect(),
                    _ => Vec::new(),
                };
                out.extend(
                    module
                        .impls_of(name, &outs, &types, &own, &loaded_type, &|n| {
                            self.modules.iter().any(|m| m.is_type(n))
                        })
                        .into_iter()
                        .map(|i| (m, i)),
                );
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// What the file calls the struct, enum, or trait that the module that
    /// wrote `name` at `pos` calls it.
    fn key<'k>(
        resolution: &'k Resolution,
        index: &Map<usize, usize>,
        modules: &[LazyModule],
        pos: Pos,
        name: &'k str,
    ) -> std::borrow::Cow<'k, str> {
        use std::borrow::Cow;
        let module = module_of(pos);
        // The file and the prelude write every name as it is.
        let Some(naming) = (module != 0)
            .then(|| resolution.modules.get(&module))
            .flatten()
        else {
            return Cow::Borrowed(name);
        };
        if let Some((_, to, _)) = naming.imports.iter().find(|(from, _, _)| from == name) {
            return Cow::Borrowed(to);
        }
        let own = naming.types.contains(name)
            || (index.get(&module)).is_some_and(|&m| modules[m].defines_type(name));
        if own {
            return naming.own_key(name);
        }
        // A function the module defines is what it calls the name, and the
        // file calls it with the module after it, unless the file lists it.
        if (index.get(&module)).is_some_and(|&m| modules[m].defines_function(name)) {
            return Cow::Owned(naming.final_name(name));
        }
        Cow::Borrowed(name)
    }

    /// Finds the items, not read, that a name written at `pos` calls for, once
    /// for each kind of ask.
    #[inline]
    fn note(&mut self, name: &str, pos: Pos, ask: Ask, work: &mut Vec<(usize, usize)>) {
        if !self.filter.maybe(name) {
            return;
        }
        // The file and the prelude write every name as it is, and a module
        // writes a name the same way wherever it is written.
        let at = module_of(pos);
        let cached: std::rc::Rc<str>;
        let key: &str = if at == 0 {
            name
        } else {
            let hit = self.keys.get(at).and_then(|m| m.get(name)).cloned();
            cached = match hit {
                Some(key) => key,
                None => {
                    let key: std::rc::Rc<str> =
                        Lazy::key(&self.resolution, &self.index, &self.modules, pos, name)
                            .as_ref()
                            .into();
                    if self.keys.len() <= at {
                        self.keys.resize_with(at + 1, Default::default);
                    }
                    self.keys[at].insert(name.into(), key.clone());
                    key
                }
            };
            &cached
        };
        let done = self.done.get(key).copied().unwrap_or(0);
        let kinds = ask.kinds_not(done);
        if kinds.is_empty() {
            return;
        }
        for (m, module) in self.modules.iter().enumerate() {
            work.extend(module.provided(key, kinds).into_iter().map(|i| (m, i)));
        }
        match self.done.get_mut(key) {
            Some(bits) => *bits |= ask.bits(),
            None => {
                self.done.insert(key.to_string(), ask.bits());
            }
        }
    }

    /// Reads the items, which are not read yet, that the file names or that
    /// are `wanted`, and the items that the names in what is read call for,
    /// until there are no more. `file` is named by `names`, which is given
    /// every name it looks up.
    fn load(
        &mut self,
        wanted: &[(usize, usize)],
        written: &[String],
        names: impl FnOnce(&mut dyn FnMut(&str, Pos, Ask)),
        traits: &[String],
        errors: &mut Vec<Error>,
    ) {
        let mut work: Vec<(usize, usize)> = wanted.to_vec();
        for name in written {
            self.note(name, Pos::START, Ask::Any, &mut work);
        }
        names(&mut |name, pos, want| self.note(name, pos, want, &mut work));
        while let Some((m, i)) = work.pop() {
            if self.modules[m].pieces.loaded[i] {
                continue;
            }
            let module = &mut self.modules[m];
            let Some(mut found) = module.read(i, errors) else {
                continue;
            };
            self.reads += 1;
            traits::split_functions_needs(&mut found.functions, traits);
            let written = std::mem::take(&mut self.modules[m].written);
            self.resolution.written.extend(written);
            names_in_items(&found, &found.impls, &mut |name, pos, want| {
                self.note(name, pos, want, &mut work)
            });
            self.modules[m].parsed[i] = Some(found);
        }
    }

    /// Puts the items read among the file's, in the order of the modules and
    /// of their source.
    fn append(&self, file: &mut File) {
        for module in &self.modules {
            for found in module.parsed.iter().flatten() {
                file.functions.extend(found.functions.iter().cloned());
                file.structs.extend(found.structs.iter().cloned());
                file.enums.extend(found.enums.iter().cloned());
                file.impls.extend(found.impls.iter().cloned());
                file.args.extend(found.args.iter().cloned());
            }
        }
    }
}

/// An error for each `impl` that is neither in the module of its trait nor in
/// the module of one of the types it is for, which is a module that defines
/// one of them.
fn placement(file: &File, errors: &mut Vec<Error>) {
    let defined = defined_names(file);
    for im in &file.impls {
        let types = im.args.iter().flat_map(type_names);
        if !defined.contains(im.name.as_str())
            && !types.into_iter().any(|(n, _)| defined.contains(n))
        {
            errors.push(
                Error::new(
                    im.pos,
                    format!(
                        "this `impl` of `{}` is in a module that defines neither the trait nor a type it is for",
                        im.name
                    ),
                )
                .with_fix("move it to the module of the trait or of the type"),
            );
        }
    }
}

/// A warning for each definition `main` does not reach, at its name, and
/// for each binding whose name is never read, at the name.
fn warnings(file: &File, reached: &[Item]) -> Vec<Error> {
    let mut out = Vec::new();
    let reached: Set<Item> = reached.iter().copied().collect();
    let mut dead = |item: Item, pos: Pos, what: &dyn Fn() -> String| {
        if !reached.contains(&item) && !of_prelude(pos) && !of_library(pos) {
            let what = what();
            out.push(
                Error::new(
                    pos,
                    format!("{what} is dead code: `main` does not reach it"),
                )
                .with_fix("delete it, or use it from code that `main` reaches"),
            );
        }
    };
    for (i, f) in
        (file.functions.iter().enumerate()).filter(|(_, f)| !crate::parser::is_operation(f))
    {
        if !f.name.starts_with("lambda@") {
            dead(Item::Fn(i), f.pos, &|| format!("the function `{}`", f.name));
        }
    }
    // The struct of the arguments of a function is made for `Function`, and is
    // not written, so it is nothing for the program to delete.
    for (i, s) in (file.structs.iter().enumerate()).filter(|(_, s)| !s.name.contains('.')) {
        dead(Item::Struct(i), s.pos, &|| {
            format!(
                "the {} `{}`",
                if s.alias.is_some() { "alias" } else { "struct" },
                s.name
            )
        });
    }
    for (i, e) in file.enums.iter().enumerate() {
        dead(Item::Enum(i), e.pos, &|| format!("the enum `{}`", e.name));
    }
    for (i, t) in file.traits.iter().enumerate() {
        dead(Item::Trait(i), t.pos, &|| format!("the trait `{}`", t.name));
    }
    for (i, im) in file.impls.iter().enumerate() {
        let what = || {
            let args: Vec<String> = im.args.iter().map(written_type).collect();
            format!("the implementation `{}[{}]`", im.name, args.join(", "))
        };
        dead(Item::Impl(i), im.pos, &what);
    }
    let bodies =
        (file.functions.iter().map(|f| &**f)).chain(file.impls.iter().flat_map(|im| &im.functions));
    for f in bodies.filter(|f| !of_library(f.pos)) {
        unused_bindings(file, f, &mut out);
    }
    out
}

/// A warning for each binding of the function whose name its body never
/// reads. A name is defined once in a function, so any read of it is a
/// read of that binding.
fn unused_bindings(file: &File, f: &Function, out: &mut Vec<Error>) {
    let proof_given = |value: &Expr| matches!(value, Expr::Call(c) if file.functions.iter().any(|g| g.name == c.callee && is_proof(g)));
    // What `arith` is asked to prove in the body: a sum, an index, or a call of
    // a function that owes something. A proof a call gave is read by it, as it
    // is a fact `arith` may use, though no line writes its name. It is looked
    // for only where a proof is given.
    let gives_proof = statements(&f.body)
        .into_iter()
        .any(|s| matches!(s, Stmt::Define { value, .. } if proof_given(value)));
    let asks_arith = gives_proof
        && exprs(values(&f.body)).into_iter().any(|e| match e {
            Expr::Arith { .. } | Expr::Index { .. } | Expr::Slice { .. } => true,
            Expr::Call(c) => file
                .functions
                .iter()
                .any(|g| g.name == c.callee && !g.requires.is_empty() && !is_proof(g)),
            _ => false,
        });
    let read: Vec<&str> = (exprs(values(&f.body)).into_iter())
        .filter_map(|e| match e {
            Expr::Name { name, .. } => Some(name.as_str()),
            // `sale.discount(x)` reads `sale`, which holds the function.
            Expr::Call(Call {
                qualifier: Some((q, _)),
                ..
            }) => Some(q.as_str()),
            _ => None,
        })
        .collect();
    for stmt in statements(&f.body) {
        let defined = match stmt {
            Stmt::Define { value, .. } if asks_arith && proof_given(value) => Vec::new(),
            Stmt::Define { name, pos, .. } => vec![(name.as_str(), *pos)],
            Stmt::Destructure { pattern, .. } => pattern_names(pattern),
            Stmt::Assign { .. } | Stmt::Expr(_) => Vec::new(),
        };
        for (name, pos) in defined {
            // `_` defines no name, so there is none to read.
            if name != "_" && !read.contains(&name) {
                out.push(
                    Error::new(
                        pos,
                        format!("`{name}` is defined in `{}` and never read", f.name),
                    )
                    .with_fix(format!("delete the binding, or read `{name}`")),
                );
            }
        }
    }
}

/// Each bare name in a pattern, with where it is, from left to right.
fn pattern_names(pattern: &Pattern) -> Vec<(&str, Pos)> {
    match pattern {
        Pattern::Bind { name, pos, .. } => vec![(name.as_str(), *pos)],
        Pattern::Case { inner, .. } => inner.as_deref().map_or(Vec::new(), pattern_names),
        Pattern::Tuple { items, .. } | Pattern::List { items, .. } => {
            items.iter().flat_map(pattern_names).collect()
        }
        Pattern::Number { .. } | Pattern::Text { .. } => Vec::new(),
    }
}

/// A struct or an enum, by its place among those of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Def {
    Struct(usize),
    Enum(usize),
}

/// The struct or the enum a type is, if it is one.
fn head(t: &Ty) -> Option<Def> {
    match t {
        Ty::Struct(s, _) => Some(Def::Struct(*s)),
        Ty::Enum(e, _) => Some(Def::Enum(*e)),
        _ => None,
    }
}

/// The types a struct or an enum holds: those of its fields, or those its
/// cases hold.
fn held_types(file: &File, d: Def) -> Vec<&Type> {
    match d {
        Def::Struct(s) => (file.structs[s].fields.iter().map(|f| &f.ty))
            .chain(&file.structs[s].alias)
            .collect(),
        Def::Enum(e) => file.enums[e]
            .cases
            .iter()
            .filter_map(|c| c.ty.as_ref())
            .collect(),
    }
}

/// The struct or the enum the name written at `pos` is, if it is one.
fn def_at(defs: &Defs, pos: Pos, name: &str) -> Option<Def> {
    match defs.type_at(pos, name) {
        Some(Item::Struct(s)) => Some(Def::Struct(s)),
        Some(Item::Enum(e)) => Some(Def::Enum(e)),
        _ => None,
    }
}

/// Finds the structs and enums that hold themselves with no `List` between,
/// directly or through others, each of which is kept as the address of a
/// node of its scalars. Of those `reached`, refuses each that no value can be
/// made of, at its name, and each type that holds itself as a parameter of a
/// function it holds.
fn cycles(defs: &mut Defs, reached: &[Def], errors: &mut Vec<Error>) {
    let file = defs.file;
    let all: Vec<Def> = (0..file.structs.len())
        .map(Def::Struct)
        .chain((0..file.enums.len()).map(Def::Enum))
        .collect();
    // What each holds in place, and what it names anywhere in its types.
    let edges = |in_place: bool| -> Vec<Vec<Def>> {
        (all.iter())
            .map(|&d| {
                let mut to = Vec::new();
                for ty in held_types(file, d) {
                    let names = match in_place {
                        true => held_names(defs, ty),
                        false => named(ty),
                    };
                    to.extend(
                        names
                            .into_iter()
                            .filter_map(|(n, pos)| def_at(defs, pos, n)),
                    );
                }
                to
            })
            .collect()
    };
    let (in_place, anywhere) = (edges(true), edges(false));
    let index = |d: Def| all.iter().position(|&a| a == d).expect("a definition");
    // Whether `to` is reached from `from` by one step or more.
    let reaches = |graph: &[Vec<Def>], from: Def, to: Def| {
        let mut seen: Vec<Def> = graph[index(from)].clone();
        let mut k = 0;
        while k < seen.len() {
            if seen[k] == to {
                return true;
            }
            for &next in &graph[index(seen[k])] {
                if !seen.contains(&next) {
                    seen.push(next);
                }
            }
            k += 1;
        }
        false
    };
    for &d in &all {
        let boxed = match d {
            Def::Struct(s) => file.structs[s].alias.is_none() && reaches(&in_place, d, d),
            Def::Enum(_) => reaches(&in_place, d, d),
        };
        match d {
            Def::Struct(s) => defs.boxed[s] = boxed,
            Def::Enum(e) => defs.boxed_enums[e] = boxed,
        }
    }
    for &d in reached {
        for ty in held_types(file, d) {
            for (name, pos) in function_params(ty) {
                if let Some(t) = def_at(defs, pos, name)
                    && (t == d || reaches(&anywhere, t, d))
                {
                    errors.push(Error::new(
                        pos,
                        format!(
                            "a type may not hold itself as a parameter of a function it holds, as `{}` does through `{name}`",
                            def_name(file, d)
                        ),
                    ));
                }
            }
        }
    }
    let pos = |d: Def| match d {
        Def::Struct(s) => file.structs[s].pos,
        Def::Enum(e) => file.enums[e].pos,
    };
    let unmade: Vec<Def> = (reached.iter().copied())
        .filter(|&d| match d {
            Def::Struct(s) => defs.boxed[s],
            Def::Enum(e) => defs.boxed_enums[e],
        })
        .filter(|&d| !buildable_def(defs, d, &[], &mut Vec::new()))
        .collect();
    for &d in &unmade {
        // A cycle of types none of which can be made is refused once, at the
        // name of the one declared first.
        let first = (unmade.iter())
            .filter(|&&o| o == d || reaches(&in_place, d, o) && reaches(&in_place, o, d))
            .min_by_key(|&&o| (pos(o).line, pos(o).col))
            .copied();
        if first == Some(d) {
            let name = def_name(file, d);
            errors.push(Error::new(
                pos(d),
                format!(
                    "no value of `{name}` can be made, as every way to make one needs a `{name}` already"
                ),
            ));
        }
        match d {
            Def::Struct(s) => defs.cyclic[s] = true,
            Def::Enum(e) => defs.cyclic_enums[e] = true,
        }
    }
}

/// The name a struct or an enum is declared with.
fn def_name(file: &File, d: Def) -> &str {
    match d {
        Def::Struct(s) => &file.structs[s].name,
        Def::Enum(e) => &file.enums[e].name,
    }
}

/// Every name in a type, in any place, with where it is.
fn named(t: &Type) -> Vec<(&str, Pos)> {
    fn names<'a>(t: &'a Type, out: &mut Vec<(&'a str, Pos)>) {
        if !t.name.is_empty() {
            out.push((t.name.as_str(), t.pos));
        }
        for a in &t.args {
            names(a, out);
        }
    }
    let mut out = Vec::new();
    names(t, &mut out);
    out
}

/// Every name in the types of the parameters of each function type in a
/// type, with where it is.
fn function_params(t: &Type) -> Vec<(&str, Pos)> {
    let mut out = Vec::new();
    if t.function {
        for p in &t.args[..t.args.len().saturating_sub(1)] {
            out.extend(named(p));
        }
    }
    for a in &t.args {
        out.extend(function_params(a));
    }
    out
}

/// Whether a value of the struct or the enum can be made, with each of its
/// type parameters one that can be made as `params` says, when those in
/// `making` are taken as ones that cannot, as they are being made: a struct
/// whose every field can be made, or an enum with a case that holds nothing
/// or holds what can be made.
fn buildable_def(defs: &Defs, d: Def, params: &[bool], making: &mut Vec<(Def, Vec<bool>)>) -> bool {
    let key = (d, params.to_vec());
    if making.contains(&key) {
        return false;
    }
    making.push(key);
    let file = defs.file;
    let generics = match d {
        Def::Struct(s) => &file.structs[s].generics,
        Def::Enum(e) => &file.enums[e].generics,
    };
    let env: Vec<(&str, bool)> = (generics.iter())
        .enumerate()
        .map(|(i, g)| (g.name.as_str(), params.get(i).copied().unwrap_or(true)))
        .collect();
    let can = |t: &Type, making: &mut Vec<(Def, Vec<bool>)>| buildable(defs, t, &env, making);
    let found = match d {
        Def::Struct(s) => (file.structs[s].fields.iter())
            .map(|f| &f.ty)
            .chain(&file.structs[s].alias)
            .all(|t| can(t, making)),
        Def::Enum(e) => {
            (file.enums[e].cases.iter()).any(|c| c.ty.as_ref().is_none_or(|t| can(t, making)))
        }
    };
    making.pop();
    found
}

/// Whether a value of the type can be made, with the type parameters of
/// `env` ones that can be made as it says.
fn buildable(
    defs: &Defs,
    t: &Type,
    env: &[(&str, bool)],
    making: &mut Vec<(Def, Vec<bool>)>,
) -> bool {
    if t.function {
        return true;
    }
    if let Some(&(_, can)) = env.iter().find(|(n, _)| *n == t.name) {
        return can;
    }
    if let Some(d) = def_at(defs, t.pos, &t.name) {
        let params: Vec<bool> = t
            .args
            .iter()
            .map(|a| buildable(defs, a, env, making))
            .collect();
        return buildable_def(defs, d, &params, making);
    }
    // A list or a slice may be empty, and a tuple or an array holds its parts.
    if matches!(t.name.as_str(), "List" | "Slice") {
        return true;
    }
    t.args.iter().all(|a| buildable(defs, a, env, making))
}

/// Every name in a type that the type holds in place, with where it is: its
/// own, and those of the types in its brackets, but not those of the
/// elements of a `List`, which are held out of line.
fn held_names<'a>(defs: &Defs, t: &'a Type) -> Vec<(&'a str, Pos)> {
    fn names<'a>(defs: &Defs, t: &'a Type, out: &mut Vec<(&'a str, Pos)>) {
        if !t.name.is_empty() {
            out.push((t.name.as_str(), t.pos));
        }
        if t.function || t.name == "List" && defs.type_at(t.pos, "List").is_none() {
            return;
        }
        for a in &t.args {
            names(defs, a, out);
        }
    }
    let mut out = Vec::new();
    names(defs, t, &mut out);
    out
}

/// Every name in a type, with where it is: its own, and those of the
/// types in its brackets.
fn type_names(t: &Type) -> Vec<(&str, Pos)> {
    fn names<'a>(t: &'a Type, out: &mut Vec<(&'a str, Pos)>) {
        if !t.name.is_empty() {
            out.push((t.name.as_str(), t.pos));
        }
        for a in &t.args {
            names(a, out);
        }
    }
    let mut out = Vec::new();
    names(t, &mut out);
    out
}

/// Gives `out` every name in a type, with where it is.
fn type_names_in(t: &Type, out: &mut impl FnMut(&str, Pos)) {
    if !t.name.is_empty() {
        out(t.name.as_str(), t.pos);
    }
    for a in &t.args {
        type_names_in(a, out);
    }
}

/// The error for assigning to `name`, or to a field of it, where `name`,
/// the name at `at` among those of the body, is not a `mut`.
fn not_a_var(body: &Body, name: &str, pos: Pos, at: usize) -> Error {
    let error = Error::new(
        pos,
        format!("`{name}` is not a `mut`, so it cannot be assigned to"),
    );
    if at < body.params {
        error.with_fix(format!(
            "declare a `mut` that starts as the parameter, as in `mut new_{name} = {name}`"
        ))
    } else {
        error.with_fix(format!(
            "declare it with `mut`: write `mut {name}` where it is defined"
        ))
    }
}

/// The value of each statement of a body, in order.
fn values(body: &[Stmt]) -> impl Iterator<Item = &Expr> {
    body.iter().map(|stmt| match stmt {
        Stmt::Define { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::Destructure { value, .. }
        | Stmt::Expr(value) => value,
    })
}

pub(crate) fn walk<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    out.push(expr);
    match expr {
        Expr::Call(call) => {
            if let Some(inner) = &call.through {
                walk(inner, out);
            }
            for arg in &call.args {
                walk(&arg.value, out);
            }
        }
        Expr::Field { of, .. } => walk(of, out),
        Expr::Arith { left, right, .. } => {
            walk(left, out);
            walk(right, out);
        }
        Expr::Case { payload, .. } => {
            if let Some(p) = payload {
                walk(p, out);
            }
        }
        Expr::Match(m) => {
            walk(&m.of, out);
            for arm in &m.arms {
                for value in values(&arm.body) {
                    walk(value, out);
                }
            }
        }
        Expr::Tuple { items, .. } | Expr::List { items, .. } => {
            for item in items {
                walk(item, out);
            }
        }
        Expr::Index { of, index, .. } => {
            walk(of, out);
            walk(index, out);
        }
        Expr::Slice { of, from, to, .. } => {
            walk(of, out);
            for end in [from, to].into_iter().flatten() {
                walk(end, out);
            }
        }
        Expr::Lambda(l) => {
            for value in values(&l.body) {
                walk(value, out);
            }
        }
        Expr::Loop(l) => {
            if let Some(r) = &l.range {
                walk(&r.from, out);
                walk(&r.to, out);
            }
            if let Some(o) = &l.over {
                walk(&o.value, out);
            }
            if let Some(e) = &l.decreases {
                walk(e, out);
            }
            for e in &l.invariants {
                walk(e, out);
            }
            for value in values(&l.body) {
                walk(value, out);
            }
        }
        Expr::For(f) => {
            for value in values(&f.round.body) {
                walk(value, out);
            }
        }
        Expr::Interp { parts, .. } => {
            for part in parts {
                if let Part::Expr(e) = part {
                    walk(e, out);
                }
            }
        }
        Expr::Return { value, .. } => {
            if let Some(v) = value {
                walk(v, out);
            }
        }
        Expr::Text { .. }
        | Expr::Number { .. }
        | Expr::Float { .. }
        | Expr::Unbuilt { .. }
        | Expr::Name { .. }
        | Expr::Break { .. } => {}
    }
}

/// Every statement of a body, and those of the blocks of each `match` and
/// each lambda in them, each before those inside it.
pub(crate) fn statements(body: &[Stmt]) -> Vec<&Stmt> {
    fn collect<'a>(body: &'a [Stmt], out: &mut Vec<&'a Stmt>) {
        for (stmt, value) in body.iter().zip(values(body)) {
            out.push(stmt);
            let mut found = Vec::new();
            blocks(value, &mut found);
            for block in found {
                collect(block, out);
            }
        }
    }
    let mut out = Vec::with_capacity(16);
    collect(body, &mut out);
    out
}

/// The blocks of each `match` and each lambda in the expression that is
/// not inside another's block.
pub(super) fn blocks<'a>(e: &'a Expr, out: &mut Vec<&'a [Stmt]>) {
    match e {
        Expr::Call(call) => {
            for arg in &call.args {
                blocks(&arg.value, out);
            }
        }
        Expr::Field { of, .. } => blocks(of, out),
        Expr::Arith { left, right, .. } => {
            blocks(left, out);
            blocks(right, out);
        }
        Expr::Case {
            payload: Some(p), ..
        } => blocks(p, out),
        Expr::Match(m) => {
            blocks(&m.of, out);
            out.extend(m.arms.iter().map(|arm| arm.body.as_slice()));
        }
        Expr::Tuple { items, .. } | Expr::List { items, .. } => {
            for item in items {
                blocks(item, out);
            }
        }
        Expr::Index { of, index, .. } => {
            blocks(of, out);
            blocks(index, out);
        }
        Expr::Slice { of, from, to, .. } => {
            blocks(of, out);
            for end in [from, to].into_iter().flatten() {
                blocks(end, out);
            }
        }
        Expr::Lambda(l) => out.push(&l.body),
        Expr::Loop(l) => {
            if let Some(r) = &l.range {
                blocks(&r.from, out);
                blocks(&r.to, out);
            }
            if let Some(o) = &l.over {
                blocks(&o.value, out);
            }
            if let Some(e) = &l.decreases {
                blocks(e, out);
            }
            for e in &l.invariants {
                blocks(e, out);
            }
            out.push(&l.body);
        }
        Expr::For(f) => out.push(&f.round.body),
        Expr::Interp { parts, .. } => {
            for part in parts {
                if let Part::Expr(e) = part {
                    blocks(e, out);
                }
            }
        }
        Expr::Return { value: Some(v), .. } => blocks(v, out),
        Expr::Case { .. }
        | Expr::Text { .. }
        | Expr::Number { .. }
        | Expr::Float { .. }
        | Expr::Unbuilt { .. }
        | Expr::Name { .. }
        | Expr::Break { .. }
        | Expr::Return { value: None, .. } => {}
    }
}

/// Every expression in the expressions, in order, each before the
/// expressions inside it, and then from left to right.
fn exprs<'a>(exprs: impl IntoIterator<Item = &'a Expr>) -> Vec<&'a Expr> {
    let mut inside = Vec::with_capacity(32);
    for e in exprs {
        walk(e, &mut inside);
    }
    inside
}

/// Every call in the expressions, in order, each outermost first and then
/// from left to right. A construction is written as a call, so it is among
/// them.
fn calls<'a>(values: impl IntoIterator<Item = &'a Expr>) -> Vec<&'a Call> {
    (exprs(values).into_iter())
        .filter_map(|e| match e {
            Expr::Call(call) => Some(call),
            _ => None,
        })
        .collect()
}

/// The error for a call in a default that leaves out that default, directly
/// or through the defaults it leaves out in turn.
fn default_cycle(pos: Pos) -> Error {
    Error::new(
        pos,
        "this call leaves out the default it is written in, directly or through others, \
         so filling it in would never end",
    )
}

/// Gives `out` every name a function uses as a type, with where it is: in its
/// signature, in its bindings, and in what its body calls, which may be a
/// struct.
fn types_named(f: &Function, out: &mut impl FnMut(&str, Pos)) {
    let mut types: Vec<&Type> = f.params.iter().map(|p| &p.ty).collect();
    types.extend(&f.result);
    for stmt in statements(&f.body) {
        if let Stmt::Define { ty: Some(t), .. } | Stmt::Destructure { ty: Some(t), .. } = stmt {
            types.push(t);
        }
    }
    for t in types {
        type_names_in(t, out);
    }
    bounds_named(&f.generics, out);
    needs_named(&f.needs, out);
}

/// A function with a body: one written on its own, the function of an
/// implementation that implements the trait's function at a place, or the
/// one function of an implementation a type is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Template {
    Free(usize),
    Impl(usize, usize),
    /// The implementation of a trait a type is given automatically, by the
    /// trait's place, and the place of the function of the trait it is. Its
    /// copy's one type is the type.
    Given(usize, usize),
}

/// A copy of a function, for a choice of its type parameters.
// Equality is the derived one, with a count of its calls, so it agrees with `Hash`.
#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Clone, Debug, Eq, Hash)]
struct Instance {
    template: Template,
    args: Vec<Ty>,
}

impl PartialEq for Instance {
    fn eq(&self, other: &Instance) -> bool {
        crate::work::instance();
        self.template == other.template && self.args == other.args
    }
}

/// A function with a body, as its body sees it: its signature is the
/// trait's, for the function of an implementation.
struct View<'a> {
    f: &'a Function,
    scope: &'a Scope,
    params: Vec<(String, Ty)>,
    gives: Ty,
    /// How its result type is written, if it has one.
    result: Option<Written<'a>>,
    /// The effects it lists.
    effects: &'a [(String, Pos)],
}

/// What a checked call does, once every type in it is known.
enum Callee {
    /// `__eprint`, which the prelude calls to write a line to standard error.
    Eprint,
    /// `__end`, which the prelude calls to end the program with a status.
    End,
    Construct(usize, Vec<Ty>),
    /// A case of an enum written in full, by its place among the enum's
    /// cases.
    Case(usize),
    /// A copy of a function to run, and what it gives.
    Run(Instance, Ty),
    /// The function a name holds, which a parameter or a lambda's parameter
    /// was given.
    Value,
    /// A lambda that a field or a call gives, which holds the values of the
    /// names it read: the struct that holds them, with its type arguments.
    Env(usize, Vec<Ty>),
    /// A function the compiler builds, which only the prelude calls.
    Intrinsic,
    /// `__system`, `__reserve`, or `__sized`, which the library's `std.net` and
    /// `std.time` call to ask the kernel and to make the list it fills.
    Kernel,
    /// `__frame`, `__poll`, `__result`, or `__wait`, which the library's
    /// scheduler is written with.
    Frames(frames::Kind),
    /// `push`, which gives a list with one more element.
    Push,
    /// `freeze`, which gives the memory of a list as a slice.
    Freeze,
    /// `utf8`, which gives a text of the bytes, or where they stop being valid.
    Utf8,
    /// `boundary`, a fact that is proven and never computed, which calls
    /// nothing.
    Boundary,
    /// `narrow` or one of its three relatives, which take a whole number to
    /// a type that does not hold all of its values.
    Narrow(Narrowing),
    /// `float`, which gives the float of the value of a whole number.
    Float,
    /// The name of an effect, called with a lambda for each operation and the
    /// body to run with them in force: the effect, by its place among the
    /// file's.
    Handle(usize),
    /// `alloc`, called with the body to run, which gives the body a fresh
    /// arena that is freed whole when it ends.
    Arena,
}

/// For each argument of a call, the place of its parameter, or none when an
/// argument is for no parameter: a name that is not one, or more arguments by
/// position than there are parameters. `by_position` is how many of the
/// parameters are given by position, which come before the `;`. Such a call is reported where its
/// arguments are checked, so a call that was not reported gives some.
fn params_of(params: &[(String, Ty)], by_position: usize, args: &[Arg]) -> Option<Vec<usize>> {
    let mut next = 0;
    (args.iter())
        .map(|arg| match &arg.name {
            Some((name, _)) => params.iter().position(|(p, _)| p == name),
            // A lambda after the call is for the last parameter given by position.
            None if arg.after => by_position.checked_sub(1),
            None => {
                next += 1;
                (next <= params.len()).then(|| next - 1)
            }
        })
        .collect()
}

impl<'a> Defs<'a> {
    /// The defaults a call leaves out, each with the place of its parameter
    /// or field, in the order of the parameters or fields. A call that
    /// names nothing there is leaves out nothing.
    fn left_out(&self, call: &Call) -> Vec<(usize, &'a Expr)> {
        let file = self.file;
        let position = |f: &Function| {
            (f.params.iter())
                .position(|p| p.by_name)
                .unwrap_or(f.params.len())
        };
        let params = |f: &'a Function| -> Vec<(&'a str, Option<&'a Expr>)> {
            (f.params.iter())
                .map(|p| (p.name.as_str(), p.default.as_ref()))
                .collect()
        };
        let name = call.callee.as_str();
        // The parameters, and how many are given by position when they are a
        // function's, which the fields of a struct are not.
        let (slots, by_position) = match &call.qualifier {
            Some((q, at)) => {
                let Some(k) = self.trait_at(*at, q) else {
                    return Vec::new();
                };
                match traits::trait_function(&file.traits[k], name) {
                    Some(fi) => {
                        let f = &file.traits[k].functions[fi];
                        (params(f), Some(position(f)))
                    }
                    None => return Vec::new(),
                }
            }
            None => {
                if let Some(s) = self.struct_at(call.name_pos, name) {
                    let fields = (file.structs[s].fields.iter())
                        .map(|f| (f.name.as_str(), f.default.as_ref()))
                        .collect();
                    (fields, None)
                } else if is_stream_call(call) {
                    return Vec::new();
                } else if let Some(f) = self.function(call.name_pos, name) {
                    (
                        params(&file.functions[f]),
                        Some(position(&file.functions[f])),
                    )
                } else if let Some([(k, fi)]) =
                    self.trait_fns_at(call.name_pos, name).map(Vec::as_slice)
                {
                    let f = &file.traits[*k].functions[*fi];
                    (params(f), Some(position(f)))
                } else {
                    return Vec::new();
                }
            }
        };
        let mut given = vec![false; slots.len()];
        let mut next = 0;
        for arg in &call.args {
            match &arg.name {
                None if arg.after && by_position.is_some() => {
                    // A lambda after the call is for the last parameter given by position.
                    let at = by_position.and_then(|k| k.checked_sub(1));
                    if let Some(g) = at.and_then(|i| given.get_mut(i)) {
                        *g = true;
                    }
                }
                Some((n, _)) => {
                    if let Some(i) = slots.iter().position(|(s, _)| s == n) {
                        given[i] = true;
                    }
                }
                None if by_position.is_some() => {
                    if let Some(g) = given.get_mut(next) {
                        *g = true;
                    }
                    next += 1;
                }
                None => {}
            }
        }
        (slots.into_iter().zip(given).enumerate())
            .filter_map(|(i, ((_, default), given))| default.filter(|_| !given).map(|d| (i, d)))
            .collect()
    }

    /// The calls in the defaults a call leaves out, which run where the
    /// call is written, each followed by the calls in the defaults it
    /// leaves out in turn. A default already in `stack` is not filled in
    /// again, and the place of the call that leaves it out goes in
    /// `cycles`.
    fn defaults_called(
        &self,
        call: &Call,
        stack: &mut Vec<&'a Expr>,
        out: &mut Vec<&'a Call>,
        cycles: &mut Vec<Pos>,
    ) {
        for (_, default) in self.left_out(call) {
            if stack.iter().any(|d| std::ptr::eq(*d, default)) {
                cycles.push(call.pos);
                continue;
            }
            stack.push(default);
            for inner in calls([default]) {
                out.push(inner);
                self.defaults_called(inner, stack, out, cycles);
            }
            stack.pop();
        }
    }

    /// The parameters of a function with a body, among its type
    /// parameters, which those of a copy of it replace.
    /// How many of the parameters of a function with a body are given by
    /// position.
    fn by_position(&self, t: Template) -> usize {
        match t {
            Template::Given(k, fi) => self.trait_sigs[k][fi].by_position(),
            Template::Free(f) => self.signatures[f].by_position(),
            Template::Impl(i, fi) => {
                let info = self.impls[i]
                    .as_ref()
                    .expect("an implementation that is known");
                self.trait_sigs[info.of][fi].by_position()
            }
        }
    }

    fn params(&self, t: Template) -> Vec<(String, Ty)> {
        match t {
            Template::Given(k, fi) => self.trait_sigs[k][fi].params.clone(),
            t => self.view(t).params,
        }
    }

    fn view(&self, t: Template) -> View<'_> {
        let file = self.file;
        match t {
            Template::Given(..) => unreachable!("a given implementation has no body"),
            Template::Free(f) => {
                let sig = &self.signatures[f];
                let def = &file.functions[f];
                View {
                    f: def,
                    scope: &sig.scope,
                    params: sig.params.clone(),
                    gives: sig.gives.clone(),
                    result: def.result.as_ref().map(Written::Source),
                    effects: &def.effects,
                }
            }
            Template::Impl(i, fi) => {
                let info = self.impls[i]
                    .as_ref()
                    .expect("an implementation that is known");
                let sig = &self.trait_sigs[info.of][fi];
                let scope = &info.fn_scopes[fi];
                // The function's own type parameters stand for themselves.
                let mut env = info.args.clone();
                let n = info.scope.names.len();
                env.extend((n..scope.names.len()).map(Ty::Param));
                let gives = sig.gives.subst(&env);
                let f = &file.impls[i].functions[info.fns[fi].expect("implemented")];
                View {
                    f,
                    scope,
                    // The names the implementation wrote, so that a renamed
                    // parameter is reported once, where it is renamed.
                    params: (f.params.iter().enumerate())
                        .map(|(j, p)| {
                            let ty = sig
                                .params
                                .get(j)
                                .map_or(Ty::Unknown, |(_, t)| t.subst(&env));
                            (p.name.clone(), ty)
                        })
                        .collect(),
                    result: (gives != Ty::Nothing).then_some(Written::Rendered),
                    gives,
                    effects: &file.traits[info.of].functions[fi].effects,
                }
            }
        }
    }
}

/// The address of a node of the syntax tree, by which its type is recorded.
/// Whether a call is of a function the compiler builds, whose name starts
/// with two underscores and which only the prelude calls.
/// Whether a call is of a function the compiler builds for the library's
/// modules that reach the kernel: `__system`, which asks it for a call,
/// `__reserve`, which makes a list that has room and no elements, `__sized`,
/// which says how many of the elements the kernel filled, `__address_of`, which
/// gives the address of a list's bytes as a number, and `__consume`, which
/// takes a value that cannot be dropped and has nothing to give.
fn is_kernel(call: &Call) -> bool {
    call.qualifier.is_none()
        && matches!(
            call.callee.as_str(),
            "__system" | "__reserve" | "__sized" | "__consume" | "__address_of"
        )
        && of_std(call.pos)
}

fn is_intrinsic(call: &Call) -> bool {
    call.qualifier.is_none()
        && call.callee.starts_with("__")
        && frames::kind(call).is_none()
        && !lists_and_text::is_library_call(call)
        && (of_prelude(call.pos) || of_std(call.pos))
}

/// Whether a call is of `__eprint` or `__end`, which the prelude calls for
/// `main` and the library calls for a server that cannot listen.
fn is_ending_call(call: &Call, name: &str) -> bool {
    call.qualifier.is_none() && call.callee == name && (of_prelude(call.pos) || of_std(call.pos))
}

/// Whether a call is of `__eprint` or `__end`.
fn is_stream_call(call: &Call) -> bool {
    is_ending_call(call, "__eprint") || is_ending_call(call, "__end")
}

fn addr<T>(node: &T) -> usize {
    node as *const T as usize
}

/// The definitions, with the type of each expression checked, among the
/// type parameters of the definition it is written in.
struct Typed<'a> {
    defs: &'a Defs<'a>,
    types: HashMap<usize, Ty>,
    /// Where the values that may hold a child task are dropped.
    drops: ownership::Drops,
    /// The expressions that give a range where a wider one is needed, by
    /// their addresses, with the wider range each is taken as.
    widened: HashMap<usize, Int>,
    /// The calls of a name that holds a function, by their addresses.
    value_calls: Set<usize>,
    /// The names of functions passed as values, by their addresses, with
    /// the function each names.
    fn_values: Map<usize, usize>,
    /// Which of those were given for a type parameter that a trait bounds,
    /// and so are the type whose one value the function is.
    named: Set<usize>,
    /// The calls of a name that holds a function by its type, by their
    /// addresses, with the function each calls.
    named_calls: Map<usize, usize>,
    /// The proofs written as arguments and the defaults of proof parameters,
    /// by their addresses, which have no value.
    proof_names: Set<usize>,
    /// For a call of a field that holds a function, the type of the name the
    /// field is read from, which says the function once the type parameters
    /// are known.
    field_receivers: Map<usize, Ty>,
    /// The types a call of a trait's function writes in brackets after the
    /// trait, by the address of the call.
    type_args: Map<usize, Vec<Ty>>,
    /// The `mut`s in scope while a core term is made, with their types.
    core_vars: RefCell<Vec<(String, core::Type)>>,
    /// For each function with a handler of an operation that has type
    /// parameters of its own, by the address of the function, how many type
    /// parameters its lambdas are written among: its own, and then those.
    extended: Map<usize, usize>,
    /// Where the type parameters of the operations that a lambda of such a
    /// handler is checked against begin, by the address of the lambda.
    lambda_base: Map<usize, usize>,
}

impl<'a> Typed<'a> {
    /// The type of a checked expression, with the type parameters of the
    /// definition it is written in replaced by `env`.
    fn type_of(&self, e: &Expr, env: &[Ty]) -> Ty {
        self.types[&addr(e)].subst(env)
    }

    /// The function that `left + right`, `left - right`, or `left * right`
    /// runs when the two sides are not both whole numbers, or both floats, in
    /// the copy of the definition with the type parameters `env`, and the type
    /// it gives. Nothing when the operation is one instruction.
    fn operator(
        &self,
        expr: &Expr,
        op: ArithOp,
        sides: (&Expr, &Expr),
        env: &[Ty],
    ) -> Option<(Instance, Ty)> {
        let defs = self.defs;
        let gives = self.type_of(expr, env);
        if matches!(gives, Ty::Int(_) | Ty::Float(_) | Ty::Flex(_)) {
            return None;
        }
        let (k, name) = match op {
            ArithOp::Add => (defs.operators[0], "add"),
            ArithOp::Sub => (defs.operators[1], "sub"),
            ArithOp::Mul => (defs.operators[2], "mul"),
            _ => return None,
        };
        let fi = traits::trait_function(&defs.file.traits[k], name)?;
        let types = vec![
            self.type_of(sides.0, env),
            self.type_of(sides.1, env),
            gives.clone(),
        ];
        let (instance, all) = defs.chosen(k, fi, types, Vec::new());
        Some((instance, defs.trait_sigs[k][fi].gives.subst(&all)))
    }

    /// What a checked call does, with the type parameters of the definition
    /// it is written in replaced by `env`.
    fn callee(&self, call: &Call, env: &[Ty]) -> Callee {
        let defs = self.defs;
        let given = |i: usize| {
            let arg = &call.args[i].value;
            match self.widened.get(&addr(arg)) {
                Some(wide) => Ty::Int(*wide),
                None if self.named.contains(&addr(arg)) => Ty::Named(self.fn_values[&addr(arg)]),
                None => self.type_of(arg, env),
            }
        };
        let solve = |params: &[(String, Ty)], by_position: usize, vars: usize| {
            let mut sol = vec![None; vars];
            let places = params_of(params, by_position, &call.args).expect("checked");
            for (i, p) in places.into_iter().enumerate() {
                unify(&params[p].1, &as_read(&given(i), &params[p].1), &mut sol);
            }
            sol.into_iter()
                .map(|t| t.unwrap_or(Ty::Unknown))
                .collect::<Vec<Ty>>()
        };
        // A type that nothing settled is its default, if it has one.
        let defaulted = |mut sol: Vec<Ty>, defaults: Vec<Option<Ty>>| {
            for (i, d) in defaults.iter().enumerate() {
                if let (Ty::Unknown, Some(d)) = (&sol[i], d) {
                    sol[i] = d.subst(&sol);
                }
            }
            sol
        };
        if self.value_calls.contains(&addr(call)) {
            return Callee::Value;
        }
        if numbers_and_operators::bits_function(defs, call).is_some() {
            return Callee::Intrinsic;
        }
        let run = |f: usize| {
            let sig = &defs.signatures[f];
            let args = solve(&sig.params, sig.by_position(), sig.scope.names.len());
            let args = defaulted(args, sig.param_defaults.clone());
            // What only a requirement names is what its implementation says.
            let args = if sig.scope.requires.is_empty() {
                args
            } else {
                let mut some: Vec<Option<Ty>> = args
                    .into_iter()
                    .map(|t| (t != Ty::Unknown).then_some(t))
                    .collect();
                defs.complete_outs(&sig.scope.requires, &mut some, &Scope::default());
                some.into_iter().map(|t| t.unwrap_or(Ty::Unknown)).collect()
            };
            let gives = sig.gives.subst(&args);
            let template = Template::Free(f);
            Callee::Run(Instance { template, args }, gives)
        };
        if let Some(receiver) = self.field_receivers.get(&addr(call))
            && let Ty::Struct(s, args) = receiver.subst(env)
            && let Some(i) = defs.file.structs[s]
                .fields
                .iter()
                .position(|f| f.name == call.callee)
        {
            match defs.fields[s][i].subst(&args) {
                Ty::Named(g) => return run(g),
                Ty::Struct(e, held) if defs.env_lambda(e).is_some() => {
                    return Callee::Env(e, held);
                }
                _ => {}
            }
        }
        if let Some(inner) = call.through.as_deref()
            && !matches!(inner, Expr::Field { .. })
            && let Ty::Struct(e, held) = self.type_of(inner, env)
            && defs.env_lambda(e).is_some()
        {
            return Callee::Env(e, held);
        }
        if let Some(&f) = self.named_calls.get(&addr(call))
            && (call.qualifier.is_some() || call.through.is_some())
        {
            return run(f);
        }
        let (k, fi) = match &call.qualifier {
            None => {
                let name = call.callee.as_str();
                if defs.struct_at(call.name_pos, name).is_some() {
                    let Ty::Struct(s, args) = self.types[&addr(call)].subst(env) else {
                        unreachable!("checked")
                    };
                    return Callee::Construct(s, args);
                }
                if name == "__eprint" {
                    return Callee::Eprint;
                }
                if name == "__end" {
                    return Callee::End;
                }
                if is_kernel(call) {
                    return Callee::Kernel;
                }
                if let Some(kind) = frames::kind(call) {
                    return Callee::Frames(kind);
                }
                if is_intrinsic(call) {
                    return Callee::Intrinsic;
                }
                if is_push(defs, call) {
                    return Callee::Push;
                }
                if is_freeze(defs, call) {
                    return Callee::Freeze;
                }
                if is_utf8(defs, call) {
                    return Callee::Utf8;
                }
                if lists_and_text::is_boundary(defs, call) {
                    return Callee::Boundary;
                }
                if let Some(narrowing) = narrowing(defs, call) {
                    return Callee::Narrow(narrowing);
                }
                if is_float_call(defs, call) {
                    return Callee::Float;
                }
                let by_type = self.named_calls.get(&addr(call)).copied();
                if let Some(f) = by_type.or_else(|| defs.function(call.name_pos, name)) {
                    return run(f);
                }
                if let Some(effect) = defs.effect_named(name) {
                    return Callee::Handle(effect);
                }
                if is_arena_call(defs, call) {
                    return Callee::Arena;
                }
                defs.trait_fns_at(call.name_pos, name).expect("checked")[0]
            }
            Some((q, at)) => {
                if let Some(e) = defs.enum_at(*at, q) {
                    return Callee::Case(defs.case_of(e, &call.callee).expect("checked"));
                }
                let k = defs.trait_at(*at, q).expect("checked");
                let at = traits::trait_function(&defs.file.traits[k], &call.callee);
                (k, at.expect("checked"))
            }
        };
        let sig = &defs.trait_sigs[k][fi];
        let mut types = solve(&sig.params, sig.by_position(), sig.scope.names.len());
        for (i, j) in defs.defaulted(k) {
            if types[i] == Ty::Unknown {
                types[i] = types[j].clone();
            }
        }
        // The types the call writes after its trait are those of the trait.
        if let Some(written) = self.type_args.get(&addr(call)) {
            for (t, w) in types.iter_mut().zip(written) {
                if *t == Ty::Unknown {
                    *t = w.subst(env);
                }
            }
        }
        let mut types = self.result_types(call, sig, env, types);
        // The function's own type parameters follow the trait's.
        let own = types.split_off(defs.file.traits[k].generics.len());
        let (instance, mut all) = defs.chosen(k, fi, types, own.clone());
        all.extend(own);
        Callee::Run(instance, sig.gives.subst(&all))
    }

    /// The copy of the `each` that a `loop` over a value runs, where `env`
    /// stands for the type parameters of the definition it is written in.
    fn each_of(&self, o: &Over, env: &[Ty]) -> Instance {
        let walked = as_read(&self.type_of(&o.value, env), &Ty::List(vec![Ty::Unknown]));
        let defs = self.defs;
        defs.chosen(defs.iterate, 0, vec![walked, Ty::Unknown], Vec::new())
            .0
    }

    /// The types that stand for the type parameters of the defaults a call
    /// leaves out: those of the function, of the trait, or of the struct
    /// they are written in.
    fn env_of(&self, callee: &Callee) -> Vec<Ty> {
        match callee {
            Callee::Run(instance, _) => match instance.template {
                Template::Free(_) | Template::Given(..) => instance.args.clone(),
                Template::Impl(i, _) => {
                    let info = self.defs.impls[i].as_ref().expect("known");
                    info.args.iter().map(|t| t.subst(&instance.args)).collect()
                }
            },
            Callee::Construct(_, args) => args.clone(),
            Callee::Eprint
            | Callee::End
            | Callee::Case(_)
            | Callee::Value
            | Callee::Env(..)
            | Callee::Intrinsic
            | Callee::Kernel
            | Callee::Frames(_)
            | Callee::Push
            | Callee::Freeze
            | Callee::Utf8
            | Callee::Boundary
            | Callee::Handle(_)
            | Callee::Arena
            | Callee::Float
            | Callee::Narrow(_) => Vec::new(),
        }
    }

    /// The calls in the defaults a call leaves out, which run where the
    /// call is written, each with the types that stand for the type
    /// parameters of the definition the default is in, and each followed
    /// by the calls in the defaults it leaves out in turn. A default
    /// already in `stack` is not filled in again, and the place of the call
    /// that leaves it out goes in `cycles`.
    fn filled(
        &self,
        call: &'a Call,
        env: &[Ty],
        stack: &mut Vec<&'a Expr>,
        out: &mut Vec<(&'a Call, Vec<Ty>)>,
        cycles: &mut Vec<Pos>,
    ) {
        let left_out = self.defs.left_out(call);
        if left_out.is_empty() || self.value_calls.contains(&addr(call)) {
            return;
        }
        let inner_env = self.env_of(&self.callee(call, env));
        for (_, default) in left_out {
            if stack.iter().any(|d| std::ptr::eq(*d, default)) {
                cycles.push(call.pos);
                continue;
            }
            stack.push(default);
            for inner in calls([default]) {
                out.push((inner, inner_env.clone()));
                self.filled(inner, &inner_env, stack, out, cycles);
            }
            stack.pop();
        }
    }

    /// The calls in a copy's body that run a function, outermost first and
    /// then from left to right, each with where it is and the copy it runs.
    /// The calls in a default a call leaves out follow that call. Each call
    /// that leaves out the default it is written in is in the second list.
    /// A given implementation has no body, so its calls are at `at`, where
    /// it is run.
    fn calls_of(&self, instance: &Instance, at: Pos) -> (Vec<(Pos, Instance)>, Vec<Pos>) {
        if let Template::Given(k, fi) = instance.template {
            let calls = self.defs.given_calls(k, fi, &instance.args);
            return (calls.into_iter().map(|c| (at, c)).collect(), Vec::new());
        }
        let view = self.defs.view(instance.template);
        let mut out = Vec::new();
        let mut cycles = Vec::new();
        // The lambdas of a handler of an operation with type parameters of its
        // own are written among more parameters than the function has, which
        // are here the parameters themselves, so that what a lambda calls is
        // told by what it is written for and not by what it is run for.
        let mut args = instance.args.clone();
        if let Some(&total) = self.extended.get(&addr(view.f)) {
            while args.len() < total {
                args.push(Ty::Param(args.len()));
            }
        }
        let instance = &Instance {
            template: instance.template,
            args,
        };
        for e in exprs(values(&view.f.body)) {
            if let Some(&f) = self.fn_values.get(&addr(e)) {
                let template = Template::Free(f);
                let args = Vec::new();
                out.push((e.pos(), Instance { template, args }));
            }
            if let Expr::Loop(l) = e
                && let Some(o) = &l.over
            {
                out.push((l.pos, self.each_of(o, &instance.args)));
            }
            if let Expr::Arith {
                op,
                left,
                right,
                op_pos,
            } = e
                && let Some((callee, _)) = self.operator(e, *op, (left, right), &instance.args)
            {
                out.push((*op_pos, callee));
            }
            let Expr::Call(call) = e else {
                continue;
            };
            let mut all = vec![(call, instance.args.clone())];
            self.filled(call, &instance.args, &mut Vec::new(), &mut all, &mut cycles);
            for (call, env) in all {
                if let Callee::Run(callee, _) = self.callee(call, &env) {
                    out.push((call.pos, callee));
                }
            }
        }
        (out, cycles)
    }

    /// Reports each call that would need a copy of a function without end:
    /// one already being walked, or one of a function being walked for
    /// larger types, which would grow forever. `path` is the copies being
    /// walked from `main`, by their templates, `done` those already walked,
    /// and `at` where the copy is run.
    ///
    /// A given implementation runs only those of its parts, which are
    /// smaller, and of its own type, for another trait, so a copy that would
    /// grow forever passes through a function written in the source, where
    /// it is reported.
    fn copies(
        &self,
        instance: &Instance,
        at: Pos,
        path: &mut Map<Template, Vec<Instance>>,
        done: &mut Set<Instance>,
        errors: &mut Vec<Error>,
    ) {
        path.entry(instance.template)
            .or_default()
            .push(instance.clone());
        let (calls, cycles) = self.calls_of(instance, at);
        errors.extend(cycles.into_iter().map(default_cycle));
        for (pos, callee) in calls {
            // A proof is never built, so no copy of the function that gives one
            // is made, and what it calls is for the kernel to check.
            if let Template::Free(f) = callee.template
                && is_proof(&self.defs.file.functions[f])
            {
                continue;
            }
            let size = |i: &Instance| i.args.iter().map(Ty::size).sum::<usize>();
            let grows = |p: &Instance| {
                !matches!(callee.template, Template::Given(..)) && size(&callee) > size(p)
            };
            // A call of the copy from itself, or from a copy of a circle of
            // functions to another that is being walked, needs no other copy,
            // and `recursion` says whether it may be one.
            let itself = callee == *instance;
            let walking = (path.get(&callee.template).into_iter().flatten()).any(|p| *p == callee);
            let endless = (path.get(&callee.template).into_iter().flatten()).any(grows);
            if endless {
                errors.push(Error::unsupported(
                    pos,
                    "a function that calls itself, directly or through others,",
                ));
            } else if !itself && !walking && !done.contains(&callee) {
                self.copies(&callee, pos, path, done, errors);
            }
        }
        if let Some(walking) = path.get_mut(&instance.template) {
            walking.pop();
        }
        done.insert(instance.clone());
    }
}

/// How the result type of a function is said when it is wrong: as it is written
/// in the source, or as the type of the trait's function it implements.
enum Written<'a> {
    Source(&'a Type),
    Rendered,
}

impl Written<'_> {
    fn text(&self, view: &View, defs: &Defs) -> String {
        match self {
            Written::Source(t) => written_type(t),
            Written::Rendered => defs.text(&view.gives, view.scope),
        }
    }
}

/// What kind of definition a list of type parameters belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
}

impl<'a> Checker<'a> {
    /// A checker that has looked at nothing, over the definitions `defs`.
    fn new(defs: &'a Defs<'a>, errors: Vec<Error>) -> Self {
        Checker {
            defs,
            errors,
            probed: Map::default(),
            seen: Map::default(),
            gave: Map::default(),
            lambda_gave: Ty::Unknown,
            last_args: Vec::new(),
            free_args: false,
            lambda_from: HashMap::new(),
            literal_from: HashMap::new(),
            from_calls: None,
            using: None,
            probing: false,
            types: HashMap::new(),
            drops: ownership::Drops::default(),
            widened: HashMap::new(),
            negated: Set::default(),
            infer_on: false,
            solve: None,
            solved: infer::Solved::default(),
            rounds: Vec::new(),
            inferred: HashMap::new(),
            patterned: HashSet::new(),
            assigning: None,
            comparing: false,
            discovering: false,
            found: HashMap::new(),
            settled: false,
            value_calls: Set::default(),
            fn_values: Map::default(),
            named: Set::default(),
            named_calls: Map::default(),
            proof_names: Set::default(),
            field_receivers: Map::default(),
            type_args: Map::default(),
            frame_gives: Map::default(),
            seeded: None,
            extended: Map::default(),
            handler_slots: Map::default(),
            lambda_base: Map::default(),
            no_impl: false,
            refusals: 0,
            zero_divisors: Vec::new(),
            deciding: None,
            cross: Vec::new(),
            parts_passed: Map::default(),
            walking: Vec::new(),
            walked_calls: Vec::new(),
            kernel_calls: Vec::new(),
            invariant: false,
            fors: 0,
            unused_effects: Vec::new(),
            circles: Vec::new(),
        }
    }
}

struct Checker<'a> {
    defs: &'a Defs<'a>,
    errors: Vec<Error>,
    /// The types of the names each lambda that reads names read, as the body
    /// that was checked last gave them, by the struct that holds them.
    probed: Map<usize, Vec<Ty>>,
    /// The types of the arguments of each call of a lambda that leaves types
    /// out, in the look that was made last, by the struct that holds it, with
    /// where each call is and how it is written.
    seen: Map<usize, Vec<(Pos, String, Vec<Ty>)>>,
    /// What each such lambda gave, in the look that was made last.
    gave: Map<usize, Ty>,
    /// What the lambda checked last gave.
    lambda_gave: Ty,
    /// The types of the arguments of the call that `value_call` checked last.
    last_args: Vec<Ty>,
    /// Whether the call being checked is of a lambda whose parameters are the
    /// types of what its calls pass.
    free_args: bool,
    /// The parameters of lambdas in scope whose types are those of the calls of
    /// them, by their place among the names, with the call that gave the first.
    lambda_from: HashMap<usize, Vec<(Pos, String)>>,
    /// The names bound to a whole number that keep its own range, by their
    /// place among the names, with the name, where it is bound, and the number
    /// as it is written.
    literal_from: HashMap<usize, (String, Pos, String)>,
    /// The calls that gave the parameters of the lambda about to be checked.
    from_calls: Option<Vec<(Pos, String)>>,
    /// The function whose arguments are being checked.
    using: Option<String>,
    /// Whether this look is made to find the types of the lambdas that leave
    /// some out, and its errors are thrown away.
    probing: bool,
    /// The type of each expression checked, by its address.
    types: HashMap<usize, Ty>,
    /// Where the values that may hold a child task are dropped.
    drops: ownership::Drops,
    /// The expressions that give a range where a wider one is needed, by
    /// their addresses, with the wider range each is taken as.
    widened: HashMap<usize, Int>,
    /// The `==` calls that a `!=` negates, by their addresses, which the
    /// `!=` reports and not the call.
    negated: Set<usize>,
    /// Whether the body being checked has a `mut` with no type written,
    /// whose range its values give it.
    infer_on: bool,
    /// What is learned while the ranges of those `mut`s are solved.
    solve: Option<infer::Solve>,
    /// What the solving found, which the look that is kept reads.
    solved: infer::Solved,
    /// How many rounds each `loop` around what is being checked has at most,
    /// from the outermost, where that is known when the program is built.
    rounds: Vec<Option<u128>>,
    /// The `mut`s of the body whose range is solved, by their place among the
    /// names, with the place of the statement that defines each.
    inferred: HashMap<usize, Pos>,
    /// The names that a pattern gives a value in the body, which are not
    /// solved.
    patterned: HashSet<String>,
    /// The `mut` that is being given a value, and whether the value reads it.
    assigning: Option<(Pos, bool)>,
    /// Whether the arguments being fitted are those of a comparison, whose
    /// two sides are taken as the smallest range that holds both.
    comparing: bool,
    /// Whether a block is being looked at once to find what it gives, in which
    /// the blocks inside it are looked at once and not twice.
    discovering: bool,
    /// What a block that was looked at to find what it gives gave, by the
    /// address of its lambda, so that the block is not looked at twice again
    /// where it is checked for good.
    found: HashMap<usize, Ty>,
    /// Whether the type a part of a case is fitted to was settled by the type
    /// that was asked for, which a range that is narrower is taken to.
    settled: bool,
    /// What the lambda of each call of `__frame` gives, by the address of the
    /// call, which `__result` reads a frame at.
    frame_gives: Map<usize, Ty>,
    /// What the type parameters of the signature of the call being checked
    /// are taken to be before any argument says, which is the type parameters
    /// themselves for those a handler's lambdas are checked against.
    seeded: Option<Vec<Option<Ty>>>,
    /// For each function with a handler of an operation that has type
    /// parameters of its own, by its address, the type parameters its lambdas
    /// are written among: its own, and then those of each such handler, which
    /// each have places of their own.
    extended: Map<usize, Scope>,
    /// Where the type parameters of the operations a handler's lambdas are
    /// checked against begin, by the address of the call of the handler.
    handler_slots: Map<usize, usize>,
    /// The same for each lambda of the handler, by its address.
    lambda_base: Map<usize, usize>,
    /// The calls of a name that holds a function, by their addresses.
    value_calls: Set<usize>,
    /// The names of functions passed as values, by their addresses, with
    /// the function each names.
    fn_values: Map<usize, usize>,
    /// Which of those were given for a type parameter that a trait bounds.
    named: Set<usize>,
    /// The calls of a name that holds a function by its type, with the function.
    named_calls: Map<usize, usize>,
    /// The proofs written as arguments and the defaults of proof parameters,
    /// by their addresses, which have no value.
    proof_names: Set<usize>,
    /// For a call of a field that holds a function, the type of the name the
    /// field is read from, which says the function once the type parameters
    /// are known.
    field_receivers: Map<usize, Ty>,
    /// The types a call writes in brackets after its trait.
    type_args: Map<usize, Vec<Ty>>,
    /// Whether the call of a trait's function being checked has types no
    /// implementation is for, which is already reported.
    no_impl: bool,
    /// How many lambdas after a call have been reported as given to a
    /// function that cannot take them, so that a call that had one is not
    /// reported again for what it gives.
    refusals: usize,
    /// Where a division was refused for a divisor that may be zero, which is
    /// the one thing said of it.
    zero_divisors: Vec<Pos>,
    /// While a number literal is checked against the type the uses of its
    /// name decided, the place of the use that decided it.
    deciding: Option<Pos>,
    /// The calls of a function that has a `decreases` from another that has,
    /// where the measure of the call was not proven smaller, with the
    /// functions the call is from and to. They are the mistake of the program
    /// only when the two are in a circle of calls, which `recursion` finds.
    cross: Vec<(usize, usize, Error)>,
    /// Each call of a function from a function of the file, by its place:
    /// the function it is from, the one it calls, and the places at which it
    /// passes a part of the caller's parameter at the same place.
    parts_passed: Map<Pos, (usize, usize, Vec<usize>)>,
    /// The functions `recursion` is walking, and the places of the calls that
    /// lead from each to the next.
    walking: Vec<usize>,
    walked_calls: Vec<Pos>,
    /// The circles of functions already reported, each in order.
    circles: Vec<Vec<usize>>,
    /// The calls whose `requires` the kernel works out when the program is
    /// checked whole.
    kernel_calls: Vec<proof::KernelCall>,
    /// Whether an `invariant` is being checked, which is never computed, so
    /// that a fact such as `boundary` may be written in it.
    invariant: bool,
    /// How many `for`s the expression being checked is in, whose blocks may
    /// need no effect but `alloc`.
    fors: usize,
    /// The effects a function lists that nothing in its body needs: the place
    /// of the function, its name, the effect, where it is listed, and the
    /// signature without the effects nothing in it needs.
    unused_effects: Vec<(Pos, String, String, Pos, String, bool)>,
}

/// What is expected of an expression's value.
#[derive(Clone, Debug)]
enum Want {
    /// It is a statement, and its value is not used.
    Unused,
    /// Its value is used, and nothing says what type it must be.
    Any,
    /// Its value must be of this type, which may be partly known.
    Ty(Ty),
}

/// How a block ends.
enum End<'e> {
    Empty,
    /// With a binding or an assignment, at this place.
    Binding(Pos),
    /// With an expression, which gives this type.
    Value(Ty, &'e Expr),
}

/// A pattern once checked: `Any` matches every value, as a bare name does,
/// `Case` matches one case of an enum, by its place, and what it holds if a
/// pattern for it is given, and `Tuple` matches a tuple whose elements its
/// patterns match.
///
/// `Int` matches the whole number equal to it, and `Text` the text equal to
/// it, and `List` a list of as many elements as it has patterns, whose
/// elements they match.
enum Pat {
    Any,
    Case(usize, Option<Box<Pat>>),
    Tuple(Vec<Pat>),
    List(Vec<Pat>),
    Int(i128),
    Text(Vec<u8>),
}

const ANY: Pat = Pat::Any;

/// Rows of patterns, one column for each value matched at once.
type Rows<'p> = Vec<Vec<&'p Pat>>;

/// The rows whose first pattern matches case `k`, with that pattern
/// replaced by the one for what the case holds, if it `holds` something. A
/// pattern that matches anything matches what it holds with anything too.
fn specialize<'p>(rows: &[Vec<&'p Pat>], k: usize, holds: bool) -> Rows<'p> {
    (rows.iter())
        .filter_map(|r| {
            let inner = match r[0] {
                Pat::Any => &ANY,
                Pat::Case(j, inner) if *j == k => inner.as_deref().unwrap_or(&ANY),
                Pat::Case(..) | Pat::Tuple(_) | Pat::List(_) | Pat::Int(_) | Pat::Text(_) => {
                    return None;
                }
            };
            let mut row = if holds { vec![inner] } else { Vec::new() };
            row.extend(&r[1..]);
            Some(row)
        })
        .collect()
}

/// The rows with their first pattern, which matches a tuple of `n`,
/// replaced by a pattern for each of its elements.
fn spread<'p>(rows: &[Vec<&'p Pat>], n: usize) -> Rows<'p> {
    (rows.iter())
        .map(|r| {
            let mut row: Vec<&Pat> = match r[0] {
                Pat::Tuple(items) => items.iter().collect(),
                _ => vec![&ANY; n],
            };
            row.extend(&r[1..]);
            row
        })
        .collect()
}

/// The rows whose first pattern matches a list of `n` elements, with that
/// pattern replaced by a pattern for each element. A pattern that matches
/// anything matches each element with anything too.
fn elements<'p>(rows: &[Vec<&'p Pat>], n: usize) -> Rows<'p> {
    (rows.iter())
        .filter_map(|r| {
            let mut row: Vec<&Pat> = match r[0] {
                Pat::Any => vec![&ANY; n],
                Pat::List(items) if items.len() == n => items.iter().collect(),
                _ => return None,
            };
            row.extend(&r[1..]);
            Some(row)
        })
        .collect()
}

/// Whether some row's first pattern names a case.
fn names_a_case(rows: &[Vec<&Pat>]) -> bool {
    rows.iter().any(|r| matches!(r[0], Pat::Case(..)))
}

/// The values of types `tys` that none of the rows matches, as the
/// patterns that match them are written, one for each type, in the order
/// the enums declare their cases. A value no row matches any part of is a
/// bare name, and a case none of the rows names is one pattern, with a bare
/// name for what it holds.
fn missing_rows(defs: &Defs, rows: &[Vec<&Pat>], tys: &[Ty]) -> Vec<Vec<String>> {
    let Some((ty, rest)) = tys.split_first() else {
        return if rows.is_empty() {
            vec![Vec::new()]
        } else {
            Vec::new()
        };
    };
    let any = |n: usize| vec!["_".to_string(); n];
    match ty {
        Ty::Tuple(_) if rows.is_empty() => vec![any(tys.len())],
        Ty::Tuple(items) => {
            let tys: Vec<Ty> = items.iter().chain(rest).cloned().collect();
            (missing_rows(defs, &spread(rows, items.len()), &tys).into_iter())
                .map(|left| {
                    let (first, after) = left.split_at(items.len());
                    let mut out = vec![format!("({})", first.join(", "))];
                    out.extend_from_slice(after);
                    out
                })
                .collect()
        }
        Ty::Enum(e, args) if rows.is_empty() || names_a_case(rows) => {
            let mut out = Vec::new();
            for (k, case) in defs.file.enums[*e].cases.iter().enumerate() {
                let holds = defs.cases[*e][k].as_ref().map(|t| t.subst(args));
                let tys: Vec<Ty> = holds.iter().chain(rest).cloned().collect();
                let inside = specialize(rows, k, holds.is_some());
                let left = if inside.is_empty() {
                    vec![any(tys.len())]
                } else {
                    missing_rows(defs, &inside, &tys)
                };
                for left in left {
                    let (first, after) = match holds {
                        Some(_) => (format!(".{}({})", case.name, left[0]), &left[1..]),
                        // The cases of `bool` are written as bare names.
                        None if *e == defs.boolean => (case.name.clone(), &left[..]),
                        None => (format!(".{}", case.name), &left[..]),
                    };
                    let mut row = vec![first];
                    row.extend_from_slice(after);
                    out.push(row);
                }
            }
            out
        }
        // No list of literals covers every number or every text.
        Ty::Int(_) | Ty::Flex(_) | Ty::Text | Ty::List(_) => {
            let rows: Rows = (rows.iter())
                .filter(|r| matches!(r[0], Pat::Any))
                .map(|r| r[1..].to_vec())
                .collect();
            (missing_rows(defs, &rows, rest).into_iter())
                .map(|left| {
                    let mut row = vec!["_".to_string()];
                    row.extend(left);
                    row
                })
                .collect()
        }
        _ => {
            let rows: Rows = rows.iter().map(|r| r[1..].to_vec()).collect();
            (missing_rows(defs, &rows, rest).into_iter())
                .map(|left| {
                    let mut row = vec!["_".to_string()];
                    row.extend(left);
                    row
                })
                .collect()
        }
    }
}

/// The values of type `ty` that none of the patterns matches, as the
/// patterns that match them are written.
fn missing(defs: &Defs, patterns: &[&Pat], ty: &Ty) -> Vec<String> {
    let rows: Rows = patterns.iter().map(|p| vec![*p]).collect();
    (missing_rows(defs, &rows, std::slice::from_ref(ty)).into_iter())
        .map(|mut left| left.remove(0))
        .collect()
}

/// Whether `row` matches some values of types `tys` that none of `rows`
/// matches.
fn useful_row(defs: &Defs, rows: &[Vec<&Pat>], row: &[&Pat], tys: &[Ty]) -> bool {
    let Some((ty, rest)) = tys.split_first() else {
        return rows.is_empty();
    };
    match (row[0], ty) {
        (_, Ty::Tuple(items)) => {
            let tys: Vec<Ty> = items.iter().chain(rest).cloned().collect();
            let rows = spread(rows, items.len());
            let row = spread(&[row.to_vec()], items.len()).remove(0);
            useful_row(defs, &rows, &row, &tys)
        }
        (Pat::Case(k, _), Ty::Enum(e, args)) => {
            let holds = defs.cases[*e][*k].as_ref().map(|t| t.subst(args));
            let tys: Vec<Ty> = holds.iter().chain(rest).cloned().collect();
            let rows = specialize(rows, *k, holds.is_some());
            let row = specialize(&[row.to_vec()], *k, holds.is_some()).remove(0);
            useful_row(defs, &rows, &row, &tys)
        }
        (Pat::Any, Ty::Enum(e, args)) if names_a_case(rows) => (0..defs.file.enums[*e].cases.len())
            .any(|k| {
                let holds = defs.cases[*e][k].as_ref().map(|t| t.subst(args));
                let tys: Vec<Ty> = holds.iter().chain(rest).cloned().collect();
                let rows = specialize(rows, k, holds.is_some());
                let row = specialize(&[row.to_vec()], k, holds.is_some()).remove(0);
                useful_row(defs, &rows, &row, &tys)
            }),
        (Pat::List(items), Ty::List(of)) => {
            let n = items.len();
            let tys: Vec<Ty> = (0..n)
                .map(|_| of[0].clone())
                .chain(rest.iter().cloned())
                .collect();
            let rows = elements(rows, n);
            let row = elements(&[row.to_vec()], n).remove(0);
            useful_row(defs, &rows, &row, &tys)
        }
        (_, Ty::Int(_) | Ty::Flex(_) | Ty::Text | Ty::List(_)) => {
            // A literal is matched again by the same literal and by a bare
            // name, and a bare name only by a bare name.
            let same = |p: &Pat| match (row[0], p) {
                (_, Pat::Any) => true,
                (Pat::Int(a), Pat::Int(b)) => a == b,
                (Pat::Text(a), Pat::Text(b)) => a == b,
                _ => false,
            };
            let rows: Rows = (rows.iter())
                .filter(|r| same(r[0]))
                .map(|r| r[1..].to_vec())
                .collect();
            useful_row(defs, &rows, &row[1..], rest)
        }
        _ => {
            let rows: Rows = rows.iter().map(|r| r[1..].to_vec()).collect();
            useful_row(defs, &rows, &row[1..], rest)
        }
    }
}

/// Whether the pattern matches some value of type `ty` that none of the
/// patterns before it matches.
fn useful(defs: &Defs, before: &[&Pat], pattern: &Pat, ty: &Ty) -> bool {
    let rows: Rows = before.iter().map(|p| vec![*p]).collect();
    useful_row(defs, &rows, &[pattern], std::slice::from_ref(ty))
}

/// What checking one body needs to keep.
struct Body<'a> {
    /// The function whose body this is, or `None` for a default.
    f: Option<&'a Function>,
    /// For a default, the names of the parameters of the function it is
    /// in, which it may not use.
    default: Option<Vec<&'a str>>,
    scope: &'a Scope,
    /// The effects it lists that have a name, each once.
    listed: Vec<&'a str>,
    /// The effects a lambda being checked may carry beside those listed: those
    /// of the function type of the parameter it is written for.
    extra: Vec<String>,
    /// Whether it lists an effect with no such name, which may be one it
    /// needs misspelt, so that a missing effect is then not reported.
    unknown_effect: bool,
    /// The effects already reported missing, each reported once.
    missing: Vec<String>,
    /// The places in the errors of the fixes that write the effects the
    /// function lacks, which say the whole list once the body is checked.
    missing_at: Vec<usize>,
    /// The listed effects something in the body needed.
    used: Vec<String>,
    /// The names defined so far, and what each gives.
    names: Vec<(String, Ty)>,
    /// How many of the names are parameters, which come first.
    params: usize,
    /// The names defined with `mut`.
    vars: Vec<String>,
    /// What looking at the uses of the whole numbers bound to names has
    /// found, or is finding.
    numbers: Numbers,
    /// The names of numbers with no type written that the uses decided, with
    /// the type and the place of the use that decided it.
    decided: HashMap<String, (Int, Option<Pos>)>,
    /// What `arith` knows where the body is being checked.
    facts: proof::Facts,
    /// The places of the expressions the body can end with, which each have
    /// to meet what the function `ensures`.
    tails: Vec<usize>,
    /// The type of the result of the function.
    gives: Ty,
    /// Whether the body is a contract, which states what is so of whole
    /// numbers and never computes, so that nothing in it can overflow.
    contract: bool,
    /// How many `loop`s the code being checked is inside, in the function or
    /// lambda it is written in.
    loops: usize,
    /// How many `loop`s the lambdas around the code being checked are inside,
    /// which a `break` cannot reach.
    outer_loops: usize,
    /// The type a `return` in the code being checked gives: the result of
    /// the function or the lambda it is written in, which a lambda whose
    /// result is not yet known leaves unknown.
    returns: Ty,
    /// The type of the first `return` in a lambda whose result is not yet
    /// known, which is then the result.
    returned: Option<Ty>,
    /// How many lambdas the code being checked is inside.
    lambdas: usize,
    /// How many of those are stored in a field or returned, which carry no
    /// effect, as the type that holds one states none.
    kept: usize,
    /// The proofs in scope, each by its name, with the statement it proves in
    /// terms of the names around it: a parameter that is a proof, a name a
    /// `check` bound, and a name a call of a proof function was given.
    proofs: Vec<(String, Held)>,
    /// What each call of a proof function proved, by the address of the call,
    /// for the name the call is bound to or the parameter it is passed for.
    call_proofs: HashMap<usize, Held>,
    /// The names that are a part of a parameter, each by its place among the
    /// names, with the place of the parameter among those of the function.
    parts: Vec<(usize, usize)>,
}

impl<'a> Body<'a> {
    /// Leaves only the first `n` names defined, and forgets that a name
    /// left out is a part of a parameter.
    fn truncate_names(&mut self, n: usize) {
        self.names.truncate(n);
        self.parts.retain(|&(at, _)| at < n);
    }

    /// Defines the names a pattern binds, each with its type and whether it
    /// is a `mut`.
    fn define(&mut self, binds: Vec<(String, Ty, bool)>) {
        for (name, ty, var) in binds {
            if var {
                self.vars.push(name.clone());
            }
            self.names.push((name, ty));
        }
    }

    /// The function whose body this is.
    fn owner(&self) -> &'a Function {
        self.f.expect("a function's body")
    }

    /// Where the body is, as an error names it after a place.
    fn in_owner(&self) -> String {
        self.f
            .map_or(String::new(), |f| format!(" in `{}`", f.name))
    }
}

impl<'a> Checker<'a> {
    /// What only `main` must be: with no parameter, or with one, a list of
    /// the words the program was started with.
    fn main(&mut self, index: usize) {
        let f = &self.defs.file.functions[index];
        if let Some(g) = f.generics.first() {
            self.errors
                .push(Error::unsupported(g.pos, "a type parameter of `main`"));
        }
        let signature = &self.defs.signatures[index];
        let words = Ty::list(Ty::Text);
        for (i, p) in f.params.iter().enumerate() {
            let right = i == 0 && signature.params[i].1 == words && p.default.is_none();
            if !right && signature.params[i].1 != Ty::Unknown {
                self.errors.push(
                    Error::new(
                        p.pos,
                        "`main` takes no parameter, or one, the words the program was started with, as a `List[str]`",
                    )
                    .with_fix("write `fn main(args: List[str])`, or take the parameter away"),
                );
            }
        }
        if let Some(t) = &f.result {
            let gives = &signature.gives;
            let exit = self.defs.exit;
            if *gives != Ty::Unknown
                && let Found::No = self
                    .defs
                    .meets(exit, gives, &signature.scope, &mut Vec::new())
            {
                self.errors.push(
                    Error::new(
                        t.pos,
                        format!(
                            "`main` returns {}, which does not implement `Exit`, the trait that gives the status the program ends with",
                            self.defs.describe(gives, &signature.scope)
                        ),
                    )
                    .with_fix("return `()`, or a `Result` of it, or write `impl Exit` for the type"),
                );
            }
        }
    }

    /// Checks that a type written in the source is one there is, among the
    /// type parameters of `scope`, and holds no function type, and gives
    /// the type, which is unknown if it holds a mistake.
    fn ty(&mut self, t: &Type, scope: &Scope) -> Ty {
        let ty = self.defs.resolve(t, scope, &mut self.errors);
        if self.no_function(t) { ty } else { Ty::Unknown }
    }

    /// Reports the first function type in a type, which only a parameter's
    /// type may be.
    fn no_function(&mut self, t: &Type) -> bool {
        if t.function {
            self.errors.push(Error::unsupported(
                t.pos,
                "a function type other than a parameter's type",
            ));
            return false;
        }
        t.args.iter().all(|a| self.no_function(a))
    }

    /// Checks the type of a parameter, which may be a function type whose
    /// own parameters' types may be function types in turn.
    fn param_ty(&mut self, t: &Type, scope: &Scope) {
        self.defs.resolve(t, scope, &mut self.errors);
        self.passed(t);
    }

    fn passed(&mut self, t: &Type) {
        match t.args.split_last() {
            Some((gives, params)) if t.function => {
                for p in params {
                    self.passed(p);
                }
                self.no_function(gives);
            }
            _ => {
                self.no_function(t);
            }
        }
    }

    /// Checks a list of type parameters: each named once, with a name no
    /// type has, `out` only in a trait, and each bound a trait.
    fn generics(&mut self, generics: &[Generic], owner: &str, kind: Owner) {
        let defs = self.defs;
        for (i, g) in generics.iter().enumerate() {
            let name = g.name.as_str();
            if generics[..i].iter().any(|h| h.name == g.name) {
                self.errors.push(Error::new(
                    g.pos,
                    format!("`{name}` is already a type parameter of {owner}"),
                ));
            } else if !of_prelude(g.pos)
                && !of_library(g.pos)
                && (name == TEXT || defs.types.contains_key(name))
            {
                self.errors.push(Error::new(
                    g.pos,
                    format!("`{name}` is already the name of a type"),
                ));
            }
            match &g.default {
                Some(d) => {
                    let scope = defs.scope(generics, &[]);
                    for (n, at) in type_names(d) {
                        if let Some(later) = generics[i..].iter().find(|h| h.name == n) {
                            self.errors.push(Error::new(
                                at,
                                format!(
                                    "the default of `{name}` names `{}`, which does not come before it",
                                    later.name
                                ),
                            ));
                        }
                    }
                    self.ty(d, &scope);
                }
                None => {
                    if let Some(with) = generics[..i].iter().find(|h| h.default.is_some()) {
                        self.errors.push(Error::new(
                            g.pos,
                            format!(
                                "`{name}` has no default, and comes after `{}`, which has one",
                                with.name
                            ),
                        ));
                    }
                }
            }
            if let Some(at) = g.out
                && kind != Owner::Trait
            {
                self.errors
                    .push(Error::new(at, "only the types of a trait can be `out`"));
            }
            match (&g.bound, kind) {
                (Some(b), Owner::Struct) => self.errors.push(Error::unsupported(
                    b.pos,
                    "a bound on a type parameter of a struct",
                )),
                (Some(b), Owner::Enum) => self.errors.push(Error::unsupported(
                    b.pos,
                    "a bound on a type parameter of an enum",
                )),
                (Some(b), Owner::Trait) => self
                    .errors
                    .push(Error::unsupported(b.pos, "a bound on a type of a trait")),
                (bound, _) => {
                    defs.bound(bound, &mut self.errors);
                }
            }
        }
    }

    /// Checks a struct: its type parameters, each named by a field, and its
    /// fields, each named once, each of a type there is.
    fn structure(&mut self, s: usize) {
        let defs = self.defs;
        let def = &defs.file.structs[s];
        let scope = &defs.struct_scopes[s];
        self.generics(&def.generics, &format!("`{}`", def.name), Owner::Struct);
        for g in &def.generics {
            let named = (def.fields.iter().map(|f| &f.ty).chain(&def.alias))
                .any(|t| type_names(t).iter().any(|(n, _)| *n == g.name));
            if !named {
                self.errors.push(Error::unsupported(
                    g.pos,
                    "a type parameter that no field's type names",
                ));
            }
        }
        if let Some(ty) = &def.alias {
            self.ty(ty, scope);
        }
        for (i, field) in def.fields.iter().enumerate() {
            if def.fields[..i].iter().any(|f| f.name == field.name) {
                self.errors.push(Error::new(
                    field.pos,
                    format!("`{}` is already a field of `{}`", field.name, def.name),
                ));
            }
            self.ty(&field.ty, scope);
        }
        for (field, ty) in def.fields.iter().zip(&defs.fields[s]) {
            if let Some(value) = &field.default {
                self.default(value, ty, scope, Vec::new(), Vec::new());
            }
        }
        // What a proof field says is a statement about the other fields.
        if let Some((function, signature)) = &defs.constructions[s] {
            self.contracts(function, signature);
        }
    }

    /// Checks an enum: its type parameters, each named by a case's type,
    /// and its cases, each named once, each of a type there is.
    fn enumeration(&mut self, e: usize) {
        let defs = self.defs;
        let def = &defs.file.enums[e];
        let scope = &defs.enum_scopes[e];
        self.generics(&def.generics, &format!("`{}`", def.name), Owner::Enum);
        for g in &def.generics {
            let named = (def.cases.iter())
                .filter_map(|c| c.ty.as_ref())
                .any(|t| type_names(t).iter().any(|(n, _)| *n == g.name));
            if !named {
                self.errors.push(Error::unsupported(
                    g.pos,
                    "a type parameter that no case's type names",
                ));
            }
        }
        for (i, case) in def.cases.iter().enumerate() {
            if def.cases[..i].iter().any(|c| c.name == case.name) {
                self.errors.push(Error::new(
                    case.pos,
                    format!("`{}` is already a case of `{}`", case.name, def.name),
                ));
            }
            if let Some(t) = &case.ty {
                self.ty(t, scope);
            }
        }
    }

    /// Reports a value that gives `given` where `needed` is needed.
    fn expect(&mut self, body: &mut Body<'a>, value: &Expr, given: &Ty, needed: &Ty) {
        let defs = self.defs;
        if let Ty::Int(n) = needed {
            self.note_need(body, value, *n);
        }
        self.note_element(body, value, needed);
        match (given, needed) {
            (_, Ty::Unknown) | (Ty::Unknown, _) => {}
            (g, n)
                if {
                    use_at(value.pos());
                    flex_pair(g, n)
                } => {}
            (Ty::Nothing, needed) if needed != given => {
                let needed = defs.describe(needed, body.scope);
                self.errors.push(self.nothing_where(value, &needed));
            }
            // A range is taken where a wider one is needed, with nothing written.
            (Ty::Int(g), Ty::Int(n)) if n.holds(*g) => {
                if g != n {
                    self.widened.insert(addr(value), *n);
                }
            }
            // A range is taken where a narrower one is needed only where it
            // is shown to fit.
            (Ty::Int(g), Ty::Int(n)) => {
                self.widened.insert(addr(value), *n);
                self.enters(body, value, (*g, *n));
            }
            // A name that holds a function with a name is a function of that
            // function's type.
            (Ty::Named(g), Ty::Fn(items)) if defs.function_items(*g) == *items => {}
            // A lambda that holds the values of the names it read is a function of
            // its own types.
            (Ty::Struct(e, held), Ty::Fn(items))
                if defs.env_items(*e, held).as_ref() == Some(items) => {}
            (Ty::Fn(_), Ty::Named(n)) if self.fn_values.contains_key(&addr(value)) => {
                self.named.insert(addr(value));
                if self.fn_values[&addr(value)] != *n {
                    self.errors.push(Error::new(value.pos(), WHICH_FUNCTION));
                }
            }
            (given, needed) if given.differs_in_function(needed) => {
                self.errors.push(Error::new(value.pos(), WHICH_FUNCTION));
            }
            (given, needed) if given != needed => {
                let message = self.mismatch(
                    body,
                    value,
                    defs.describe(given, body.scope),
                    defs.describe(needed, body.scope),
                );
                self.errors.push(Error::new(value.pos(), message));
            }
            _ => {}
        }
    }

    /// Reports a value that gives `given` where a value of `pattern` is
    /// needed, whose type parameters, those of `vars`, stand for the types
    /// in `sol`, and fills in `sol`.
    fn fit(
        &mut self,
        body: &mut Body<'a>,
        value: &Expr,
        given: &Ty,
        pattern: &Ty,
        sol: &mut [Option<Ty>],
        vars: &Scope,
    ) {
        let defs = self.defs;
        self.note_element(body, value, pattern);
        match (given, pattern) {
            (_, Ty::Unknown) | (Ty::Unknown, _) => {}
            (Ty::Nothing, pattern) if !matches!(pattern, Ty::Nothing | Ty::Param(_)) => {
                let needed = defs.describe_pattern(pattern, sol, vars, body.scope);
                self.errors.push(self.nothing_where(value, &needed));
            }
            // A range is taken where a wider one is needed, with nothing written.
            (Ty::Int(g), Ty::Int(n)) if n.holds(*g) => {
                if g != n {
                    self.widened.insert(addr(value), *n);
                }
            }
            (Ty::Int(g), Ty::Int(n)) => {
                self.widened.insert(addr(value), *n);
                self.enters(body, value, (*g, *n));
            }
            // A range is taken where a type that is a range is already settled,
            // as the item of an `Option[Byte]` that is asked for, as it is taken
            // where that range is written.
            (Ty::Int(g), Ty::Param(p)) if self.settled && matches!(&sol[*p], Some(Ty::Int(_))) => {
                let Some(Ty::Int(n)) = sol[*p].clone() else {
                    unreachable!("checked")
                };
                if n.holds(*g) {
                    if *g != n {
                        self.widened.insert(addr(value), n);
                    }
                } else {
                    self.widened.insert(addr(value), n);
                    self.enters(body, value, (*g, n));
                }
            }
            // The other side of a comparison is a range too.
            (Ty::Int(g), Ty::Param(p))
                if self.comparing
                    && matches!(&sol[*p], Some(Ty::Int(x)) if x != g && x.hull(*g).is_some()) =>
            {
                let Some(Ty::Int(x)) = &sol[*p] else {
                    unreachable!("checked")
                };
                sol[*p] = x.hull(*g).map(Ty::Int);
            }
            // Two ranges that no type of 64 bits holds are compared as they are.
            (Ty::Int(g), Ty::Param(p))
                if self.comparing
                    && matches!(&sol[*p], Some(Ty::Int(x)) if x != g && x.hull(*g).is_none()) => {}
            // The name of a function, given for a type parameter that a trait
            // bounds, is the type whose one value that function is.
            (Ty::Fn(_), Ty::Param(p))
                if self.fn_values.contains_key(&addr(value))
                    && (defs.bounded(vars, *p)
                        || vars.names[*p].contains('@')
                        || defs.file.functions[self.fn_values[&addr(value)]]
                            .name
                            .starts_with("lambda@")) =>
            {
                let f = self.fn_values[&addr(value)];
                let named = Ty::Named(f);
                self.named.insert(addr(value));
                if let Some(field) = vars.names[*p].strip_suffix("@function") {
                    self.stored_effects(value, f, &format!("the type of the field `{field}`"));
                }
                let asks_function = vars
                    .requires
                    .iter()
                    .any(|r| r.of == defs.function && r.types.contains(pattern));
                if asks_function
                    && defs.file.functions[f]
                        .params
                        .iter()
                        .any(|q| q.name.starts_with("pattern@"))
                {
                    let message = format!(
                        "`{}` has a pattern for a parameter, so it has no `Function`",
                        defs.file.functions[f].name
                    );
                    self.errors.push(Error::new(value.pos(), message));
                } else if !unify(pattern, &named, sol) {
                    let message = if matches!(pattern, Ty::Param(q) if matches!(sol[*q], Some(Ty::Named(_))))
                    {
                        WHICH_FUNCTION.to_string()
                    } else {
                        let needed = defs.describe_pattern(pattern, sol, vars, body.scope);
                        self.mismatch(body, value, defs.describe(&named, body.scope), needed)
                    };
                    self.errors.push(Error::new(value.pos(), message));
                }
            }
            // The name of the function a type names is a value of that type.
            (Ty::Fn(_), Ty::Named(n)) if self.fn_values.contains_key(&addr(value)) => {
                let f = self.fn_values[&addr(value)];
                self.named.insert(addr(value));
                if f != *n {
                    let message = self.mismatch(
                        body,
                        value,
                        defs.describe(&Ty::Named(f), body.scope),
                        defs.describe(pattern, body.scope),
                    );
                    self.errors.push(Error::new(value.pos(), message));
                }
            }
            // A name that holds a function with a name is a function of that
            // function's type.
            // A parameter of a function type held by a field is the function of
            // the argument at each call, which the result settles there.
            (Ty::Fn(_), Ty::Param(p))
                if !self.fn_values.contains_key(&addr(value))
                    && matches!(value, Expr::Name { .. })
                    && vars.names[*p].ends_with("@function") =>
            {
                if sol[*p].is_none() {
                    sol[*p] = Some(Ty::Unknown);
                }
            }
            (Ty::Named(g), Ty::Param(p))
                if !self.fn_values.contains_key(&addr(value))
                    && vars.names[*p].ends_with("@function") =>
            {
                let field = vars.names[*p].trim_end_matches("@function").to_string();
                self.stored_effects(value, *g, &format!("the type of the field `{field}`"));
                if !unify(pattern, given, sol) {
                    self.errors.push(Error::new(value.pos(), WHICH_FUNCTION));
                }
            }
            (Ty::Named(g), Ty::Fn(_)) if !self.fn_values.contains_key(&addr(value)) => {
                let items = Ty::func(defs.function_items(*g));
                if !unify(pattern, &items, sol) {
                    let needed = defs.describe_pattern(pattern, sol, vars, body.scope);
                    let message =
                        self.mismatch(body, value, defs.describe(&items, body.scope), needed);
                    self.errors.push(Error::new(value.pos(), message));
                }
            }
            (Ty::Struct(e, held), Ty::Fn(_)) if defs.env_lambda(*e).is_some() => {
                let items = Ty::func(defs.env_items(*e, held).expect("checked"));
                if !unify(pattern, &items, sol) {
                    let needed = defs.describe_pattern(pattern, sol, vars, body.scope);
                    let message =
                        self.mismatch(body, value, defs.describe(given, body.scope), needed);
                    self.errors.push(Error::new(value.pos(), message));
                }
            }
            (given, pattern) if given.differs_in_function(&pattern.subst(&sol_types(sol))) => {
                self.errors.push(Error::new(value.pos(), WHICH_FUNCTION));
            }
            (given, pattern) => {
                use_at(value.pos());
                // What is needed is said as it is before the types are matched,
                // and only when they do not match.
                let before = sol.to_vec();
                let read = as_read(given, pattern);
                if !unify(pattern, &read, sol) {
                    let needed = defs.describe_pattern(pattern, &before, vars, body.scope);
                    let message = match given {
                        Ty::Text if read != *given => format!(
                            "this gives text, which is a `List[u8]`, where {needed} is needed"
                        ),
                        _ => self.mismatch(body, value, defs.describe(given, body.scope), needed),
                    };
                    self.errors.push(Error::new(value.pos(), message));
                }
            }
        }
    }

    /// What is wrong with `value`, which gives `given` where `needed` is
    /// needed. A name bound to a number whose type a use decided says which.
    fn mismatch(&self, body: &Body, value: &Expr, given: String, needed: String) -> String {
        if let Expr::Name { name, .. } = value
            && let Some((int, Some(at))) = body.decided.get(name)
        {
            return format!(
                "`{name}` is `{}`, as the use at {}:{} needs, where {needed} is needed",
                int.name(),
                at.line,
                at.col
            );
        }
        if let Expr::Name { name, .. } = value
            && let Some(said) = self.element_inferred(body, name, &given, &needed, value.pos())
        {
            return said;
        }
        format!("this gives {given}, where {needed} is needed")
    }

    fn nothing_where(&self, value: &Expr, needed: &str) -> Error {
        let what = match value {
            Expr::Call(call) => format!("`{}` returns nothing", call.callee),
            _ => "this gives nothing".to_string(),
        };
        Error::new(value.pos(), format!("{what}, where {needed} is needed"))
    }

    /// Checks the body of a function. A whole number bound to a name with no
    /// type written takes the type its uses need, so the body is first looked
    /// at with each such number undecided, to find what its uses ask of it,
    /// and then checked, with each decided and the mistakes reported.
    fn body(&mut self, template: Template) {
        if let Template::Free(f) = template
            && is_proof(&self.defs.file.functions[f])
        {
            self.proof_function(f);
            return;
        }
        let errors = self.errors.len();
        let stmts = &self.defs.view(template).f.body;
        self.infer_on = infer::has_inferred(stmts);
        self.patterned = infer::patterned(stmts);
        self.solved = infer::Solved::default();
        self.solve = None;
        if self.infer_on {
            self.solve_ranges(template);
        }
        self.look_twice(template);
        self.infer_on = false;
        if self.errors.len() == errors {
            let view = self.defs.view(template);
            let scope = self.extended.get(&addr(view.f)).unwrap_or(view.scope);
            let unmarked = ownership::check(
                self.defs,
                &self.types,
                (view.f, scope, &view.params),
                &mut self.errors,
                &mut self.drops,
            );
            for (call, at) in unmarked {
                self.unmarked_take(&call, &at);
            }
        }
    }

    /// Looks at the body twice: first at how the numbers bound to names are
    /// used, which decides their types, and then with them decided.
    fn look_twice(&mut self, template: Template) {
        USES.with(|u| *u.borrow_mut() = Uses::default());
        let (errors, refusals, no_impl) = (self.errors.len(), self.refusals, self.no_impl);
        let places = self.run_body(template, Numbers::Finding(Vec::new()));
        self.errors.truncate(errors);
        self.refusals = refusals;
        self.no_impl = no_impl;
        let uses = USES.with(|u| std::mem::take(&mut *u.borrow_mut()));
        // The numbers that must be of one type are decided together.
        let mut root: Vec<usize> = (0..places.len()).collect();
        fn find(root: &mut [usize], k: usize) -> usize {
            if root[k] != k {
                let r = find(root, root[k]);
                root[k] = r;
            }
            root[k]
        }
        for (j, k) in &uses.same {
            let (j, k) = (find(&mut root, *j), find(&mut root, *k));
            root[j.max(k)] = j.min(k);
        }
        let mut decided = HashMap::new();
        for (k, pos) in places.iter().enumerate() {
            let r = find(&mut root, k);
            let first = (uses.wants.iter())
                .filter(|(w, ..)| find(&mut root, *w) == r)
                .min_by_key(|(.., at)| *at);
            let decision = match first {
                Some((_, int, at)) => (*int, Some(*at)),
                None => (Int::I64, None),
            };
            decided.insert(*pos, decision);
        }
        self.run_body(template, Numbers::Decided(decided));
    }

    /// Finds the range of each `mut` with no type written: the body is looked
    /// at again, with what it says of each so far, until no range grows. A
    /// `mut` that still grows after as many looks as there are `mut`s is
    /// changed by itself, and is refused.
    fn solve_ranges(&mut self, template: Template) {
        let (errors, refusals, no_impl) = (self.errors.len(), self.refusals, self.no_impl);
        let widened = self.widened.clone();
        let mut before: HashMap<Pos, Int> = HashMap::new();
        let mut before_elems: HashMap<Pos, Ty> = HashMap::new();
        let mut put: HashMap<Pos, Vec<(Pos, String)>> = HashMap::new();
        let mut looks = 0;
        let mut refused = HashSet::new();
        let last = loop {
            self.solve = Some(infer::Solve {
                before: before.clone(),
                before_elems: before_elems.clone(),
                ..infer::Solve::default()
            });
            self.look_twice(template);
            self.errors.truncate(errors);
            self.refusals = refusals;
            self.no_impl = no_impl;
            let mut solve = self.solve.take().expect("solving");
            let defs = self.defs;
            self.templates(&mut solve, &defs.view(template).f.body);
            looks += 1;
            let grown: Vec<Pos> = solve
                .totals
                .iter()
                .filter(|(p, t)| before.get(*p) != Some(*t))
                .map(|(p, _)| *p)
                .collect();
            for (def, found) in &solve.put {
                let held = put.entry(*def).or_default();
                for one in found {
                    if !held.iter().any(|(p, _)| *p == one.0) {
                        held.push(one.clone());
                    }
                }
            }
            let merged = solve.merged_elems();
            let settled = merged == before_elems;
            // Where no value that a `mut` is given widened it, what was read
            // as its range in this look is what it has, and no look is needed
            // to find it.
            let unchanged = looks == 1
                && solve.stuck.is_empty()
                && solve.empties.is_empty()
                && solve
                    .totals
                    .iter()
                    .all(|(p, t)| solve.first.get(p) == Some(t));
            let grown = if unchanged { Vec::new() } else { grown };
            if (grown.is_empty() && settled)
                || unchanged
                || looks > solve.totals.len() + solve.empties.len() + 1
            {
                let mut stuck: Vec<Pos> = solve.stuck.iter().copied().collect();
                stuck.extend(grown);
                stuck.sort();
                stuck.dedup();
                // A `mut` first given a value that has a type, and that is given
                // itself, has the range of that value, as it had before it was
                // solved from every value. One first given a number written out
                // has nothing to go on, and is refused.
                let mut solve = solve;
                for def in stuck {
                    if solve.written.contains(&def) {
                        let error = self.not_finite(def, &solve);
                        self.errors.push(error);
                        refused.insert(def);
                    } else if let Some((_, _, first)) =
                        solve.given.get(&def).and_then(|g| g.first())
                    {
                        let first = *first;
                        solve.totals.insert(def, first);
                    }
                }
                break solve;
            }
            before = solve.totals;
            before_elems = merged;
        };
        self.widened = widened;
        self.solved.elems = last.merged_elems();
        self.solved.ranges = last.totals;
        self.solved.given = last.given;
        self.solved.templated = last
            .adds
            .keys()
            .filter(|d| !last.unbounded.contains_key(*d))
            .copied()
            .collect();
        self.solved.refused = refused;
        self.solved.put = put;
    }

    /// One look at the body of a function, which gives the places of the
    /// literals of the numbers it bound to names with no type written.
    fn run_body(&mut self, template: Template, numbers: Numbers) -> Vec<Pos> {
        let defs = self.defs;
        let view = defs.view(template);
        let f = view.f;
        let errors_before = self.errors.len();
        let mut listed = Vec::new();
        let mut unknown_effect = false;
        for (effect, _) in view.effects {
            let effect = effect.as_str();
            if !listed.contains(&effect) {
                listed.push(effect);
                unknown_effect |= !BUILT.contains(&effect)
                    && !defs.declares(effect)
                    && !LIBRARY_EFFECTS.contains(&effect)
                    && !BUILT_IN_EFFECTS.contains(&effect);
            }
        }
        self.inferred.clear();
        self.seen.clear();
        self.gave.clear();
        self.lambda_from.clear();
        self.literal_from.clear();
        let mut body = Body {
            f: Some(f),
            default: None,
            scope: view.scope,
            listed,
            extra: Vec::new(),
            unknown_effect,
            missing: Vec::new(),
            missing_at: Vec::new(),
            used: Vec::new(),
            names: view.params.clone(),
            params: view.params.len(),
            // A parameter marked `mut` is changed for the caller, so it is
            // assigned to as a `mut` is.
            vars: (f.params.iter().filter(|p| p.mutable))
                .map(|p| p.name.clone())
                .collect(),
            numbers,
            decided: HashMap::new(),
            facts: proof::Facts::default(),
            tails: Vec::new(),
            gives: view.gives.clone(),
            contract: false,
            loops: 0,
            outer_loops: 0,
            returns: view.gives.clone(),
            returned: None,
            lambdas: 0,
            kept: 0,
            proofs: Vec::new(),
            call_proofs: HashMap::new(),
            parts: Vec::new(),
        };
        body.facts.solving = self.solve.is_some();
        body.facts.reads_bytes = self.needs_boundaries(f);
        if let Template::Free(_) = template {
            self.hold_changed(&mut body, f);
            for requirement in &f.requires {
                self.assume(&mut body, requirement, true);
            }
            self.hold_params(&mut body, f);
            self.measure_parts(&mut body, f);
            if !f.ensures.is_empty() {
                proof::tails(&f.body, &mut body.tails);
            }
        } else if let Template::Impl(i, place) = template
            && f.requires.is_empty()
            && f.ensures.is_empty()
        {
            // What the trait's function requires holds in the body of the one
            // that implements it.
            let info = self.defs.impls[i].as_ref().expect("known");
            if let Some(fi) = info.fns.iter().position(|p| *p == Some(place)) {
                body.facts.sibling = Some(info.of);
                for requirement in &self.defs.file.traits[info.of].functions[fi].requires {
                    self.assume(&mut body, requirement, true);
                }
                body.facts.sibling = None;
            }
        } else if let Some(contract) = f.requires.first().or(f.ensures.first()) {
            self.errors.push(Error::unsupported(
                contract.pos(),
                "a contract of an implementation",
            ));
        }
        let want = match view.result {
            Some(_) => Want::Ty(view.gives.clone()),
            None => Want::Unused,
        };
        match (self.block(&mut body, &f.body, &want), &view.result) {
            _ if proof::ends_in_exit(&f.body) => {}
            (End::Empty | End::Binding(_), Some(_)) if view.gives == Ty::Nothing => {}
            (End::Empty, Some(t)) => {
                let t = t.text(&view, self.defs);
                self.errors.push(Error::new(
                    f.end,
                    format!(
                        "`{}` returns `{t}`, and its body is empty, which gives nothing",
                        f.name
                    ),
                ))
            }
            (End::Binding(pos), Some(t)) => {
                let t = t.text(&view, self.defs);
                self.ends_with_nothing(f, pos, &t)
            }
            (End::Value(gives, value), Some(result)) => {
                let written =
                    matches!(result, Written::Source(_)).then(|| result.text(&view, self.defs));
                let defs = self.defs;
                defs.writing(&view.gives, written, || {
                    self.expect(&mut body, value, &gives, &view.gives)
                })
            }
            (
                End::Value(
                    gives
                    @ (Ty::Text | Ty::Struct(..) | Ty::Enum(..) | Ty::Tuple(_) | Ty::Param(_)),
                    value,
                ),
                None,
            ) => self.errors.push(Error::new(
                value.pos(),
                format!(
                    "`{}` returns nothing, and its body ends with {}",
                    f.name,
                    defs.describe(&gives, view.scope)
                ),
            )),
            _ => {}
        }
        if !body.missing_at.is_empty() && !f.name.starts_with("lambda@") {
            let listed: Vec<&str> = body.listed.iter().map(|e| written_name(e)).collect();
            let missing: Vec<&str> = body.missing.iter().map(|e| written_name(e)).collect();
            let effects = defs.file.fix_effects(f, &listed, &missing);
            let effects: Vec<&str> = effects.iter().map(|e| e.as_str()).collect();
            let fix = format!("write `{}`", written(f, &effects));
            for &at in &body.missing_at {
                self.errors[at].fix = Some(fix.clone());
            }
        }
        if !view.effects.is_empty()
            && matches!(template, Template::Free(_))
            && !body.unknown_effect
            && self.errors.len() == errors_before
        {
            let is_used = |e: &str| body.used.iter().any(|u| written_name(u) == written_name(e));
            // A group is used when anything in it is.
            let entries = defs.file.listed(f.pos, view.effects);
            let group_used = |e: &str| match defs.file.holds(e) {
                Some(held) => held.iter().any(|h| is_used(h)),
                None => is_used(e),
            };
            let unused: Vec<&(String, Pos)> =
                entries.iter().filter(|(e, _)| !group_used(e)).collect();
            if !unused.is_empty() {
                let kept: Vec<&str> = (entries.iter())
                    .filter(|(e, _)| group_used(e))
                    .map(|(e, _)| written_name(e))
                    .collect();
                let fix = written(f, &kept);
                for (effect, at) in unused {
                    // A function of the library lists `task` to be built for the
                    // scheduler it runs under, though it asks nothing of it.
                    if written_name(effect) == "task" && of_std(f.pos) {
                        continue;
                    }
                    let known = (self.unused_effects.iter())
                        .any(|(_, _, e, p, _, _)| e == effect && p == at);
                    if !known {
                        self.unused_effects.push((
                            f.pos,
                            f.name.clone(),
                            effect.clone(),
                            *at,
                            fix.clone(),
                            defs.file.holds(effect).is_some(),
                        ));
                    }
                }
            }
        }
        match body.numbers {
            Numbers::Finding(places) => places,
            _ => Vec::new(),
        }
    }

    /// The error for defining `name` at `pos` where it is already defined.
    fn defined_twice(&self, body: &Body, name: &str, pos: Pos) -> Error {
        match body.f {
            Some(f) => Error::new(pos, format!("`{name}` is already defined in `{}`", f.name)),
            None => Error::new(pos, format!("`{name}` is already defined here")),
        }
    }

    /// Checks the statements of a block in order, the last against `last`,
    /// and gives how it ends.
    fn block<'e>(&mut self, body: &mut Body<'a>, stmts: &'e [Stmt], last: &Want) -> End<'e> {
        let mut end = End::Empty;
        let first_return = stmts
            .iter()
            .position(|s| matches!(s, Stmt::Expr(Expr::Return { .. })));
        for (i, stmt) in stmts.iter().enumerate() {
            let is_last = i + 1 == stmts.len();
            if first_return.is_some_and(|r| r + 1 == i) {
                let at = match stmt {
                    Stmt::Define { pos, .. } | Stmt::Assign { pos, .. } => *pos,
                    Stmt::Destructure { pattern, .. } => pattern.pos(),
                    Stmt::Expr(value) => value.pos(),
                };
                self.errors.push(Error::new(
                    at,
                    "this line never runs, as the `return` before it leaves the function or lambda",
                ));
            }
            end = match stmt {
                Stmt::Define {
                    name,
                    pos,
                    value: Expr::Lambda(l),
                    ..
                } if self.defs.env_of(l).is_some() => self.bind_env(body, name, *pos, l),
                Stmt::Define {
                    name, pos, value, ..
                } if matches!(value, Expr::Lambda(_)) || lambda_choice(value).is_some() => {
                    let message = match (value, lambda_choice(value)) {
                        (_, Some(at)) => Error::new(at, WHICH_FUNCTION),
                        (Expr::Lambda(l), _) if leaves_a_type_out(l) => Error::new(
                            value.pos(),
                            "a lambda bound to a name writes the type of each parameter and of its result, as `fn(v: f32) -> f32 { ... }`, since no function type decides them",
                        ),
                        _ => Error::new(
                            value.pos(),
                            "a lambda cannot be bound to a name, as it is only passed to a call",
                        ),
                    };
                    self.errors.push(message);
                    if !body.names.iter().any(|(n, _)| n == name) {
                        body.names.push((name.clone(), Ty::Unknown));
                    }
                    End::Binding(*pos)
                }
                Stmt::Assign { pos, value, .. } if matches!(value, Expr::Lambda(_)) => {
                    self.errors.push(match value {
                        Expr::Lambda(l) if leaves_a_type_out(l) => Error::new(
                            value.pos(),
                            "a lambda bound to a name writes the type of each parameter and of its result, as `fn(v: f32) -> f32 { ... }`, since no function type decides them",
                        )
,
                        _ => Error::new(
                            value.pos(),
                            "a lambda cannot be bound to a name, as it is only passed to a call",
                        ),
                    });
                    End::Binding(*pos)
                }
                Stmt::Destructure { pattern, value, .. } if matches!(value, Expr::Lambda(_)) => {
                    self.errors.push(Error::new(
                        value.pos(),
                        "a lambda cannot be bound to a name, as it is only passed to a call",
                    ));
                    let mut binds = Vec::new();
                    self.pattern(
                        body,
                        pattern,
                        &Ty::Unknown,
                        &mut binds,
                        Some(&mut Vec::new()),
                    );
                    body.define(binds);
                    End::Binding(pattern.pos())
                }
                Stmt::Expr(value @ Expr::Lambda(l)) if self.defs.env_of(l).is_some() => {
                    let held = self.held_lambda(body, l, "returned", true);
                    self.types.insert(addr(value), held.clone());
                    End::Value(held, value)
                }
                Stmt::Expr(value @ Expr::Lambda(l)) => {
                    let message = if is_last && matches!(last, Want::Ty(_)) {
                        "a lambda cannot be returned, as it is only passed to a call"
                    } else if l.params.is_empty() {
                        "a block that stands alone is not an expression"
                    } else {
                        "a lambda that stands alone is not an expression, as it is only passed to \
                         a call"
                    };
                    self.errors.push(Error::new(l.pos, message));
                    End::Value(Ty::Unknown, value)
                }
                Stmt::Define {
                    name,
                    pos,
                    ty,
                    var,
                    value,
                } if name == "_" && !*var => {
                    // `_ = e` runs `e` and throws its value away, and defines
                    // no name.
                    let gives = match ty {
                        Some(t) => {
                            let needed = self.ty(t, body.scope);
                            let gives = self.expr(body, value, &Want::Ty(needed.clone()));
                            self.expect(body, value, &gives, &needed);
                            needed
                        }
                        None => self.expr(body, value, &Want::Any),
                    };
                    self.not_stored(value, gives);
                    End::Binding(*pos)
                }
                Stmt::Define {
                    name,
                    pos,
                    ty,
                    var,
                    value,
                } => {
                    let defined = body.names.iter().position(|(n, _)| n == name);
                    match (defined, ty, var) {
                        (Some(at), None, false) => {
                            let needed = body.names[at].1.clone();
                            if body.vars.contains(name)
                                && self.solve.is_some()
                                && self.inferred.contains_key(&at)
                            {
                                self.infer_assign(body, at, (name, *pos), value);
                            } else if body.vars.contains(name)
                                && self
                                    .inferred
                                    .get(&at)
                                    .is_some_and(|d| self.solved.templated.contains(d))
                                && self.adds_to(name, value)
                            {
                                // The count of passes shows that it fits, as
                                // [`Checker::templates`] says.
                                self.expr(body, value, &Want::Any);
                                self.assign(body, at, value);
                            } else if body.vars.contains(name) {
                                let gives = self.expr(body, value, &Want::Ty(needed.clone()));
                                // A number that its uses have not decided is decided by
                                // them, and not by the range of a `mut` that is solved.
                                if !(matches!(gives, Ty::Flex(_))
                                    && self.inferred.contains_key(&at))
                                {
                                    self.expect(body, value, &gives, &needed);
                                }
                                self.assign(body, at, value);
                            } else {
                                self.expr(body, value, &Want::Any);
                                self.errors.push(not_a_var(body, name, *pos, at));
                            }
                        }
                        _ => {
                            let mut named = Vec::new();
                            let mut first = None;
                            let mut registered = false;
                            let bound = match ty {
                                Some(t)
                                    if t.range.as_ref().is_some_and(|(a, b)| {
                                        matches!(a, Bound::Name(_) | Bound::Size(_))
                                            || matches!(b, Bound::Name(_) | Bound::Size(_))
                                    }) =>
                                {
                                    let needed = match self.named_range(body, t) {
                                        Some((needed, bounds)) => {
                                            named = bounds;
                                            needed
                                        }
                                        None => Ty::Unknown,
                                    };
                                    let gives = self.expr(body, value, &Want::Ty(needed.clone()));
                                    // What is not shown of the names is said once, not
                                    // again of the range that holds every number the
                                    // range can have.
                                    let before = self.errors.len();
                                    for (bound, end) in &named {
                                        self.bounded(body, value, (bound, *end), t.pos);
                                    }
                                    let taken = match (&gives, &needed) {
                                        (Ty::Int(g), Ty::Int(n)) if self.errors.len() > before => {
                                            g.hull(*n).map_or(needed.clone(), Ty::Int)
                                        }
                                        _ => needed.clone(),
                                    };
                                    self.expect(body, value, &gives, &taken);
                                    needed
                                }
                                Some(t) => {
                                    let needed = self.ty(t, body.scope);
                                    let defs = self.defs;
                                    defs.writing(&needed, Some(written_type(t)), || {
                                        let gives =
                                            self.expr(body, value, &Want::Ty(needed.clone()));
                                        self.expect(body, value, &gives, &needed);
                                    });
                                    needed
                                }
                                None if defined.is_none() && self.infers_empty(body, value) => {
                                    registered = true;
                                    self.infer_empty(body, *pos, value)
                                }
                                None if *var && defined.is_none() && self.infers(body, name) => {
                                    let (bound, held) = self.infer_var(body, *pos, name, value);
                                    first = held;
                                    bound
                                }
                                None if (matches!(value, Expr::Number { .. })
                                    || matches!(value, Expr::Match(m) if literal_blocks(m)))
                                    && !matches!(body.numbers, Numbers::Off) =>
                                {
                                    self.literal_binding(body, (name, *var), value)
                                }
                                None => self.expr(body, value, &Want::Any),
                            };
                            let bound = match self.fn_values.get(&addr(value)) {
                                Some(&f) if want_any_fn(&bound) => {
                                    self.named.insert(addr(value));
                                    self.types.insert(addr(value), Ty::Named(f));
                                    Ty::Named(f)
                                }
                                _ => self.not_stored(value, bound),
                            };
                            if defined.is_some() {
                                self.errors.push(self.defined_twice(body, name, *pos));
                            } else {
                                body.names.push((name.clone(), bound));
                                if registered {
                                    self.inferred.insert(body.names.len() - 1, *pos);
                                }
                                if *var {
                                    body.vars.push(name.clone());
                                    self.track_var(body, value);
                                    if let Some(held) = first {
                                        self.inferred.insert(body.names.len() - 1, *pos);
                                        if self.solve.is_some() {
                                            body.facts.set_step(body.names.len() - 1, held);
                                        }
                                    }
                                } else {
                                    self.assume_binding(body, value);
                                    self.note_bounds(body, &named);
                                    self.hold_called_proof(body, name, value);
                                }
                            }
                        }
                    }
                    End::Binding(*pos)
                }
                Stmt::Assign {
                    name,
                    pos,
                    fields,
                    at,
                    value,
                } => {
                    let mut needed = self.place(body, name, *pos, fields);
                    // An element of an array is the place, and its index is proven
                    // as when it is read.
                    if let (Some(at), Some(held)) = (at, &needed) {
                        needed = match held {
                            Ty::Array(..) => Some(self.index(body, &at.0, &at.1, *pos)),
                            Ty::Unknown => None,
                            given => {
                                let given = self.defs.describe(given, body.scope);
                                self.errors.push(Error::new(
                                    *pos,
                                    format!(
                                        "an element is assigned in an array, and this is {given}"
                                    ),
                                ));
                                None
                            }
                        };
                    }
                    let want = needed.clone().map_or(Want::Any, Want::Ty);
                    let gives = self.expr(body, value, &want);
                    if let Some(needed) = needed {
                        self.expect(body, value, &gives, &needed);
                    }
                    if let Some(at) = body.names.iter().position(|(n, _)| n == name) {
                        self.assign_field(body, at);
                    }
                    End::Binding(*pos)
                }
                Stmt::Destructure { pattern, ty, value } => {
                    let gives = match ty {
                        Some(t) => {
                            let needed = self.ty(t, body.scope);
                            let defs = self.defs;
                            defs.writing(&needed, Some(written_type(t)), || {
                                let gives = self.expr(body, value, &Want::Ty(needed.clone()));
                                self.expect(body, value, &gives, &needed);
                            });
                            needed
                        }
                        None => self.expr(body, value, &Want::Any),
                    };
                    let gives = self.not_stored(value, gives);
                    let mut binds = Vec::new();
                    let mut assigned = Vec::new();
                    self.pattern(body, pattern, &gives, &mut binds, Some(&mut assigned));
                    body.define(binds);
                    let held = self.assigned_vars(body, &[std::slice::from_ref(stmt)]);
                    self.forget(body, &held);
                    End::Binding(pattern.pos())
                }
                Stmt::Expr(value) => {
                    let want = if is_last { last } else { &Want::Unused };
                    let gives = self.expr(body, value, want);
                    if !is_last {
                        self.dropped(body, value, &gives);
                    }
                    End::Value(gives, value)
                }
            };
        }
        end
    }

    /// Reports a value of type `ty` that holds a function, which a binding
    /// cannot store yet, and gives the type the binding then has.
    fn not_stored(&mut self, value: &Expr, ty: Ty) -> Ty {
        if ty.holds_fn() {
            self.errors.push(Error::unsupported(
                value.pos(),
                "a function stored in a binding",
            ));
            return Ty::Unknown;
        }
        ty
    }

    /// Checks a field of a `mut`, or a field of one of its fields, as a
    /// place to assign to, and gives the type of what it holds. Gives
    /// `None` when the place holds a mistake, which is reported.
    fn place(&mut self, body: &Body, name: &str, pos: Pos, fields: &[(String, Pos)]) -> Option<Ty> {
        let defs = self.defs;
        let Some(at) = body.names.iter().position(|(n, _)| n == name) else {
            self.errors.push(Error::new(
                pos,
                format!(
                    "there is no name `{name}` defined before this{}",
                    body.in_owner()
                ),
            ));
            return None;
        };
        if !body.vars.iter().any(|v| v == name) {
            self.errors.push(not_a_var(body, name, pos, at));
            return None;
        }
        let mut ty = body.names[at].1.clone();
        for (k, (field, pos)) in fields.iter().enumerate() {
            ty = match ty {
                Ty::Struct(s, args) => {
                    let def = &defs.file.structs[s];
                    match def.fields.iter().position(|f| &f.name == field) {
                        Some(at) => {
                            if hidden(def, &def.fields[at], *pos) {
                                self.errors.push(private_field(def, field, *pos));
                            }
                            if k + 1 == fields.len() {
                                let held: Vec<&str> =
                                    fields[..k].iter().map(|(f, _)| f.as_str()).collect();
                                let held = [name].into_iter().chain(held).collect::<Vec<_>>();
                                self.refuse_named_field(def, field, *pos, &held.join("."));
                            }
                            defs.fields[s][at].subst(&args)
                        }
                        None => {
                            self.errors.push(Error::new(
                                *pos,
                                format!("`{}` has no field named `{field}`", def.name),
                            ));
                            return None;
                        }
                    }
                }
                Ty::Unknown => return None,
                given => {
                    self.errors.push(Error::new(
                        *pos,
                        format!(
                            "the field `{field}` is assigned in {}, which is not a struct",
                            defs.describe(&given, body.scope)
                        ),
                    ));
                    return None;
                }
            };
        }
        Some(ty)
    }

    /// Refuses the assignment of a field that a proof field's statement, or
    /// the type of another field, names: it would leave the proof proving what
    /// is no longer so.
    fn refuse_named_field(&mut self, def: &Struct, field: &str, pos: Pos, held: &str) {
        for other in &def.fields {
            if other.name == field || !names_the_field(&other.ty, field) {
                continue;
            }
            let message = if other.ty.statement.is_some() {
                format!(
                    "`{field}` cannot be assigned, as `{}` is a proof about it",
                    other.name
                )
            } else {
                format!(
                    "`{field}` cannot be assigned, as the type of `{}` names it",
                    other.name
                )
            };
            // The value is built again with the other fields as they are read.
            let data = def.fields.iter().filter(|f| f.ty.statement.is_none());
            let given: Vec<String> = data
                .map(|f| match f.name == field {
                    true => format!("{}: e", f.name),
                    false => format!("{0}: {held}.{0}", f.name),
                })
                .collect();
            let of = crate::values::copy_of(&def.name)
                .map_or(written_name(&def.name), |(template, _)| template);
            let fix = format!("build the value again, as `{of}({})`", given.join(", "));
            self.errors.push(Error::new(pos, message).with_fix(fix));
            return;
        }
    }

    /// Checks an expression against what is wanted of it, and gives its
    /// type, which is recorded.
    fn expr(&mut self, body: &mut Body<'a>, expr: &Expr, want: &Want) -> Ty {
        crate::work::step(1);
        let ty = self.expr_of(body, expr, want);
        self.types.insert(addr(expr), ty.clone());
        if let Expr::Call(call) = expr {
            self.types.insert(addr(call), ty.clone());
        }
        self.refuse_decided_comparison(expr);
        if body.tails.contains(&addr(expr)) {
            self.ensure(body, expr);
        }
        ty
    }

    fn expr_of(&mut self, body: &mut Body<'a>, expr: &Expr, want: &Want) -> Ty {
        let defs = self.defs;
        match expr {
            Expr::Unbuilt { .. } => Ty::Unknown,
            Expr::Text { .. } => Ty::Text,
            Expr::Number {
                magnitude,
                negative,
                pos,
                ..
            } => self.number(body, expr, *magnitude, *negative, *pos, want),
            Expr::Float {
                text,
                negative,
                pos,
            } => self.float_literal(body, expr, (text, *negative), *pos, want),
            Expr::Interp { pos, parts } => self.interpolation(body, *pos, parts),
            Expr::Name { name, pos } => {
                if let Some(read) = body
                    .names
                    .iter()
                    .rposition(|(n, _)| n == name)
                    .and_then(|at| self.read_inferred(body, at))
                {
                    read
                } else if let Some((_, ty)) = body.names.iter().rfind(|(n, _)| n == name) {
                    ty.clone()
                } else if let (Some(f), None) = (defs.function(*pos, name), &body.default) {
                    self.function_value(body, expr, f, *pos)
                } else if name == "true" || name == "false" {
                    defs.bool_ty()
                } else {
                    let message = match &body.default {
                        Some(params) if params.contains(&name.as_str()) => {
                            format!("a default uses no parameter, and `{name}` is one")
                        }
                        Some(_) => format!("there is no name `{name}` for a default to use"),
                        None => format!(
                            "there is no name `{name}` defined before this{}",
                            body.in_owner()
                        ),
                    };
                    self.errors.push(Error::new(*pos, message));
                    Ty::Unknown
                }
            }
            Expr::Call(call) => self.call(body, call, want),
            Expr::Arith {
                op,
                left,
                right,
                op_pos,
            } => self.arith(body, expr, (*op, left, right, *op_pos), want),
            Expr::List { pos, items } => self.list_literal(body, *pos, items, want),
            Expr::Index { of, index, pos } => self.index(body, of, index, *pos),
            Expr::Slice { of, from, to, pos } => {
                self.piece(body, of, (from.as_deref(), to.as_deref()), *pos)
            }
            Expr::Field { of, name, pos } => {
                if let Some(e) = defs.qualified_case(of, &body.names) {
                    return self.case(body, Some(e), name, of.pos(), *pos, None, want);
                }
                match self.expr(body, of, &Want::Any) {
                    Ty::List(_) | Ty::Slice(_) | Ty::Text if name == "size" => Ty::Int(Int::U64),
                    Ty::Text if name == "bytes" => Ty::slice(Ty::Int(Int::U8)),
                    Ty::List(_) | Ty::Slice(_) => {
                        self.errors.push(Error::new(
                            *pos,
                            format!("a list has no field named `{name}`"),
                        ));
                        Ty::Unknown
                    }
                    Ty::Struct(s, args) => {
                        let def = &defs.file.structs[s];
                        match def.fields.iter().position(|f| &f.name == name) {
                            Some(at) => {
                                if hidden(def, &def.fields[at], *pos) {
                                    self.errors.push(private_field(def, name, *pos));
                                }
                                match defs.fields[s][at].subst(&args) {
                                    // A function that is not known yet is of the
                                    // function type the field is written with.
                                    Ty::Param(_) if defs.field_items[s][at].is_some() => {
                                        let items = defs.field_items[s][at].as_ref();
                                        Ty::func(
                                            items
                                                .expect("checked")
                                                .iter()
                                                .map(|t| t.subst(&args))
                                                .collect(),
                                        )
                                    }
                                    held => held,
                                }
                            }
                            None => {
                                self.errors.push(Error::new(
                                    *pos,
                                    format!("`{}` has no field named `{name}`", def.name),
                                ));
                                Ty::Unknown
                            }
                        }
                    }
                    Ty::Unknown => Ty::Unknown,
                    given => {
                        self.errors.push(Error::new(
                            *pos,
                            format!(
                                "the field `{name}` is read from {}, which is not a struct",
                                defs.describe(&given, body.scope)
                            ),
                        ));
                        Ty::Unknown
                    }
                }
            }
            Expr::Case { name, pos, payload } => {
                let at = Pos {
                    line: pos.line,
                    col: pos.col + 1,
                };
                self.case(body, None, name, *pos, at, payload.as_deref(), want)
            }
            Expr::Match(m) => self.match_expr(body, m, want),
            // A lambda that reads names, stored in a field.
            Expr::Lambda(l) if defs.env_of(l).is_some() => {
                self.held_lambda(body, l, "stored in a field", true)
            }
            Expr::Lambda(l) => match want {
                Want::Ty(Ty::Fn(items)) => self.lambda(body, l, items, false),
                Want::Ty(Ty::Unknown) => {
                    let unknown = vec![Ty::Unknown; l.params.len() + 1];
                    self.lambda(body, l, &unknown, true);
                    Ty::Unknown
                }
                Want::Ty(t) => {
                    let needed = defs.describe(t, body.scope);
                    self.errors.push(Error::new(
                        l.pos,
                        format!("this is a lambda, where {needed} is needed"),
                    ));
                    Ty::Unknown
                }
                Want::Any | Want::Unused => {
                    self.errors
                        .push(Error::new(l.pos, "a lambda is only passed to a call"));
                    Ty::Unknown
                }
            },
            Expr::Loop(l) => self.loop_expr(body, l),
            Expr::For(f) => self.for_expr(body, f, want),
            Expr::Break { pos } => {
                if body.loops == 0 {
                    let message = if body.outer_loops == 0 {
                        "`break` is outside a `loop`, so there is nothing for it to end"
                    } else {
                        "`break` is inside a lambda, which cannot end a `loop` outside it"
                    };
                    self.errors.push(Error::new(*pos, message));
                }
                Ty::Nothing
            }
            Expr::Return { value, pos } => {
                self.returning(body, value.as_deref(), *pos);
                Ty::Nothing
            }
            Expr::Tuple { items, .. } => {
                // What is wanted of each element, where it is known.
                let wants: Vec<Want> = match want {
                    Want::Ty(Ty::Tuple(of)) if of.len() == items.len() => (of.iter())
                        .map(|t| match t {
                            Ty::Unknown => Want::Any,
                            t => Want::Ty(t.clone()),
                        })
                        .collect(),
                    Want::Ty(Ty::Unknown) => vec![Want::Ty(Ty::Unknown); items.len()],
                    _ => vec![Want::Any; items.len()],
                };
                let mut given: Vec<Ty> = (items.iter().zip(&wants))
                    .map(|(item, want)| self.expr(body, item, want))
                    .collect();
                // A range is taken as an element where the element the tuple is
                // asked for is a range, as it is taken anywhere a type is needed.
                for ((item, got), want) in items.iter().zip(&mut given).zip(&wants) {
                    if let (Ty::Int(g), Want::Ty(Ty::Int(n))) = (&*got, want) {
                        let (g, n) = (*g, *n);
                        if g != n {
                            self.widened.insert(addr(item), n);
                            if !n.holds(g) {
                                self.enters(body, item, (g, n));
                            }
                            *got = Ty::Int(n);
                        }
                    }
                }
                Ty::tuple(given)
            }
        }
    }

    /// Checks a case named `name`: of enum `e` if it is written in full, or
    /// else of the enum `want` expects. `pos` is where it starts, and `at`
    /// where its name is.
    #[allow(clippy::too_many_arguments)]
    fn case(
        &mut self,
        body: &mut Body<'a>,
        e: Option<usize>,
        name: &str,
        pos: Pos,
        at: Pos,
        payload: Option<&Expr>,
        want: &Want,
    ) -> Ty {
        let defs = self.defs;
        let quiet = Want::Ty(Ty::Unknown);
        let expected = match want {
            Want::Ty(t) => Some(t),
            Want::Any | Want::Unused => None,
        };
        let error = |checker: &mut Self, body: &mut Body<'a>, error: Option<Error>| {
            checker.errors.extend(error);
            if let Some(p) = payload {
                checker.expr(body, p, &quiet);
            }
            Ty::Unknown
        };
        let e = match (e, expected) {
            (Some(e), _) => e,
            (None, Some(Ty::Enum(e, _))) => *e,
            (None, Some(Ty::Unknown)) => return error(self, body, None),
            (None, Some(t)) => {
                let needed = defs.describe(t, body.scope);
                let message = format!("`.{name}` is a case of an enum, where {needed} is needed");
                return error(self, body, Some(Error::new(pos, message)));
            }
            (None, None) => {
                let message = format!("the enum of `.{name}` is not known here");
                let fix = "write the enum's name before the dot, or give the binding a type";
                return error(self, body, Some(Error::new(pos, message).with_fix(fix)));
            }
        };
        let def = &defs.file.enums[e];
        let Some(k) = defs.case_of(e, name) else {
            let message = format!("`{}` has no case named `{name}`", def.name);
            return error(self, body, Some(Error::new(at, message)));
        };
        let vars = &defs.enum_scopes[e];
        let mut sol: Vec<Option<Ty>> = match expected {
            Some(Ty::Enum(f, args)) if *f == e => (args.iter())
                .map(|t| (!t.unknown()).then(|| t.clone()))
                .collect(),
            _ => vec![None; def.generics.len()],
        };
        let before = self.errors.len();
        match (&defs.cases[e][k], payload) {
            (Some(pattern), Some(p)) => {
                // A payload whose type nothing has settled yet is found from
                // the payload itself.
                let wanted = match pattern.partial(&sol) {
                    Ty::Unknown => Want::Any,
                    t => Want::Ty(t),
                };
                let gives = self.expr(body, p, &wanted);
                if gives.unknown() {
                    return Ty::Unknown;
                }
                self.settled = true;
                self.fit(body, p, &gives, pattern, &mut sol, vars);
                self.settled = false;
            }
            (Some(pattern), None) if pattern.solved(&sol) == Ty::Nothing => {}
            (Some(pattern), None) => {
                let holds = defs.describe_pattern(pattern, &sol, vars, body.scope);
                let message = format!("`.{name}` holds {holds}, and none is given");
                return error(self, body, Some(Error::new(pos, message)));
            }
            (None, Some(p)) => {
                let message = format!("`.{name}` holds nothing, so it takes no value");
                return error(self, body, Some(Error::new(p.pos(), message)));
            }
            (None, None) => {}
        }
        if self.errors.len() > before || self.function_for_parameter(&sol, pos) {
            return Ty::Unknown;
        }
        if let Some(g) = sol.iter().position(Option::is_none) {
            self.errors.push(Error::new(
                pos,
                format!(
                    "the type `{}` of `{}` is not known here",
                    def.generics[g].name, def.name
                ),
            ));
            return Ty::Unknown;
        }
        if defs.cyclic_enums[e] {
            return Ty::Unknown;
        }
        Ty::enumeration(e, sol.into_iter().map(|t| t.expect("solved")).collect())
    }

    /// Checks a `match`: the value it takes apart, each pattern against
    /// the value's type, each block, and that the patterns cover every
    /// value once with none that is never reached. Used as an expression,
    /// its blocks give the same type, which it gives.
    fn match_expr(&mut self, body: &mut Body<'a>, m: &Match, want: &Want) -> Ty {
        let defs = self.defs;
        if let Expr::Name { name, pos } = &m.of
            && body.proofs.iter().any(|(n, _)| n == name)
        {
            self.errors.push(
                Error::new(
                    *pos,
                    format!(
                        "`{name}` is a proof, which is erased, so no `match` or `if` picks a value by it"
                    ),
                )
                .with_fix("test the statement with `check`, which gives the proof where it holds"),
            );
            return Ty::Unknown;
        }
        match m.form {
            Form::Match | Form::Try => {}
            Form::If { otherwise } => return self.if_expr(body, m, otherwise, want),
            Form::Not | Form::And | Form::Or => return self.logic(body, m),
        }
        // What `try` splits is what the function it is written in gives
        // when it leaves.
        let split = match m.form {
            Form::Try => Want::Ty(Ty::Enum(defs.flow, vec![Ty::Unknown, body.returns.clone()])),
            _ => Want::Any,
        };
        let errors = self.errors.len();
        let mut of = self.expr(body, &m.of, &split);
        // A `try` that is refused still gives what the step holds when it
        // succeeds, so that what a block of it gives is known.
        if matches!(m.form, Form::Try)
            && self.errors.len() > errors
            && of.unknown()
            && let Expr::Call(split) = &m.of
            && let Some(Ty::Enum(_, held)) = self.types.get(&addr(&split.args[0].value))
            && let Some(kept) = held.first()
        {
            of = Ty::Enum(defs.flow, vec![kept.clone(), Ty::Unknown]);
        }
        let whole = self.whole_or_part(body, &m.of);
        let outer = body.names.len();
        let vars = body.vars.len();
        let known = body.facts.mark();
        let blocks: Vec<&[Stmt]> = m.arms.iter().map(|a| a.body.as_slice()).collect();
        let assigned = self.assigned_vars(body, &blocks);
        let (mut paths, mut excluded) = (Vec::new(), false);
        let mut patterns: Vec<Option<Pat>> = Vec::new();
        let mut first: Option<Ty> = None;
        let mut later = Vec::new();
        let mut ends = Vec::new();
        for arm in &m.arms {
            let mut binds = Vec::new();
            let pattern = self.pattern(body, &arm.pattern, &of, &mut binds, None);
            if let Some(p) = &pattern
                && !of.unknown()
                && patterns.iter().all(Option::is_some)
            {
                let before: Vec<&Pat> = patterns.iter().flatten().collect();
                if !useful(defs, &before, p, &of) {
                    self.errors.push(Error::new(
                        arm.pattern.pos(),
                        "this pattern is never reached, as the patterns before it match \
                         everything it matches",
                    ));
                }
            }
            patterns.push(pattern);
            body.define(binds);
            // What a case holds is a part of the value matched, and so is
            // each element of a tuple or a list.
            if let Some(k) = whole
                && !matches!(arm.pattern, Pattern::Bind { .. })
            {
                body.parts
                    .extend((outer..body.names.len()).map(|at| (at, k)));
            }
            self.arm(
                body,
                arm,
                want,
                (&mut first, &mut later, &mut ends),
                "a `match`",
            );
            if proof::ends_in_exit(&arm.body) {
                excluded = true;
            } else {
                paths.push(self.leave_arm(body, &known, &assigned));
            }
            body.truncate_names(outer);
            body.vars.truncate(vars);
            body.facts.reset(&known);
        }
        self.settle(body, later, &mut first);
        self.join(body, outer, paths, &assigned, excluded);
        if !of.unknown() && patterns.iter().all(Option::is_some) {
            let all: Vec<&Pat> = patterns.iter().flatten().collect();
            let left = missing(defs, &all, &of);
            if left == ["_"] {
                self.errors.push(
                    Error::new(m.pos, "this `match` does not cover the rest of the values")
                        .with_fix("end with a bare name that covers the rest"),
                );
            } else if !left.is_empty() {
                let left: Vec<&str> = left.iter().map(String::as_str).collect();
                let blocks = if left.len() == 1 { "a block" } else { "blocks" };
                let left = names(&left);
                self.errors.push(
                    Error::new(m.pos, format!("this `match` does not cover {left}")).with_fix(
                        format!(
                            "add {blocks} for {left}, or end with a bare name that covers the rest"
                        ),
                    ),
                );
            }
        }
        match want {
            Want::Unused => Ty::Nothing,
            Want::Ty(t) if !t.unknown() => t.clone(),
            _ => first.unwrap_or(Ty::Nothing),
        }
    }

    /// Checks the block of an arm of a `match`, or of an `if`, which an
    /// error names as `what`, against what is wanted of the whole or what
    /// the first block gives, which is `first` once known.
    ///
    /// Where nothing says the type yet, a block that ends in a literal does
    /// not decide it: the literal waits in `later` for the type of a block
    /// that follows, which `settle` gives it.
    fn arm<'e>(
        &mut self,
        body: &mut Body<'a>,
        arm: &'e Arm,
        want: &Want,
        (first, later, ends): (&mut Option<Ty>, &mut Vec<&'e Expr>, &mut Vec<(usize, Int)>),
        what: &str,
    ) {
        if let (Want::Any, None, Some((Stmt::Expr(tail), rest))) =
            (want, &*first, arm.body.split_last())
            && flexible(tail)
            && !proof::ends_in_exit(&arm.body)
        {
            self.block(body, rest, &Want::Unused);
            later.push(tail);
            return;
        }
        let arm_want = match (want, &*first) {
            (Want::Unused, _) => Want::Unused,
            (Want::Ty(t), _) => Want::Ty(t.clone()),
            // A block that gives a range may give another, so only a number
            // written out waits for the type of the first.
            (Want::Any, Some(Ty::Int(_))) if !matches!(arm.body.last(), Some(Stmt::Expr(e)) if flexible(e)) => {
                Want::Any
            }
            (Want::Any, Some(t)) => Want::Ty(t.clone()),
            (Want::Any, None) => Want::Any,
        };
        let end = self.block(body, &arm.body, &arm_want);
        // With no type needed, the blocks that give ranges give the smallest
        // range that holds them all, and each is taken as it.
        if let (Want::Any, End::Value(Ty::Int(g), value), false) =
            (want, &end, proof::ends_in_exit(&arm.body))
        {
            let held = match &*first {
                None => Some(*g),
                Some(Ty::Int(f)) => f.hull(*g),
                Some(_) => None,
            };
            if let Some(h) = held {
                ends.push((addr(*value), *g));
                for (at, int) in ends.iter() {
                    if *int == h {
                        self.widened.remove(at);
                    } else {
                        self.widened.insert(*at, h);
                    }
                }
                *first = Some(Ty::Int(h));
                return;
            }
        }
        if matches!(want, Want::Unused) {
            if let End::Value(gives, value) = &end {
                self.dropped(body, value, gives);
            }
        } else {
            let gives = self.arm_gives(body, arm, end, want, first, what);
            if !proof::ends_in_exit(&arm.body) {
                first.get_or_insert(gives);
            }
        }
    }

    /// Gives the blocks that ended in a literal, which waited for a type,
    /// the type of the first block that gave one, or an `i64` when none did.
    fn settle(&mut self, body: &mut Body<'a>, later: Vec<&Expr>, first: &mut Option<Ty>) {
        if later.is_empty() {
            return;
        }
        let ty = first.clone().unwrap_or(Ty::Int(Int::I64));
        for value in later {
            self.expr(body, value, &Want::Ty(ty.clone()));
        }
        first.get_or_insert(ty);
    }

    /// Checks an `if`: its condition, which is a `bool`, and its blocks.
    /// Without `else`, it gives `()`, and what its block gives is not read,
    /// unless it is a `Result`, whose failure must not be dropped.
    fn if_expr(&mut self, body: &mut Body<'a>, m: &Match, otherwise: bool, want: &Want) -> Ty {
        self.condition(body, &m.of);
        if m.checked.is_some() {
            self.checkable(body, &m.of);
        }
        let outer = body.names.len();
        let vars = body.vars.len();
        let proofs = body.proofs.len();
        let known = body.facts.mark();
        let blocks: Vec<&[Stmt]> = m.arms.iter().map(|a| a.body.as_slice()).collect();
        let assigned = self.assigned_vars(body, &blocks);
        let (mut paths, mut excluded) = (Vec::new(), false);
        if !otherwise {
            let arm = &m.arms[0];
            self.assume(body, &m.of, true);
            self.hold_proof(body, m);
            if let End::Value(gives, value) = self.block(body, &arm.body, &Want::Unused) {
                self.dropped(body, value, &gives);
            }
            if proof::ends_in_exit(&arm.body) {
                excluded = true;
            } else {
                paths.push(self.leave_arm(body, &known, &assigned));
            }
            body.truncate_names(outer);
            body.vars.truncate(vars);
            body.proofs.truncate(proofs);
            body.facts.reset(&known);
            // Where the condition is not so, nothing runs.
            self.assume(body, &m.of, false);
            paths.push(self.leave_arm(body, &known, &assigned));
            body.facts.reset(&known);
            self.join(body, outer, paths, &assigned, excluded);
            return Ty::Nothing;
        }
        let mut first = None;
        let mut later = Vec::new();
        let mut ends = Vec::new();
        for (k, arm) in m.arms.iter().enumerate() {
            self.assume(body, &m.of, k == 0);
            if k == 0 {
                self.hold_proof(body, m);
            }
            self.arm(
                body,
                arm,
                want,
                (&mut first, &mut later, &mut ends),
                "an `if`",
            );
            if proof::ends_in_exit(&arm.body) {
                excluded = true;
            } else {
                paths.push(self.leave_arm(body, &known, &assigned));
            }
            body.truncate_names(outer);
            body.vars.truncate(vars);
            body.proofs.truncate(proofs);
            body.facts.reset(&known);
        }
        self.settle(body, later, &mut first);
        self.join(body, outer, paths, &assigned, excluded);
        match want {
            Want::Unused => Ty::Nothing,
            Want::Ty(t) if !t.unknown() => t.clone(),
            _ => first.unwrap_or(Ty::Nothing),
        }
    }

    /// Where a `check` held, the name it bound is a proof of what it tested.
    fn hold_proof(&mut self, body: &mut Body<'a>, m: &Match) {
        if let Some((name, _)) = &m.checked {
            body.names.push((name.clone(), Ty::Nothing));
            let held = self.held(body, &m.of);
            body.proofs.push((name.clone(), held));
        }
    }

    /// Refuses what `check` is given when the statement names a `mut`, which
    /// holds another value later, or calls a function that carries an effect,
    /// as a proof is erased and says nothing of a value that changes.
    fn checkable(&mut self, body: &Body<'a>, test: &Expr) {
        let mut all = Vec::new();
        walk(test, &mut all);
        for e in all {
            match e {
                Expr::Name { name, pos } if body.vars.contains(name) => {
                    self.errors.push(
                        Error::new(
                            *pos,
                            format!("`check` tests a statement, which names no `mut`, and `{name}` is one"),
                        )
                        .with_fix("bind its value to a name that is not a `mut` first"),
                    );
                }
                Expr::Call(c) => {
                    let carries = c.qualifier.is_none()
                        && self.defs.function(c.name_pos, &c.callee).is_some_and(|f| {
                            !self.defs.file.functions[f].effects.is_empty()
                                || !self.defs.signatures[f].effects.is_empty()
                        });
                    if carries {
                        self.errors.push(
                            Error::new(
                                c.name_pos,
                                format!(
                                    "`check` tests a statement, which carries no effect, and `{}` does",
                                    c.callee
                                ),
                            )
                            .with_fix("call it first and test what it gave"),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Checks a `loop`, which gives nothing. Over a range, its body runs once
    /// for each whole number from the first end up to but not including the
    /// second, and `arith` knows that in it. Over a list, it runs once for
    /// each element, in order. Open-ended, it runs until a `break`, and its
    /// function carries `diverge`.
    fn loop_expr(&mut self, body: &mut Body<'a>, l: &Loop) -> Ty {
        let counter = match &l.range {
            Some(r) => Some((r, self.range_type(body, r))),
            None => {
                if l.decreases.is_none() && l.over.is_none() {
                    self.needs(body, "diverge", l.pos, "an open-ended `loop`");
                }
                None
            }
        };
        let mut passes = (0, None);
        let element = l.over.as_ref().map(|o| {
            let (element, most) = self.element_type(body, o);
            passes = most;
            (o, element)
        });
        if let Some(r) = &l.range {
            passes = self.range_rounds(body, r);
        }
        if let Some(solve) = self.solve.as_mut()
            && !matches!(body.numbers, Numbers::Finding(_))
        {
            solve.passes.insert(l.pos, passes);
        }
        let measure = l.decreases.as_ref();
        if let Some(e) = measure {
            let gives = self.measure_type(body, e);
            if !matches!(gives, Ty::Int(_) | Ty::Unknown | Ty::Flex(_)) {
                let given = self.defs.describe(&gives, body.scope);
                self.errors.push(Error::new(
                    e.pos(),
                    format!("`decreases` is a whole number, and this is {given}"),
                ));
            }
        }
        let assigned = self.assigned_vars(body, &[&l.body]);
        let outer = body.names.len();
        let vars = body.vars.len();
        let known = body.facts.mark();
        // The ends of a range are worked out once, when the loop starts, so
        // what they say is about the values before any round begins.
        if let Some((r, ty)) = counter {
            if body.names.iter().any(|(n, _)| n == &r.name) {
                self.errors.push(self.defined_twice(body, &r.name, r.pos));
            }
            body.names.push((r.name.clone(), ty));
            // An invariant is about the counter and the names outside.
            self.invariant = true;
            for invariant in &l.invariants {
                self.condition(body, invariant);
            }
            self.invariant = false;
            self.range_ordered(body, r);
            self.invariants_before(body, l, Some(r));
            self.assume_range(body, r);
        }
        if let Some((o, ty)) = element {
            // An invariant of a loop over a value is about the names outside
            // it, as the element is not one before the first pass.
            self.invariant = true;
            for invariant in &l.invariants {
                self.condition(body, invariant);
            }
            self.invariant = false;
            self.invariants_before(body, l, None);
            if body.names.iter().any(|(n, _)| n == &o.name) {
                self.errors.push(self.defined_twice(body, &o.name, o.pos));
            }
            body.names.push((o.name.clone(), ty));
            // Each element of a list or a slice is a part of it.
            if matches!(
                self.types.get(&addr(&o.value)),
                Some(Ty::List(_) | Ty::Slice(_))
            ) && let Some(k) = self.whole_or_part(body, &o.value)
            {
                body.parts.push((body.names.len() - 1, k));
            }
        }
        self.begin_round(body, &assigned);
        self.assume_invariants(body, l);
        let start = measure.and_then(|e| self.measure_of(body, std::slice::from_ref(e), &[]));
        body.loops += 1;
        self.rounds.push(passes.1);
        if let End::Value(gives, value) = self.block(body, &l.body, &Want::Unused) {
            self.dropped(body, value, &gives);
        }
        self.rounds.pop();
        body.loops -= 1;
        if let (Some(e), Some(start)) = (measure, &start) {
            self.decreased(body, l, e, start);
        }
        self.invariants_after_pass(body, l, l.range.as_ref());
        body.truncate_names(outer);
        body.vars.truncate(vars);
        body.facts.reset(&known);
        self.forget(body, &assigned);
        self.invariants_after_loop(body, l, l.range.as_ref(), &assigned);
        Ty::Nothing
    }

    /// Checks the value a `loop` runs over, which is of a type that
    /// implements `Iterate`, and gives the type of its elements. A text is
    /// read as its bytes. The value is worked out once, when the loop starts,
    /// so assigning a `mut` it was read from changes nothing about which
    /// elements the loop gives.
    fn element_type(&mut self, body: &mut Body<'a>, o: &Over) -> (Ty, infer::Passes) {
        let given = self.expr(body, &o.value, &Want::Any);
        if given.unknown() {
            return (Ty::Unknown, (0, None));
        }
        let rounds = self.over_rounds(body, o, &given);
        (self.element_of(body, o, given), rounds)
    }

    /// The type of the elements a `loop` runs over.
    fn element_of(&mut self, body: &mut Body<'a>, o: &Over, given: Ty) -> Ty {
        let defs = self.defs;
        let walked = as_read(&given, &Ty::List(vec![Ty::Unknown]));
        match defs.find(
            defs.iterate,
            &[walked.clone(), Ty::Unknown],
            body.scope,
            &mut Vec::new(),
        ) {
            Found::Yes(all, _) => all[1].clone(),
            Found::Unknown => Ty::Unknown,
            Found::Two(a, b) => {
                self.errors.push(defs.ambiguous(o.value.pos(), a, b));
                Ty::Unknown
            }
            Found::No => {
                self.errors.push(Error::new(
                    o.value.pos(),
                    format!(
                        "there is no implementation of `Iterate` for {}, so a `loop` cannot \
                         run over it",
                        defs.describe(&given, body.scope)
                    ),
                ));
                Ty::Unknown
            }
        }
    }

    /// Checks the two ends of a range, which are whole numbers of one type,
    /// and gives that type, which each number of the range has.
    fn range_type(&mut self, body: &mut Body<'a>, r: &Range) -> Ty {
        let defs = self.defs;
        // Two literals are one number, whose type the uses of the counter
        // decide, as for a literal bound to a name.
        if flexible(&r.from) && flexible(&r.to) {
            match &mut body.numbers {
                Numbers::Off => {}
                Numbers::Finding(places) => {
                    places.push(r.from.pos());
                    let at = places.len() - 1;
                    self.expr(body, &r.from, &Want::Ty(Ty::Unknown));
                    self.expr(body, &r.to, &Want::Ty(Ty::Unknown));
                    return Ty::Flex(at);
                }
                Numbers::Decided(decided) => {
                    let (int, by) = decided[&r.from.pos()];
                    self.deciding = by;
                    self.expr(body, &r.from, &Want::Ty(Ty::Int(int)));
                    // The end is not a number of the range, so it may be the one
                    // above the greatest the counter has, which the type of the
                    // counter does not hold.
                    let end = match &r.to {
                        Expr::Number {
                            magnitude,
                            negative: false,
                            ..
                        } if i128::try_from(*magnitude).ok() == int.max().checked_add(1) => {
                            Int::between(int.min(), int.max() + 1).map_or(int, |wide| wide)
                        }
                        _ => int,
                    };
                    self.expr(body, &r.to, &Want::Ty(Ty::Int(end)));
                    self.deciding = None;
                    body.decided.insert(r.name.clone(), (int, by));
                    return Ty::Int(int);
                }
            }
        }
        let (l, e) = match (flexible(&r.from), flexible(&r.to)) {
            (true, false) => {
                let e = self.expr(body, &r.to, &Want::Any);
                let side = Self::end_want(&e, &r.from);
                (self.expr(body, &r.from, &side), e)
            }
            (false, true) => {
                let l = self.expr(body, &r.from, &Want::Any);
                let side = Self::end_want(&l, &r.to);
                let e = self.expr(body, &r.to, &side);
                (l, e)
            }
            (_, _) => (
                self.expr(body, &r.from, &Want::Any),
                self.expr(body, &r.to, &Want::Any),
            ),
        };
        match (&l, &e) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => Ty::Unknown,
            (Ty::Int(a), Ty::Int(b)) if a == b => l.clone(),
            // Ends of two ranges are each taken as a `Nat`, as a `Nat` is an
            // end beside any of them, where each is shown to fit.
            (Ty::Int(a), Ty::Int(b))
                if (!a.signed() && !b.signed()) || *a == Int::U64 || *b == Int::U64 =>
            {
                let nat = Ty::Int(Int::U64);
                self.expect(body, &r.from, &l, &nat);
                self.expect(body, &r.to, &e, &nat);
                nat
            }
            (Ty::Int(a), Ty::Int(b)) => {
                self.errors.push(Error::new(
                    r.to.pos(),
                    format!(
                        "the two ends of a range need to be of one type, and the first is `{}` \
                         where the second is `{}`",
                        a.name(),
                        b.name()
                    ),
                ));
                Ty::Unknown
            }
            (Ty::Flex(_) | Ty::Int(_), Ty::Flex(_) | Ty::Int(_)) => {
                use_at(r.pos);
                if !flex_pair(&l, &e) {
                    unreachable!("a number and a number")
                }
                if matches!(l, Ty::Flex(_)) && matches!(e, Ty::Flex(_)) {
                    l.clone()
                } else if matches!(l, Ty::Flex(_)) {
                    e.clone()
                } else {
                    l.clone()
                }
            }
            (a, b) => {
                let other = if matches!(a, Ty::Int(_) | Ty::Flex(_)) {
                    b
                } else {
                    a
                };
                let given = defs.describe(other, body.scope);
                self.errors.push(Error::new(
                    r.from.pos(),
                    format!(
                        "a `loop` runs over whole numbers, and an end of this range is {given}"
                    ),
                ));
                Ty::Unknown
            }
        }
    }

    /// Checks `not`, `and`, or `or`, each of whose sides is a `bool`, and
    /// which gives a `bool`.
    fn logic(&mut self, body: &mut Body<'a>, m: &Match) -> Ty {
        let truth = self.defs.bool_ty();
        if matches!(m.form, Form::Not) {
            self.negated.insert(addr(&m.of));
        }
        self.condition(body, &m.of);
        let known = body.facts.mark();
        let blocks: Vec<&[Stmt]> = m.arms.iter().map(|a| a.body.as_slice()).collect();
        let assigned = self.assigned_vars(body, &blocks);
        for (k, arm) in m.arms.iter().enumerate() {
            self.assume(body, &m.of, k == 0);
            if let End::Value(gives, value) = self.block(body, &arm.body, &Want::Ty(truth.clone()))
            {
                self.expect(body, value, &gives, &truth);
            }
            body.facts.reset(&known);
        }
        self.forget(body, &assigned);
        truth
    }

    /// Checks a value that must be a `bool`.
    fn condition(&mut self, body: &mut Body<'a>, value: &Expr) {
        let truth = self.defs.bool_ty();
        let gives = self.expr(body, value, &Want::Ty(truth.clone()));
        self.expect(body, value, &gives, &truth);
    }

    /// Reports a value that a statement gives and nothing uses, when it is not
    /// `()`: a `Result` says that a failure would be dropped.
    fn dropped(&mut self, body: &Body, value: &Expr, gives: &Ty) {
        // A value that cannot be dropped is reported where it is dropped, as
        // ownership says.
        if matches!(gives, Ty::Nothing)
            || gives.unknown()
            || !ownership::discards(self.defs, body.scope, gives)
        {
            return;
        }
        let described = format!("a `{}`", self.defs.text(gives, body.scope));
        let (message, fix) = if self.defs.is_result(gives) {
            (
                format!(
                    "this gives {described}, which nothing uses, so a failure would be dropped"
                ),
                "write `try` before it, match on it, or write `_ =` before it to throw it away",
            )
        } else {
            (
                format!("this gives {described}, which nothing uses"),
                "bind it to a name, or write `_ =` before it to throw it away",
            )
        };
        self.errors
            .push(Error::new(value.pos(), message).with_fix(fix));
    }

    /// Checks what the block of an arm gives, where the `match` or the `if`
    /// is used as an expression, against what is wanted of it or what the
    /// first block gives, and gives it.
    fn arm_gives(
        &mut self,
        body: &mut Body<'a>,
        arm: &Arm,
        end: End,
        want: &Want,
        first: &Option<Ty>,
        what: &str,
    ) -> Ty {
        let defs = self.defs;
        let needed = match (want, first) {
            (Want::Ty(t), _) => Some(t),
            (_, Some(t)) => Some(t),
            _ => None,
        };
        let describe = |t: &Ty| defs.describe(t, body.scope);
        if proof::ends_in_exit(&arm.body) {
            return Ty::Unknown;
        }
        let (gives, at) = match &end {
            End::Value(gives, value) => (gives.clone(), value.pos()),
            End::Empty => (Ty::Nothing, arm.end),
            End::Binding(pos) => (Ty::Nothing, *pos),
        };
        let Some(needed) = needed else {
            return gives;
        };
        if agrees(needed, &gives) {
            return gives;
        }
        // A number whose type its uses decide is asked to be the type needed.
        use_at(at);
        if flex_pair(needed, &gives) {
            return gives;
        }
        // A range is taken where a wider one is wanted, with nothing written.
        if let (Want::Ty(Ty::Int(n)), Ty::Int(g), End::Value(_, value)) = (want, &gives, &end) {
            self.widened.insert(addr(*value), *n);
            if !n.holds(*g) {
                self.enters(body, value, (*g, *n));
            }
            return gives;
        }
        let gives_what = match (&end, &gives) {
            (End::Empty, _) => "this block is empty, which gives nothing".to_string(),
            (End::Binding(_), _) => {
                "this block ends with a binding or an assignment, which gives nothing".to_string()
            }
            (End::Value(_, Expr::Call(call)), Ty::Nothing) => {
                format!("`{}` returns nothing", call.callee)
            }
            _ => format!("this gives {}", describe(&gives)),
        };
        let message = match want {
            Want::Ty(t) => format!("{gives_what}, where {} is needed", describe(t)),
            _ => format!(
                "{gives_what}, and the first block gives {}, where the blocks of {what} give \
                 the same type",
                describe(needed)
            ),
        };
        self.errors.push(Error::new(at, message));
        gives
    }

    /// Checks a pattern against the type of what it matches, and gives it
    /// as checked, or `None` if it holds a mistake. Each bare name it
    /// defines goes in `binds`, with the type of what it names and whether
    /// it is a `mut`.
    ///
    /// In a `match`, `assigned` is `None` and a bare name always defines a
    /// name. In a binding, a bare name that is already a `mut` is assigned
    /// and goes in `assigned`, and a case is not built yet.
    fn pattern(
        &mut self,
        body: &Body,
        pattern: &Pattern,
        ty: &Ty,
        binds: &mut Vec<(String, Ty, bool)>,
        mut assigned: Option<&mut Vec<String>>,
    ) -> Option<Pat> {
        let defs = self.defs;
        let quietly = |checker: &mut Self,
                       binds: &mut Vec<(String, Ty, bool)>,
                       assigned: Option<&mut Vec<String>>,
                       inner: &[&Pattern]| {
            let mut assigned = assigned;
            for inner in inner {
                checker.pattern(body, inner, &Ty::Unknown, binds, assigned.as_deref_mut());
            }
            None
        };
        match pattern {
            Pattern::Bind { name, pos, var } => {
                let defined = body.names.iter().position(|(n, _)| n == name);
                let in_pattern = binds.iter().any(|(n, ..)| n == name);
                let is_var = body.vars.contains(name);
                match (defined, &mut assigned) {
                    (Some(at), Some(assigned)) if var.is_none() && !in_pattern => {
                        if !is_var {
                            self.errors.push(not_a_var(body, name, *pos, at));
                            return None;
                        }
                        if assigned.contains(name) {
                            self.errors.push(Error::new(
                                *pos,
                                format!("this pattern names `{name}` twice"),
                            ));
                            return None;
                        }
                        assigned.push(name.clone());
                        let holds = &body.names[at].1;
                        if !agrees(holds, ty) {
                            self.errors.push(Error::new(
                                *pos,
                                format!(
                                    "`{name}` is a `mut` that holds {}, and this pattern gives \
                                     it {}",
                                    defs.describe(holds, body.scope),
                                    defs.describe(ty, body.scope)
                                ),
                            ));
                            return None;
                        }
                        Some(Pat::Any)
                    }
                    _ if defined.is_some() || in_pattern => {
                        // The name stands for what it matches all the same, as
                        // not known, so that nothing it explains is reported.
                        self.errors.push(self.defined_twice(body, name, *pos));
                        binds.push((name.clone(), Ty::Unknown, var.is_some()));
                        None
                    }
                    _ => {
                        binds.push((name.clone(), ty.clone(), var.is_some()));
                        Some(Pat::Any)
                    }
                }
            }
            Pattern::Tuple { pos, items } => {
                let inner: Vec<&Pattern> = items.iter().collect();
                let of = match ty {
                    Ty::Tuple(of) if of.len() == items.len() => of,
                    Ty::Unknown => return quietly(self, binds, assigned, &inner),
                    other => {
                        self.errors.push(Error::new(
                            *pos,
                            format!(
                                "this pattern takes apart a tuple of {}, and it matches {}",
                                items.len(),
                                defs.describe(other, body.scope)
                            ),
                        ));
                        return quietly(self, binds, assigned, &inner);
                    }
                };
                let mut out = Some(Vec::new());
                for (item, t) in items.iter().zip(of) {
                    let item = self.pattern(body, item, t, binds, assigned.as_deref_mut());
                    out = out.zip(item).map(|(mut out, item)| {
                        out.push(item);
                        out
                    });
                }
                out.map(Pat::Tuple)
            }
            Pattern::List { pos, items } => {
                let inner: Vec<&Pattern> = items.iter().collect();
                if assigned.is_some() {
                    self.errors
                        .push(Error::unsupported(*pos, "a list in a binding's pattern"));
                    return quietly(self, binds, assigned, &inner);
                }
                let element = match ty {
                    Ty::List(of) => &of[0],
                    Ty::Unknown => return quietly(self, binds, assigned, &inner),
                    other => {
                        self.errors.push(Error::new(
                            *pos,
                            format!(
                                "this pattern takes apart a list, and it matches {}",
                                defs.describe(other, body.scope)
                            ),
                        ));
                        return quietly(self, binds, assigned, &inner);
                    }
                };
                let mut out = Some(Vec::new());
                for part in items {
                    let before = binds.len();
                    let item = self.pattern(body, part, element, binds, None);
                    // A name holds a copy of its element, as the list keeps
                    // the element.
                    for (name, ty, _) in &binds[before..] {
                        if name != "_"
                            && *ty != Ty::Unknown
                            && !ownership::copies(defs, body.scope, ty)
                        {
                            self.errors.push(Error::new(
                                part.pos(),
                                format!(
                                    "`{name}` would hold {}, which a list keeps and cannot copy",
                                    defs.describe(ty, body.scope)
                                ),
                            ));
                        }
                    }
                    out = out.zip(item).map(|(mut out, item)| {
                        out.push(item);
                        out
                    });
                }
                out.map(Pat::List)
            }
            Pattern::Number {
                text,
                magnitude,
                negative,
                pos,
            } => {
                if assigned.is_some() {
                    self.errors
                        .push(Error::unsupported(*pos, "a literal in a binding's pattern"));
                    return None;
                }
                let literal = Expr::Number {
                    text: text.clone(),
                    magnitude: *magnitude,
                    negative: *negative,
                    pos: *pos,
                };
                match ty {
                    Ty::Unknown => None,
                    Ty::Int(_) | Ty::Flex(_) => {
                        let errors = self.errors.len();
                        self.number(
                            body,
                            &literal,
                            *magnitude,
                            *negative,
                            *pos,
                            &Want::Ty(ty.clone()),
                        );
                        if self.errors.len() > errors {
                            return None;
                        }
                        let value = i128::try_from(*magnitude).ok()?;
                        Some(Pat::Int(if *negative { -value } else { value }))
                    }
                    other => {
                        self.errors.push(Error::new(
                            *pos,
                            format!(
                                "this is a number, and this matches {}",
                                defs.describe(other, body.scope)
                            ),
                        ));
                        None
                    }
                }
            }
            Pattern::Text { text, pos } => {
                if assigned.is_some() {
                    self.errors
                        .push(Error::unsupported(*pos, "a literal in a binding's pattern"));
                    return None;
                }
                match ty {
                    Ty::Unknown => None,
                    Ty::Text => Some(Pat::Text(text.clone())),
                    other => {
                        self.errors.push(Error::new(
                            *pos,
                            format!(
                                "this is a text, and this matches {}",
                                defs.describe(other, body.scope)
                            ),
                        ));
                        None
                    }
                }
            }
            Pattern::Case { name, pos, inner } => {
                let inner_list: Vec<&Pattern> = inner.as_deref().into_iter().collect();
                if assigned.is_some() {
                    self.errors
                        .push(Error::unsupported(*pos, "a case in a binding's pattern"));
                    return quietly(self, binds, assigned, &inner_list);
                }
                let (e, args) = match ty {
                    Ty::Enum(e, args) => (*e, args),
                    Ty::Unknown => return quietly(self, binds, assigned, &inner_list),
                    other => {
                        self.errors.push(Error::new(
                            *pos,
                            format!(
                                "`.{name}` is a case, and this matches {}, which is not an enum",
                                defs.describe(other, body.scope)
                            ),
                        ));
                        return quietly(self, binds, assigned, &inner_list);
                    }
                };
                let Some(k) = defs.case_of(e, name) else {
                    let at = Pos {
                        line: pos.line,
                        col: pos.col + 1,
                    };
                    self.errors.push(Error::new(
                        at,
                        format!("`{}` has no case named `{name}`", defs.file.enums[e].name),
                    ));
                    return quietly(self, binds, assigned, &inner_list);
                };
                match (&defs.cases[e][k], inner) {
                    (Some(holds), Some(inner)) => {
                        let inner = self.pattern(body, inner, &holds.subst(args), binds, None)?;
                        Some(Pat::Case(k, Some(Box::new(inner))))
                    }
                    (Some(holds), None) if holds.subst(args) == Ty::Nothing => {
                        Some(Pat::Case(k, Some(Box::new(Pat::Any))))
                    }
                    (Some(holds), None) => {
                        self.errors.push(
                            Error::new(
                                *pos,
                                format!(
                                    "`.{name}` holds {}, and its pattern gives none for it",
                                    defs.describe(&holds.subst(args), body.scope)
                                ),
                            )
                            .with_fix(format!("write `.{name}(x)` to name what it holds")),
                        );
                        None
                    }
                    (None, Some(inner)) => {
                        self.errors.push(Error::new(
                            inner.pos(),
                            format!("`.{name}` holds nothing, so its pattern takes none"),
                        ));
                        None
                    }
                    (None, None) => Some(Pat::Case(k, None)),
                }
            }
        }
    }

    /// Checks a case of enum `e` written in full with parentheses, as in
    /// `Pet.cat(c)`, which gives the value the case holds by position.
    fn qualified_case(&mut self, body: &mut Body<'a>, call: &Call, e: usize, want: &Want) -> Ty {
        let quiet = Want::Ty(Ty::Unknown);
        let mut ok = true;
        for (i, arg) in call.args.iter().enumerate() {
            if let Some((_, pos)) = &arg.name {
                self.errors.push(Error::new(
                    *pos,
                    "the value a case holds is given by position",
                ));
                ok = false;
            } else if i > 0 {
                self.errors.push(Error::new(
                    arg.value.pos(),
                    "a case holds one value, and this is one more",
                ));
                ok = false;
            }
        }
        if !ok {
            for arg in &call.args {
                self.expr(body, &arg.value, &quiet);
            }
            return Ty::Unknown;
        }
        let payload = call.args.first().map(|a| &a.value);
        let (callee, pos, at) = (&call.callee, call.pos, call.name_pos);
        self.case(body, Some(e), callee, pos, at, payload, want)
    }

    /// Checks the construction of struct `s`, which gives each of its
    /// fields once by name, in any order, except those it leaves to their
    /// defaults.
    fn construction(&mut self, body: &mut Body<'a>, call: &Call, s: usize, want: &Want) -> Ty {
        let defs = self.defs;
        let def = &defs.file.structs[s];
        let vars = &defs.struct_scopes[s];
        let mut sol: Vec<Option<Ty>> = match want {
            Want::Ty(Ty::Struct(t, args)) if *t == s => (args.iter())
                .map(|t| (!t.unknown()).then(|| t.clone()))
                .collect(),
            _ => vec![None; vars.names.len()],
        };
        let mut given = vec![false; def.fields.len()];
        let mut by_position = false;
        if let Some(field) = def.fields.iter().find(|f| hidden(def, f, call.pos)) {
            self.errors.push(
                Error::new(
                    call.pos,
                    format!(
                        "`{}` has the private field `{}`, so only the module that defines it builds one",
                        written_name(&def.name),
                        field.name
                    ),
                )
                .with_fix(format!(
                    "call a `pub` function of that module that gives a `{}`",
                    written_name(&def.name)
                )),
            );
        }
        let before = self.errors.len();
        let mut unknown = false;
        for arg in &call.args {
            if let Expr::Lambda(l) = &arg.value
                && defs.env_of(l).is_none()
            {
                self.errors.push(Error::new(
                    l.pos,
                    "a lambda cannot be stored in a field, as it is only passed to a call",
                ));
                let field = (arg.name.as_ref())
                    .and_then(|(name, _)| def.fields.iter().position(|f| &f.name == name));
                if let Some(i) = field {
                    given[i] = true;
                }
                by_position |= arg.name.is_none() && !arg.after;
                continue;
            }
            let field = (arg.name.as_ref())
                .and_then(|(name, _)| def.fields.iter().position(|f| &f.name == name));
            let wanted = match field {
                Some(i) => match defs.fields[s][i].partial(&sol) {
                    // A type that is not known yet is not one a mistake
                    // already explains.
                    Ty::Unknown if defs.fields[s][i] != Ty::Unknown => Want::Any,
                    t => Want::Ty(t),
                },
                None => Want::Ty(Ty::Unknown),
            };
            let written =
                field.map(|i| (defs.fields[s][i].clone(), written_type(&def.fields[i].ty)));
            let (here, written) = written.unzip();
            let here: Ty = here.unwrap_or(Ty::Unknown);
            let gives = defs.writing(&here, written.clone(), || {
                self.expr(body, &arg.value, &wanted)
            });
            unknown |= gives.unknown();
            match &arg.name {
                None => {
                    if !by_position {
                        let mut error = Error::new(
                            arg.value.pos(),
                            format!("the fields of `{}` are given by name", def.name),
                        );
                        if let Some(at) = given.iter().position(|g| !g) {
                            error = error
                                .with_fix(format!("write `{}: ` before it", def.fields[at].name));
                        }
                        self.errors.push(error);
                    }
                    by_position = true;
                }
                Some((name, pos)) => match def.fields.iter().position(|f| &f.name == name) {
                    Some(i) if given[i] => self.errors.push(Error::new(
                        *pos,
                        format!("the `{name}` field is given twice"),
                    )),
                    Some(i) => {
                        given[i] = true;
                        let pattern = &defs.fields[s][i];
                        self.settled = true;
                        defs.writing(&here, written, || {
                            self.fit(body, &arg.value, &gives, pattern, &mut sol, vars)
                        });
                        self.settled = false;
                        // A lambda that reads names is of the function type the
                        // field is written with.
                        if let (Ty::Struct(e, held), Some(declared)) =
                            (&gives, &defs.field_items[s][i])
                            && let Some(items) = defs.env_items(*e, held)
                        {
                            let wanted: Vec<Ty> = (declared.iter())
                                .map(|t| t.subst(&sol_types(&sol)))
                                .collect();
                            if !wanted.iter().any(Ty::unknown) && wanted != items {
                                let (given, needed) = (Ty::func(items), Ty::func(wanted));
                                let message = self.mismatch(
                                    body,
                                    &arg.value,
                                    defs.describe(&given, body.scope),
                                    defs.describe(&needed, body.scope),
                                );
                                self.errors.push(Error::new(arg.value.pos(), message));
                            }
                        }
                    }
                    None => self.errors.push(Error::new(
                        *pos,
                        format!("`{}` has no field named `{name}`", def.name),
                    )),
                },
            }
        }
        let missing: Vec<&str> = (def.fields.iter())
            .zip(&given)
            .filter(|(f, given)| !**given && f.default.is_none() && f.ty.statement.is_none())
            .map(|(f, _)| f.name.as_str())
            .collect();
        // A field given by position may be one of those missing.
        if !missing.is_empty() && !by_position {
            self.errors.push(Error::new(
                call.end,
                format!("`{}` needs a value for {}", def.name, names(&missing)),
            ));
        }
        if defs.cyclic[s] {
            return Ty::Unknown;
        }
        if self.errors.len() == before && def.fields.iter().any(|f| f.ty.statement.is_some()) {
            self.owe_proof_fields(body, call, s);
        }
        if self.errors.len() == before && self.function_for_parameter(&sol, call.pos) {
            return Ty::Unknown;
        }
        if self.errors.len() == before
            && !unknown
            && let Some(g) = sol.iter().position(Option::is_none)
        {
            self.errors.push(Error::new(
                call.pos,
                format!(
                    "the type `{}` of `{}` is not known here",
                    vars.names[g].trim_end_matches("@function"),
                    def.name
                ),
            ));
        }
        let args = sol.into_iter().map(|t| t.unwrap_or(Ty::Unknown)).collect();
        Ty::structure(s, args)
    }
}

/// The function a construction of the struct `s` is checked as: its fields
/// are its parameters, and the statement of each proof field is a proof
/// parameter with the default `arith`, so that what a construction owes is
/// what a call owes.
fn construction_as_call(defs: &Defs, s: usize) -> (Function, Signature) {
    let def = &defs.file.structs[s];
    let mut params = Vec::new();
    let mut requires = Vec::new();
    for field in &def.fields {
        let proof = field.ty.statement.is_some();
        if let Some(statement) = field.ty.statement.as_deref() {
            requires.push(statement.clone());
        }
        params.push(crate::parser::Param {
            name: field.name.clone(),
            pos: field.pos,
            ty: field.ty.clone(),
            default: proof.then(|| Expr::Name {
                name: "arith".to_string(),
                pos: field.pos,
            }),
            take: false,
            mutable: false,
            takes: Vec::new(),
            effects: Vec::new(),
            by_name: false,
        });
    }
    let function = Function {
        name: written_name(&def.name).to_string(),
        pos: def.pos,
        generics: Vec::new(),
        params,
        result: None,
        result_name: None,
        effects: Vec::new(),
        requires,
        needs: Vec::new(),
        notes: Vec::new(),
        ensures: Vec::new(),
        decreases: Vec::new(),
        body: Vec::new(),
        end: def.pos,
    };
    let signature = Signature {
        scope: defs.struct_scopes[s].clone(),
        params: (def.fields.iter().zip(&defs.fields[s]))
            .map(|(f, t)| (f.name.clone(), t.clone()))
            .collect(),
        written: Vec::new(),
        defaults: def
            .fields
            .iter()
            .map(|f| f.ty.statement.is_some())
            .collect(),
        param_defaults: Vec::new(),
        named_from: None,
        gives: Ty::Nothing,
        gives_param: None,
        gives_held: Vec::new(),
        effects: Vec::new(),
        fn_effects: Vec::new(),
    };
    (function, signature)
}

impl<'a> Checker<'a> {
    /// A construction owes the proof of each proof field of the struct: one
    /// written for it is of the statement with the values given put in, and
    /// one left out is built by `arith` from what is known.
    fn owe_proof_fields(&mut self, body: &mut Body<'a>, call: &Call, s: usize) {
        let defs = self.defs;
        let def = &defs.file.structs[s];
        let Some((function, signature)) = &defs.constructions[s] else {
            return;
        };
        let mut next = 0;
        let mut args = Vec::new();
        for arg in &call.args {
            let place = match &arg.name {
                Some((name, _)) => def.fields.iter().position(|f| &f.name == name),
                None => {
                    next += 1;
                    Some(next - 1)
                }
            };
            if let Some(place) = place {
                args.push((&arg.value, place));
            }
        }
        self.contract_at(
            body,
            (call.pos, addr(call), Some(call)),
            args,
            function,
            signature,
        );
    }
}

/// Whether a type written in a struct names the field `field`: a statement
/// that reads it, a bound of a range, or the type of an element.
fn names_the_field(t: &Type, field: &str) -> bool {
    if let Some(statement) = t.statement.as_deref() {
        let mut all = Vec::new();
        walk(statement, &mut all);
        return all
            .iter()
            .any(|e| matches!(e, Expr::Name { name, .. } if name == field));
    }
    let bound = |b: &Bound| matches!(b.named(), Some(n) if n == field);
    t.range.as_ref().is_some_and(|(a, b)| bound(a) || bound(b))
        || t.name == field
        || t.args.iter().any(|a| names_the_field(a, field))
}

/// Whether a function is a proof: its result is a statement.
fn is_proof(f: &Function) -> bool {
    f.result.as_ref().is_some_and(|t| t.statement.is_some())
}

impl Checker<'_> {
    /// The types of the parameters and of the result of each lambda that leaves
    /// some out, as the look that was made last gave them: each parameter is the
    /// smallest range that holds what the calls of the lambda pass it, joined
    /// before the lambda is checked, and the result is what the lambda gave.
    fn inferred_items(&self) -> Map<usize, Vec<Ty>> {
        let mut items = Map::default();
        for (&s, gave) in &self.gave {
            let Some((l, held)) = self.defs.env_lambda(s) else {
                continue;
            };
            let mut found = held.clone();
            if let Some(calls) = self.seen.get(&s) {
                for (i, slot) in found[..l.params.len()].iter_mut().enumerate() {
                    if l.types[i].is_some() {
                        continue;
                    }
                    let mut joined = Ty::Unknown;
                    for (_, _, passed) in calls {
                        joined = match (joined, &passed[i]) {
                            (Ty::Unknown, t) => t.clone(),
                            (Ty::Int(a), Ty::Int(b)) => Ty::Int(a.hull(*b).unwrap_or(a)),
                            (kept, _) => kept,
                        };
                    }
                    *slot = joined;
                }
            }
            if let Some(last) = found.last_mut() {
                *last = gave.clone();
            }
            items.insert(s, found);
        }
        items
    }
}

/// Finds the types of the names that a lambda reads and its function defines,
/// as the body of the function gives them, for the signature of a function that
/// gives the lambda, which is made before any body is checked. A function whose
/// signature needs the types of another's is checked after it, as often as there
/// are functions.
fn settle_env_types(defs: &mut Defs) {
    let mut owners: Vec<usize> = Vec::new();
    for (s, owner) in defs.env_owner.iter().enumerate() {
        let Some(f) = *owner else { continue };
        let params = &defs.file.functions[f].params;
        let defined = (defs.file.structs[s].fields.iter())
            .any(|field| !params.iter().any(|p| p.name == field.name));
        let inferred = defs
            .env_lambda(s)
            .is_some_and(|(l, _)| leaves_a_type_out(l));
        if (defined || inferred) && !owners.contains(&f) {
            owners.push(f);
        }
    }
    // The types a lambda that leaves some out is given by the calls of it are
    // found a look after the types of what the calls pass, as often again as
    // a call passes what another call gave.
    for round in 0.. {
        let mut found: Map<usize, Vec<Ty>> = Map::default();
        let mut inferred: Map<usize, Vec<Ty>> = Map::default();
        let mut calls: Map<usize, Vec<(Pos, String)>> = Map::default();
        for &f in &owners {
            let mut probe = Checker::new(defs, Vec::new());
            probe.probing = true;
            probe.body(Template::Free(f));
            inferred.extend(probe.inferred_items());
            for (s, seen) in &probe.seen {
                calls.insert(*s, seen.iter().map(|(p, t, _)| (*p, t.clone())).collect());
            }
            found.extend(probe.probed);
        }
        defs.env_calls = calls;
        let same = (found.iter()).all(|(s, types)| defs.env_types.get(s) == Some(types))
            && (inferred.iter())
                .all(|(s, items)| defs.env_lambda(*s).is_some_and(|(_, held)| held == items));
        if same {
            break;
        }
        // Parameters that grow with each look are of calls that pass what other
        // calls of the same lambda give, and have no range that holds them.
        if round >= owners.len() + 8 {
            defs.env_unsettled.extend(inferred.keys().copied());
            break;
        }
        defs.env_types.extend(found);
        for (s, items) in inferred {
            if let Some((_, held)) = defs.env_lambdas[s].as_mut() {
                *held = items;
            }
        }
        for &f in &owners {
            defs.signatures[f] = defs.signature_of(&defs.file.functions[f]);
        }
    }
}

/// The lambda that a struct the parser made for it holds the values of what it
/// reads, and the types its parameters and its result are written with.
type EnvLambda<'a> = Option<(&'a Lambda, Vec<Ty>)>;

/// The lambda that each struct the parser made for one holds the values of what
/// it reads, with the types its parameters and its result are written with.
fn env_lambdas<'a>(defs: &Defs<'a>) -> (Vec<EnvLambda<'a>>, Vec<Option<usize>>) {
    let file = defs.file;
    if !file
        .structs
        .iter()
        .any(|s| s.name.starts_with("env@lambda@"))
    {
        return (
            vec![None; file.structs.len()],
            vec![None; file.structs.len()],
        );
    }
    let mut found: Map<String, (&'a Lambda, usize)> = Map::default();
    for (at, f) in file.functions.iter().enumerate() {
        for stmt in statements(&f.body) {
            let (Stmt::Define { value, .. }
            | Stmt::Destructure { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::Expr(value)) = stmt;
            let mut all = Vec::new();
            walk(value, &mut all);
            for e in all {
                if let Expr::Lambda(l) = e {
                    found.insert(crate::parser::env_name(l.pos), (l, at));
                }
            }
        }
    }
    let lambdas = (file.structs.iter().enumerate())
        .map(|(i, s)| {
            let (l, _) = *found.get(&s.name)?;
            let scope = &defs.struct_scopes[i];
            let mut items: Vec<Ty> = l
                .types
                .iter()
                .map(|t| t.as_ref().map_or(Ty::Unknown, |t| defs.quietly(t, scope)))
                .collect();
            items.push(
                l.result
                    .as_ref()
                    .map_or(Ty::Unknown, |t| defs.quietly(t, scope)),
            );
            Some((l, items))
        })
        .collect();
    let owners = file
        .structs
        .iter()
        .map(|s| found.get(&s.name).map(|(_, f)| *f))
        .collect();
    (lambdas, owners)
}

/// Reports each alias that names itself, directly or through
/// others, with the chain, and marks it, so that a use of it is no type.
fn alias_cycles(defs: &mut Defs, errors: &mut Vec<Error>) {
    let file = defs.file;
    let aliases: Vec<usize> = (file.structs.iter().enumerate())
        .filter(|(_, s)| s.alias.is_some())
        .map(|(i, _)| i)
        .collect();
    // The aliases that the type of `a` names, in the order it writes them.
    let named = |defs: &Defs, a: usize| -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(ty) = &file.structs[a].alias {
            for (name, pos) in type_names(ty) {
                if let Some(Item::Struct(s)) = defs.type_at(pos, name)
                    && file.structs[s].alias.is_some()
                {
                    out.push(s);
                }
            }
        }
        out
    };
    for &a in &aliases {
        if defs.cyclic[a] {
            continue;
        }
        // A path from `a` back to `a`.
        let mut path = vec![a];
        let mut seen = vec![a];
        let mut stack = vec![named(defs, a)];
        let mut found = None;
        while let Some(next) = stack.last_mut() {
            let Some(n) = next.pop() else {
                stack.pop();
                path.pop();
                continue;
            };
            if n == a {
                found = Some(path.clone());
                break;
            }
            if !seen.contains(&n) {
                seen.push(n);
                path.push(n);
                stack.push(named(defs, n));
            }
        }
        if let Some(mut chain) = found {
            chain.push(a);
            let names: Vec<&str> = chain
                .iter()
                .map(|&s| file.structs[s].name.as_str())
                .collect();
            let mut error = Error::new(
                file.structs[a].pos,
                format!(
                    "the alias `{}` names itself: {}",
                    file.structs[a].name,
                    names.join(" -> ")
                ),
            );
            error.fix =
                Some("write a type that holds itself as a `struct` or an `enum`".to_string());
            errors.push(error);
            for &s in &chain {
                defs.cyclic[s] = true;
            }
        }
    }
}

/// Whether a type is a function type, which a name bound to a function with a
/// name takes the type of that function in place of.
fn want_any_fn(t: &Ty) -> bool {
    matches!(t, Ty::Fn(_))
}

/// What is said where two values hold different functions where one type is
/// needed.
pub(crate) const WHICH_FUNCTION: &str = "which function this is depends on what the program does when it \
                              runs, and a function value must be known when the program is built";

/// The types a solution settled, and unknown for the others.
fn sol_types(sol: &[Option<Ty>]) -> Vec<Ty> {
    sol.iter()
        .map(|t| t.clone().unwrap_or(Ty::Unknown))
        .collect()
}

/// The expressions a body can end with: the last of it, and the last of each
/// block of an `if` or a `match` that ends it.
fn tails(body: &[Stmt]) -> Vec<&Expr> {
    fn tail<'e>(body: &'e [Stmt], found: &mut Vec<&'e Expr>) {
        match body.last() {
            Some(Stmt::Expr(Expr::Match(m))) => {
                for arm in &m.arms {
                    tail(&arm.body, found);
                }
            }
            Some(Stmt::Expr(e)) => found.push(e),
            _ => {}
        }
    }
    let mut found = Vec::new();
    tail(body, &mut found);
    found
}

/// The functions an expression holds, when they are known from the source.
#[derive(Clone, Debug, PartialEq)]
enum Holds {
    /// A function, as the type whose one value it is.
    Func(Ty),
    /// A struct that holds functions, with the types that stand for them.
    Struct(usize, Vec<Ty>),
}

/// The place that stands, among what a function holds, for the function that
/// its parameter `i` is, which each call settles from the argument.
fn param_slot(i: usize) -> Ty {
    Ty::Param(PARAM_SLOTS + i)
}

/// The parameter a place stands for, if it is one made by `param_slot`.
fn slot_param(t: &Ty) -> Option<usize> {
    match t {
        Ty::Param(p) if *p >= PARAM_SLOTS => Some(p - PARAM_SLOTS),
        _ => None,
    }
}

/// Where the places of `param_slot` begin, above every type parameter.
const PARAM_SLOTS: usize = 1 << 20;

/// Whether a lambda written with `fn` leaves out the type of a parameter or of
/// its result. A block that takes nothing has no `fn` to write them after.
fn leaves_a_type_out(l: &Lambda) -> bool {
    !l.params.is_empty() && (l.types.iter().any(Option::is_none) || l.result.is_none())
}

/// Where a lambda ends an arm of an `if` or a `match` that is given to a name:
/// a choice between lambdas, which is a choice between functions.
fn lambda_choice(value: &Expr) -> Option<Pos> {
    let Expr::Match(m) = value else { return None };
    m.arms.iter().find_map(|arm| match arm.body.last() {
        Some(Stmt::Expr(Expr::Lambda(l))) => Some(l.pos),
        _ => None,
    })
}
