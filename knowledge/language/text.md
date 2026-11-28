---
type: specification
title: Text and bytes
description: A str as bytes known to be valid UTF-8, which are used as bytes with no call, made from bytes through utf8, and cut into pieces that share its memory where both ends are known to fall between characters.
status: partial
built:
  - "`s.size` and `s[i]` of a `str`, with `arith` proving the index as it does for a `List`"
  - "`loop b in s` over a `str`, which gives each of its bytes, once each, in order"
  - "a `str` passed where a `List[u8]` is a parameter's type, with no call and no copy, and where a generic function lends a `List[T]`, which makes `T` a `u8`, and is refused with one error when `T` is already another type"
  - "`utf8`, which takes a `Slice[u8]` or a text, copies nothing, and gives `.ok` of the text of the same bytes, or `.err` with the place where the first character that is not valid begins, the slice still the caller's; `Utf8Error` holds `at` as a `Nat`; a `List` is frozen first, as `utf8(freeze(take out))`"
  - "`s.bytes` of a `str`, its `Slice[u8]`, with no call and no copy, read only like any slice"
  - "building a `str` by hand is refused"
  - "`s[a..b]`, `s[..k]`, and `s[k..]` of a `str`, a `str` that shares the memory of `s` and has the instructions of `s.bytes[a..b]`, which compiles when `arith` proves `a <= b`, `b <= s.size`, `boundary(s, a)`, and `boundary(s, b)`, and is refused naming the fact it cannot prove"
  - "`boundary(s, i)` in a `requires`, an `ensures`, and an `invariant`, and refused anywhere else, a `check` among them, which `arith` knows at `0` and at `s.size`, at `i` and `i + 1` where `s[i]` or `s.bytes[i]` is known to be below 128, and where a `requires` or an `ensures` says it, of a place it proves equal"
---

# Text and bytes

A `str` is bytes that cannot change, kept together with a proof that they are valid UTF-8. It is an ordinary struct, of a [`Slice`](../library/slice.md) and a proof, in the way [A struct that holds a proof](proof-fields.md) describes.

```
struct str {
  bytes: Slice[Byte]
  valid: is_utf8(bytes)
}
```

So everything a `Slice` is, a text is: it has `Copy`, any number of names share it, and a piece of it shares its memory. And a `str` is used wherever bytes are read, with no call and no copy: its `size` is how many bytes it has, `s[i]` is a byte, and a function that takes a `List[u8]` to read is given a text as it is given a slice.

```
fn count_spaces(s: str) -> Nat {
  mut n: Nat = 0
  loop i in 0..s.size {
    if s[i] == ' ' { n = clamp(n + 1) }
  }
  n
}
```

Bytes become text through `utf8`, which reads them once. It gives the text, which is the same bytes and no copy of them, or the place of the first byte that is not valid. It takes a `Slice`, so the bytes are the caller's still when they are not valid, and bytes a program built in a `List` are frozen first, as `utf8(freeze(take out))`.

```
pub struct Utf8Error {
  pub at: Nat
}

fn utf8(bytes: Slice[Byte]) -> Result[str, Utf8Error]
```

The bytes of a text are its `bytes` field, a `Slice[u8]` that is read like any slice and cannot be assigned to. So a piece of a text is made a text again by `utf8(s.bytes[a..b])`, which copies nothing. The size of `s.bytes` is the size of `s` to `arith`, so a fact about one proves an index or a piece of the other, also through a name given `s.bytes`.

`utf8` of the bytes of `"hi"` is `.ok("hi")`. `utf8` of bytes whose first is 255, which no UTF-8 text holds, is `.err(Utf8Error(at: 0))`.

## A piece of a text

A piece of a text is a text. `s[a..b]` is the bytes from `a` up to `b` as a `str`, which shares the memory of `s`, so nothing is copied and nothing is tested when the program runs: it is the instructions of `s.bytes[a..b]`. It compiles when four facts are known: `a <= b` and `b <= s.size`, which [`arith`](arith.md) proves as it does for [any piece](ranges.md#a-slice-must-be-proven-inside), and `boundary(s, a)` and `boundary(s, b)`, that both ends fall between characters and not inside one.

```
fn before_space(s: str, i: Nat) -> str
  requires i < s.size
{
  if s[i] == ' ' { s[..i] } else { s }
}
```

`before_space("ab cd", 2)` is `"ab"`. `boundary(s, i)` is a fact of the prelude that is proven and never computed, so it is written only in a proof: a `requires`, an `ensures`, or a loop's [`invariant`](loop.md#an-invariant). Anywhere the program computes what it tests, as in an `if` or a [`check`](check.md), it is refused with "`boundary` is a fact for a `requires`, an `ensures`, or an `invariant`, and is never computed". `arith` does not look inside it, and knows it of a place in these ways, and of any place it proves equal to one:

- At `0` and at `s.size`.
- On both sides of a byte below 128: where `s[i] < 128` is known, from an `if`, a `check`, or a `requires`, so are `boundary(s, i)` and `boundary(s, i + 1)`, as such a byte is a character of its own in UTF-8. This is the one way that looks at a byte.
- Where a `requires` asks it, or the `ensures` of a function that was called promises it.

```
fn word(s: str, from: Nat, to: Nat) -> str
  requires from <= to and to <= s.size
  requires boundary(s, from) and boundary(s, to)
{
  s[from..to]
}
```

A piece whose end is not known to fall between characters is refused with the error of an index, which names the fact: "cannot prove that `boundary(s, i)`, which a piece of `s` needs", with the fix "check that the byte is ASCII, or take the position from a function that promises a boundary".

- Nothing else makes a `str` from bytes that are not known to be valid, and no bytes are replaced to make them valid. So `utf8` is the only way in for such bytes, and a list with an invalid byte gives the `.err` above, never a text with the byte changed. A text built from texts is made with [`Text`](../library/text.md#building-a-text), which takes only what is valid and so checks nothing again.
- A range of a text's bytes, as bytes, is taken from its bytes and not from the text.
- Text is not counted or indexed in characters. A character may be several bytes, so `"é".size` is 2, and a program that needs the characters reads the bytes.

## The forms

| Written | Is |
| --- | --- |
| `s.size` | How many bytes `s` has |
| `s[i]` | The byte at `i` |
| `s.bytes` | The `Slice[u8]` of the bytes of `s` |
| `utf8(bytes)` | `.ok(text)` of the same bytes, or `.err` with the place of the first byte that is not valid |
| `s[a..b]` | The bytes from `a` up to `b` as a `str`, sharing the memory of `s`, where both ends are known to fall between characters |
| `boundary(s, i)` | That the place `i` falls between two characters of `s`, a fact that is proven and never computed |

The functions that cut a text and read a character are in [Text functions](../library/text.md) and [UTF-8](../library/utf8.md).

Next: [Ownership](ownership.md)
