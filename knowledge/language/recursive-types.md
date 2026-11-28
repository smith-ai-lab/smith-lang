---
type: specification
title: Types that contain themselves
description: A type may contain itself, which is how a tree is written, and a graph is a collection plus indices.
status: partial
built:
  - "a struct or an enum that holds itself only through a `List`, directly or through other types, as `struct Node { kids: List[Node] }`, which is accepted where it is declared, and a value of it is built"
  - "a struct or an enum that holds itself with no `List` between, directly or through others, as a field, as what a case holds, in a tuple, or in an `Option`, with `List`s mixed in, which is built, matched on, and walked by a function that calls itself to any depth, as the page's example is"
  - "a type of which no value can be made, as `struct Loop { next: Loop }`, refused at its name with an error that says no value of it can be made"
  - "a type that holds itself as a parameter of a function it holds refused at the name, directly or through a `List` or another type"
  - "`==`, `Repr`, `Fmt`, and `Hash` of a type that holds itself, through a `List` or not, given as to any other struct or enum, by a function that calls itself, so a tree of any depth is compared, written, and hashed"
---

# Types that contain themselves

A type may contain itself, which is how a tree is written. The compiler stores such a field out of line, in the same arena as its parent, and no pointer appears in the source.

```
enum Expr {
  num: f64
  neg: Expr
  add: Pair
}

struct Pair {
  left: Expr
  right: Expr
}

fn eval(e: Expr) -> f64 <alloc> {
  match e {
    .num(v) { v }
    .neg(x) { -eval(x) }
    .add(p) { eval(p.left) + eval(p.right) }
  }
}
```

`Expr` holds an `Expr` in `neg`, and holds one through `Pair` in `add`. `eval` recurses over it, so it carries `alloc` as [Recursion](recursion.md) says.

## Where a type may hold itself

A type holds itself only where it is held: as a field, as what a case holds, or as an element of a list or an array inside it. It may not hold itself as a parameter of a function it holds, so `enum T { wrap: fn(T) -> T }` is refused. [What is trusted](trust.md) says why: such a type lets a computation go on for ever with no recursion to be seen.

## A tree is a value

Values are immutable, so two trees may share a subtree and nothing can observe it. A tree can never contain a cycle.

```
fn twice(x: Expr) -> Expr { .add(Pair(left: x, right: x)) }
```

`twice` holds the same `x` on both sides, and `eval` of the result is twice `eval(x)`.

## Where the nodes live

Nodes are bumped into the [arena](arenas.md) and dropped with it, so there is no per-node allocation, reference count, or collector. No address is observable, so the compiler picks the layout, such as 32-bit offsets within the arena.

A value of a struct or an enum that holds itself with no `List` between is kept as one address, of a node that holds its scalars. The node is asked of `alloc` as a list of one element is, so it is in the arena a list built there would be in. Building the value stores the node, and reading a part of it loads the node. Such a type needs a way out: `struct Loop { next: Loop }` is refused at its name, as no value of it can be made.

## Changing a tree

Changing a tree builds new nodes and leaves the old ones until the arena is dropped.

```
fn negate_all(e: Expr) -> Expr <alloc> {
  match e {
    .num(v) { .num(-v) }
    .neg(x) { x }
    .add(p) { .add(Pair(left: negate_all(p.left), right: negate_all(p.right))) }
  }
}
```

A pass that rewrites a tree many times runs in its own arena and builds its result in the parent's.

```
fn rewritten(e: Expr) -> Expr <alloc> {
  with arena()
  negate_all(negate_all(e))
}
```

## A graph

A value cannot point back at itself, so a graph, or anything else with shared or cyclic links, is a collection plus indices into it.

```
struct Graph {
  labels: List[str]
  next: List[Nat]
}
```

`next[i]` is the index in `labels` of what `labels[i]` links to.

## The forms

| Shape | Written as |
| --- | --- |
| A tree | A type that holds itself, directly or through another |
| Shared or cyclic links | A collection and indices into it |

Next: [Functions over code](code.md)
