//! A call that gives away a name, a field, or an item marks it `take`, as
//! `knowledge/language/take.md` says, and `take` is written nowhere that
//! nothing is given away.

use smith::check;
use smith::diag::Error;

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;

pub const ONCE: &[Test] = &[
    (
        "a_key_split_is_written_take_and_a_split_without_it_is_refused_with_the_call_to_write",
        a_key_split_is_written_take_and_a_split_without_it_is_refused_with_the_call_to_write,
    ),
    (
        "an_argument_a_call_gives_away_is_refused_exactly_when_take_is_not_written_before_it",
        an_argument_a_call_gives_away_is_refused_exactly_when_take_is_not_written_before_it,
    ),
    (
        "take_where_nothing_is_given_away_is_refused_where_it_is_written_saying_why",
        take_where_nothing_is_given_away_is_refused_where_it_is_written_saying_why,
    ),
    (
        "a_payload_made_in_the_call_is_posted_with_no_take",
        a_payload_made_in_the_call_is_posted_with_no_take,
    ),
];

/// The errors a program gives, which are none when it is accepted.
fn errors(source: &str) -> Vec<Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

fn a_key_split_is_written_take_and_a_split_without_it_is_refused_with_the_call_to_write() {
    let program = |arg: &str| {
        format!(
            "use std.bits (u64)\nuse std.int (wrapping_add)\nuse std.random (Key, bits, seeded, split)\n\nfn two(k: take Key) -> u64 {{\n  (a, b) = split({arg})\n  wrapping_add(bits(take a), bits(take b))\n}}\n\nfn main() {{\n  n = two(seeded(1))\n  _ = n\n}}\n"
        )
    };
    let marked = program("take k");
    assert!(errors(&marked).is_empty(), "{:?}\n{marked}", errors(&marked));
    let unmarked = program("k");
    let found = errors(&unmarked);
    assert_eq!(found.len(), 1, "{found:?}\n{unmarked}");
    assert_eq!(found[0].pos, pos_after(&unmarked[..unmarked.find("split(k)").unwrap() + "split(".len()]));
    assert_eq!(found[0].fix.as_deref(), Some("write `split(take k)`"), "{found:?}");
}

/// The type a value is given as: one without `Copy`, a number, and a struct
/// of numbers, which has `Copy`.
const TYPES: &[(&str, &str, bool)] = &[
    ("Key", "Key(secret: 1)", false),
    ("u64", "7", true),
    ("Pt", "Pt(x: 1, y: 2)", true),
];

/// How the argument is reached: a name, a field of a struct, or an item of a
/// list.
#[derive(Clone, Copy, Debug)]
enum Place {
    Name,
    Field,
    Item,
}

#[derive(Debug)]
struct Given {
    ty: usize,
    place: Place,
    marked: bool,
}

fn given(g: &mut Gen) -> Given {
    Given {
        ty: g.below(TYPES.len()),
        place: *g.pick(&[Place::Name, Place::Field, Place::Item]),
        marked: g.one_in(2),
    }
}

const HEAD: &str = "use std.bits (u64)\n\nstruct Key {\n  secret: u64\n}\n\nwithout Copy for Key\n\nstruct Pt {\n  x: u64\n  y: u64\n}\n\n";

/// A program whose `main` gives what `g` says to a parameter marked `take`,
/// and where the argument starts.
fn giving(g: &Given) -> (String, String, usize) {
    let (ty, value, _) = TYPES[g.ty];
    let (made, arg) = match g.place {
        Place::Name => (format!("v: {ty} = {value}"), "v".to_string()),
        Place::Field => (format!("h = Holder(item: {value}, n: 1)"), "h.item".to_string()),
        Place::Item => (format!("vs: List[{ty}] = [{value}]"), "vs[0]".to_string()),
    };
    let mark = if g.marked { "take " } else { "" };
    let mut source = format!(
        "{HEAD}struct Holder {{\n  item: {ty}\n  n: u64\n}}\n\nfn sink(x: take {ty}) -> u64 {{ 1 }}\n\nfn main() <alloc> {{\n  {made}\n  r = sink({mark}"
    );
    let at = source.len();
    source += &format!("{arg})\n  _ = r\n}}\n");
    (source, arg, at)
}

fn an_argument_a_call_gives_away_is_refused_exactly_when_take_is_not_written_before_it() {
    generate::check(1240, 30, given, |g| {
        let (source, arg, at) = giving(g);
        let found = errors(&source);
        let copies = TYPES[g.ty].2;
        match (copies, g.marked) {
            (false, true) | (true, false) => {
                assert!(found.is_empty(), "{found:?}\n{source}");
            }
            (false, false) => {
                assert_eq!(found.len(), 1, "{found:?}\n{source}");
                assert_eq!(found[0].pos, pos_after(&source[..at]), "{source}");
                let fix = format!("write `sink(take {arg})`");
                assert_eq!(found[0].fix.as_deref(), Some(fix.as_str()), "{found:?}\n{source}");
            }
            (true, true) => {
                assert_eq!(found.len(), 1, "{found:?}\n{source}");
                let mark = pos_after(&source[..at - "take ".len()]);
                assert_eq!(found[0].pos, mark, "{source}");
                assert!(found[0].message.contains("`Copy`"), "{}", found[0]);
            }
        }
    });
}

fn take_where_nothing_is_given_away_is_refused_where_it_is_written_saying_why() {
    // What is written before the call, the call with `@` where `take` is,
    // and what the error says of why nothing is given away.
    let cases = [
        ("k = Key(secret: 1)", "look(@k)", "lends it"),
        ("k = Key(secret: 1)", "sink(@Key(secret: 2))", "new value"),
        ("k = Key(secret: 1)", "sink(@make())", "new value"),
        ("n: u64 = 3", "count(@n)", "has `Copy`"),
        ("n: u64 = 3", "count(@4)", "new value"),
    ];
    for (made, call, why) in cases {
        let head = format!(
            "{HEAD}fn look(k: Key) -> u64 {{ k.secret }}\n\nfn sink(k: take Key) -> u64 {{ k.secret }}\n\nfn make() -> Key {{ Key(secret: 5) }}\n\nfn count(n: take u64) -> u64 {{ n }}\n\nfn main() {{\n  {made}\n  r = "
        );
        let at = head.len() + call.find('@').unwrap();
        let source = format!("{head}{}\n  _ = r\n}}\n", call.replace('@', "take "));
        let source = if made.starts_with("k =") && !call.contains("(@k)") {
            source.replace("  _ = r\n", "  _ = r\n  _ = sink(take k)\n")
        } else {
            source
        };
        let found = errors(&source);
        assert_eq!(found.len(), 1, "{found:?}\n{source}");
        assert_eq!(found[0].pos, pos_after(&source[..at]), "{source}");
        assert!(found[0].message.contains(why), "{} does not say {why}\n{source}", found[0]);
        assert_eq!(found[0].fix.as_deref(), Some("take away `take`"), "{found:?}");
    }
}

fn a_payload_made_in_the_call_is_posted_with_no_take() {
    // `post` takes its payload, as the `post` of `std.fetch` does.
    let program = |body: &str| {
        format!(
            "use std.bits (u64)\nuse std.message (Payload, json)\n\nfn post(url: str, payload: take Payload) -> Nat {{ url.size }}\n\nfn send(url: str, item: u64) -> Nat <alloc> {{\n  {body}\n}}\n\nfn main() <alloc> {{\n  n = send(\"a\", 1)\n  _ = n\n}}\n"
        )
    };
    let made = program("post(url, json(item))");
    assert!(errors(&made).is_empty(), "{:?}\n{made}", errors(&made));
    let named = program("p = json(item)\n  post(url, p)");
    let found = errors(&named);
    assert_eq!(found.len(), 1, "{found:?}\n{named}");
    assert_eq!(found[0].fix.as_deref(), Some("write `post(url, take p)`"), "{found:?}");
    let marked = program("p = json(item)\n  post(url, take p)");
    assert!(errors(&marked).is_empty(), "{:?}\n{marked}", errors(&marked));
}
