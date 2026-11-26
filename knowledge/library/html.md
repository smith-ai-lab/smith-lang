---
type: specification
title: HTML
description: std.html, with which a page is written as calls of plain functions, one for each element, that write the page as they run, with every text escaped and every link one that runs no code, and render, which gives the page as an Html.
status: partial
built:
  - "`std.html` declares the effect `markup`, which a page is written through as it runs, with `text`, which writes a text with `<`, `>`, `&`, `\"`, and `'` written as `&lt;`, `&gt;`, `&amp;`, `&quot;`, and `&#39;` before any handler is given it, and `insert`, which puts an `Html` inside the page being written"
  - "`render`, the handler of `markup`, which keeps the page in a `Text` its operations append to and gives it as an `Html`, whose field is private to `std.html`, and which has `Fmt`, so `print(\"{page}\")` writes the page as its text"
  - "the operations of `markup` are private to `std.html`: a program that imports one, or handles `markup`, is refused, so `render` is the only handler and no function writes a text as HTML"
  - "a tag stays open until something that is not an attribute is written, so an attribute follows the tag it belongs to, and a handler answers `markup` with the operations `start`, `attribute`, `emit`, `include`, `void`, and `end`"
  - "the elements `html`, `head`, `title`, `body`, `h1`, `h2`, `p`, `b`, `span`, `ul`, `li`, `article`, and `a`, each one function that takes its attributes by name, after the `;` of its parameters, and a block as its one parameter by position, as `h1 { text(\"Hi\") }`, with text written only by `text`, and `br` and `input`, which hold nothing and take no block, with attributes by name, each written only when it is given and not empty; every function that writes a page carries `alloc`"
  - "`Link`, which `a` takes as its `href`, made by `link` from a link written out, which is refused where it is written when it holds a space or a control character or has a scheme but `http`, `https`, or `mailto`, and by `check_link` from a text known when the program runs, as a `Result` with a `LinkError`; no element has `src` yet"
  - "not `div`, whose name the prelude holds for division, not `attr`, whose name is a text written in the source and checked when the program is built, which waits on functions over code"
---

# HTML

`std.html` writes HTML. An element is a function, a page is the calls that make it, and nothing of a page is kept as a tree: each call writes its part as it runs.

```
use std.html (render, Html, html, head, title, body, h1, p, text)

fn hello(name: str) -> Html <alloc> {
  render() {
    html {
      head {
        title { text("Hello") }
      }
      body {
        h1 { text("Hello, {name}") }
        p { text("This page is written in Smith.") }
      }
    }
  }
}
```

`hello("Ana")` is the page `<html><head><title>Hello</title></head><body><h1>Hello, Ana</h1><p>This page is written in Smith.</p></body></html>`.

## An element

An element is one function, named for it, that takes its attributes by name and a block by position. It writes its opening tag, runs the block, and writes its closing tag. `body { ... }` writes `<body>`, runs the block, and writes `</body>`, and it reads as the page does: the body, and in it these lines. The block is a lambda, written after the call as [Lambdas](../language/lambdas.md) says, and a call whose only argument is a block has no parentheses. Each line of the block writes what stands inside the element.

Text is not an argument of an element. It is written by `text`, in its place among the children, so text and elements mix in the order they are written.

```
p {
  text("Hello, ")
  b { text(name) }
  text("!")
}
```

This writes `<p>Hello, <b>Ana</b>!</p>` for the name `Ana`. There is one name for each element and one way to write what it holds, so a model has no choice to make between a form for a text and a form for a block, and a text that stands among elements is written as it is anywhere else. `h1("Hi") { text("Hi") }` is refused: "`class` of `h1` is given by name", as the one argument of an element that is given by position is its block, and `h1("Hi")` and `h1()` are refused, as `h1` needs its block.

A block is statements and gives no value, so what an element holds is never the value of its block. An element that holds nothing, as `br` and `input`, takes no block, and `br { }` is refused.

## Attributes

An attribute is a parameter after the `;`, so it is given by name, and the name of one is checked when the program is built.

```
fn p(block: fn() <markup>; class: str = "", id: str = "") <markup, alloc>
fn a(block: fn() <markup>; href: Link, class: str = "", id: str = "") <markup, alloc>
fn input(; name: str = "", placeholder: str = "", type: str = "", value: str = "", class: str = "", id: str = "") <markup, alloc>
```

```
a(href: link("/notes/7"), class: "button") { text("Open") }

input(name: "title", placeholder: "Title")
```

These write `<a href="/notes/7" class="button">Open</a>` and `<input name="title" placeholder="Title">`. Every element has the attributes every element may have, as `class` and `id`, and its own, as `href` on `a`. Each but `href` has a default that writes nothing, so only what is given is written. `a(hreff: link("/")) { text("Open") }` is refused, as `a` has no parameter `hreff`. The compiler does not build `std.html`, so no words are quoted.

An attribute the library has no parameter for, as one that begins `data-`, is written by `attr`, on the first lines of the element's block.

```
li {
  attr("data-id", "7")
  text("First")
}
```

This writes `<li data-id="7">First</li>`.

The value is escaped, as every text is. The name cannot be escaped, since a name that holds a space or a `=` is more markup and no name. So the name is a text written in the source, and it is checked when the program is built: letters, digits, `-`, `_`, `:`, and `.`, and nothing else. `attr("data id", "7")` is refused, and so is a name that is not written out, as `attr(key, "7")`. `attr` is a [function over code](../language/code.md), which is how it reads the name where the call is written. No page is built that holds a name the library did not check.

## A link

`href` takes a `Link`, which holds a text that cannot run code. A link has no space and no control character, and it has no scheme, or its scheme is `http`, `https`, or `mailto`, in any case of its letters. A scheme is what stands before a `:` that comes before any `/`, `?`, or `#`, so `/notes/7`, `?page=2`, and `#top` have none, and `javascript:alert(1)` and `data:text/html,x` have schemes that are refused.

A link is made in one of two ways, and `Link` has no other, as its field is private.

- **`link`**, from a link written out. It is checked where the program is built, and a text that is no link is refused where the call is written, with the text and what is wrong in the message.
- **`check_link`**, from a text known when the program runs, as one the user typed or a store gave. It gives a `Result` whose error is a `LinkError`: `not_plain` or `scheme`, each holding the text, with a [`Fmt`](fmt.md) that writes the text and what is wrong.

```
use std.html (Html, a, check_link, link, p, render, text)

fn home() -> Html <alloc> {
  render() {
    a(href: link("/")) { text("Home") }
  }
}

fn profile(site: str) -> Html <alloc> {
  match check_link(site) {
    .ok(to) { render() { a(href: to) { text("Site") } } }
    .err(e) { render() { p { text("{e}") } } }
  }
}
```

`link("javascript:alert(1)")` is refused where it is written, and `a(href: "/") { text("Open") }` is refused, as a text is not a `Link`. A `site` of `javascript:alert(1)` gives `.err(.scheme(...))`, so the page holds no link that runs code. The value of a link is escaped, as every text is.

## Every text is escaped

A text given to `text` or to an attribute is written so that it is text and nothing else: `<`, `>`, `&`, and a quote are written as HTML writes them. `h1 { text("{name}") }` is safe whatever `name` holds, as `text` escapes the finished text.

No function writes a text as HTML. What is already HTML is an `Html`, which only `render` makes, and `insert` puts one inside another page.

```
fn frame(inside: Html) -> Html <alloc> {
  render() {
    span(class: "frame") {
      insert(inside)
    }
  }
}
```

## A component

A part of a page is a function that carries the effect `markup`, which every element carries. It is called where it belongs, as an element is.

```
// One note of the list.
fn note_card(note: Note) <markup, alloc> {
  article(class: "card") {
    h2 { text(note.title) }
    p { text(note.body) }
    if note.pinned {
      span(class: "badge") { text("Pinned") }
    }
  }
}

fn notes(all: List[Note]) -> Html <alloc> {
  render() {
    ul {
      loop note in all {
        li {
          note_card(note)
        }
      }
    }
  }
}
```

`if`, `loop`, and `match` are the language's own, and work in a page as anywhere, so there is no other language for a page that chooses or repeats.

A component that puts its caller's content somewhere takes it as a block, a lambda that carries `markup`.

```
fn layout(heading: str, inside: fn() <markup>) -> Html <alloc> {
  render() {
    html {
      head {
        title { text(heading) }
      }
      body {
        h1 { text(heading) }
        inside()
      }
    }
  }
}

fn about() -> Html <alloc> {
  layout("About") {
    p { text("Smith is a programming language.") }
  }
}
```

## Where the page goes

Each element writes through the effect `markup`, and `render` is the [handler](../language/handlers.md) that answers it: it runs its block, gathers what the block writes into one text, and gives it as an `Html`. So a function that calls `render` carries no `markup`, and a component, which does not, carries it.

The operations of `markup` are private to `std.html`, as [an operation is unless it is marked `pub`](../language/effect-declarations.md#an-operation-is-private-unless-it-is-marked-pub). A program writes `<markup>` and calls the elements, `text`, `attr`, and `insert`, and nothing else reaches the page, so `render` is the only handler and no function writes a text as HTML.

Nothing is sent or printed while a page is written.

## What can fail

A block gives no value, so nothing fails through one: a [`try`](../language/try.md) in a block has no function to leave. What can fail a page is done before the page is written.

```
fn note_page(id: str) -> Result[Html, str] <alloc, store> {
  note = try load(id)
  .ok(render() {
    h1 { text(note.title) }
    p { text(note.body) }
  })
}
```

A component that meets a failure handles it where it is, and writes what stands in its place.

```
fn note_card(id: str) <markup, alloc, store> {
  match load(id) {
    .ok(note) { h2 { text(note.title) } }
    .err(e) { p(class: "missing") { text("This note is gone.") } }
  }
}
```

So a page is whole or is not made: no call inside `render` leaves it half written.

An `Html` is given to whatever wants a page. A route of an [HTTP server](http.md#what-a-function-gives) that gives one answers with it as a page.

An `Html` has [`Fmt`](fmt.md), which writes the page as its text. So a program prints a page, writes it to a file, or compares it in a test, as it does any value that has `Fmt`.

```
use std.console (console, print)
use std.html (render, p, text)

fn main() <console, alloc> {
  page = render() {
    p { text("a < b") }
  }
  print("{page}")
}
```

This prints `<p>a &lt; b</p>`. `Fmt` reads a page and makes none: the text it gives is a `str`, and a `str` given to an element or to `text` is escaped as any text is. What is inside an `Html` stays private, so `render` remains the only thing that makes one.

## Names

A file imports each element it uses, as any name, or imports the module and writes `html.h1`, as [Qualified names](../language/qualified-names.md) says. An element whose name the file already uses, as `main` in the file that holds the program's `main`, is imported under another name with `as`.

## The forms

| Written | Is |
| --- | --- |
| `body { ... }` | An element, which holds what its block writes |
| `text("Hello")` | A text among other elements |
| `a(href: link("/")) { ... }` | An attribute, as an argument with a name |
| `link("/notes/7")` | A `Link` from a link written out, checked where the program is built |
| `check_link(text)` | A `Result` of a `Link` or a `LinkError` from a text known when the program runs |
| `attr("data-id", "7")` | An attribute the library has no parameter for |
| `fn card(n: Note) <markup, alloc>` | A component |
| `fn layout(inside: fn() <markup>)` | A component that takes its caller's content |
| `render() { ... }` | The page its block writes, as an `Html` |
| `insert(page)` | An `Html` put inside another |
