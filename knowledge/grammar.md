---
type: specification
title: Smith grammar
description: Lexical rules, operator precedence, the EBNF grammar, and the properties the parser relies on.
status: partial
built:
  - "`effect_def`: `effect IDENT { fn_sig }`, with `pub` in a module, whose operations are signatures with no body"
  - "`group_def`: `effects IDENT = effects`, with `pub` in a module, and a group in any list of `effects`"
  - "`params`: the parameters of a function before a `;` are given by position or by name and those after it by name only, and a call, a statement, and a function type hold no `;`"
  - "`value_def`: a value at the top level states its type, `IDENT \":\" type \"=\" expr`, and one that does not is refused at its name; the form with a type is not built yet"
---

# Smith grammar

This grammar is the single source of truth for syntax. It is a first draft, and the examples in [language/](language/index.md) are its first test cases.

## Lexical rules

- Source text is UTF-8. Spaces and tabs separate tokens, and a line break is a line feed. A carriage return is an error anywhere in a file, inside a string or a comment too. Any other character that belongs to no token, string, or comment is an error.
- A newline ends a statement, except inside `( )` or `[ ]`, after an operator or comma, or before `|>`, `requires`, `ensures`, `decreases`, `invariant`, and the `{` that opens a function's body, or that opens an `impl`'s after a `requires`, or a `loop`'s after an `invariant`. A block inside `( )` or `[ ]` separates its own statements by newlines as usual.
- Commas appear only inside `( )` and `[ ]`, in a list of effects in `< >`, and between the indices of a `for`. A semicolon appears only in a list of parameters, where it stands once, after the parameters that may be given by position.
- An identifier, `IDENT`, is a letter or `_` followed by letters, digits, and `_`, and is not a keyword, and is not `_` alone, which is a token of its own: the place of a pipeline's value, and the pattern that throws a value away. The letters are `a` to `z` and `A` to `Z`.
- The keywords are `and`, `as`, `break`, `decreases`, `derive`, `effect`, `effects`, `else`, `ensures`, `enum`, `fn`, `for`, `foreign`, `if`, `impl`, `in`, `invariant`, `loop`, `match`, `mut`, `not`, `or`, `out`, `pub`, `requires`, `return`, `struct`, `take`, `trait`, `try`, `use`, `with`, and `without`. The word `type` is a keyword only where an item starts and a name follows it, so it is an identifier anywhere else, as in a parameter named `type`.
- A comment starts with `//`, stands on its own line, and attaches to the definition, parameter, field, case, `requires`, or statement that follows it.
- A whole number is written in decimal, as `1000`, in hexadecimal after `0x`, as `0xff`, or in binary after `0b`, as `0b1010`. A float has a digit on both sides of its point, as `2.5`, and may end with an exponent, as `2.5e3`, and a decimal number with an exponent alone is a float too, as `1e-5`. An `_` may stand between two digits, as in `1_000_000`. There is no octal form and no suffix that names a type.
- A whole number is also written as one character between single quotes, as `'a'`, and it is that character's code. The character is any but `'`, `\`, and a line break, or one of the escapes `\'`, `\\`, `\n`, and `\t`.
- A `-` written where a value may begin, directly before a number, is part of the literal, so `-128` is one literal. After a value, `-` is subtraction.
- Literals keep their source spelling, so `0x10` and `16` stay distinct in the syntax tree.
- A string may hold expressions in braces. The lexer returns the text before a `{`, then ordinary tokens up to the matching `}`, then resumes the string.
- Inside a string, `TEXT` is a run of characters other than `"`, `\`, `{`, `}`, and a line break. `ESCAPE` is a backslash followed by one of `"`, `\`, `{`, `}`, `n`, or `t`. A backslash before anything else, a `}` that closes no expression, and a line break before the closing quote are errors.

## Precedence

Operators bind more tightly further down the table.

| Operators | Associativity |
| --- | --- |
| `\|>` | left |
| `or` | left |
| `and` | left |
| `not` | prefix |
| `==` `!=` `<` `<=` `>` `>=` | none |
| `..` | none |
| `+` `-` | left |
| `*` `/` `%` `@` | left |
| unary `-`, `try` | prefix |
| `^` | right |
| call, index, `.` | left |

`not` covers the whole comparison that follows it, so `not a < b` means `not (a < b)`. The EBNF below gives the forms an expression can take, and this table alone says how its operators group. The `postfix` after a `|>` ends with a `call`, and one `arg` of that call may be `_`, which is where the value before the `|>` goes, as [Pipeline](language/pipeline.md) says.

## EBNF

Items inside braces are separated by newlines under the lexical rules above.

```
file       = { item } ;
item       = use | [ "pub" ] ( fn_def | struct_def | alias_def | enum_def | trait_def | effect_def | group_def | value_def )
           | impl_def
           | derive | without | foreign ;

fn_def     = fn_sig block ;
value_def  = IDENT ":" type "=" expr ;
fn_sig     = "fn" IDENT [ generics ] "(" [ params ] ")" [ "->" result ]
             [ effects ] { contract } ;
result     = [ IDENT ":" ] type | statement ;
statement  = expr ;   (* a comparison, or comparisons joined with `and`, `or`, `not` *)
generics   = "[" generic { "," generic } "]" ;
generic    = [ "out" ] IDENT [ ":" type ] [ "=" ( type | LITERAL ) ] ;   (* a LITERAL is the default of a whole number parameter *)
params     = param { "," param } ( [ "," ] | ";" named )
           | ";" named ;
named      = param { "," param } [ "," ] ;   (* given by name only, as those before the `;` may be given by position or by name *)
param      = ( IDENT | tuple_pat ) ":" [ "take" | "mut" ] type [ "=" expr ] ;   (* the one default of a statement is `arith` *)
tuple_pat  = "(" bind_pat { "," bind_pat } ")" ;
bind_pat   = IDENT | "_" | "mut" IDENT | tuple_pat ;
effects    = "<" IDENT { "," IDENT } [ "," ] ">" ;
contract   = ( "requires" | "ensures" ) expr | "decreases" expr { "," expr } ;

struct_def = "struct" IDENT [ generics ] "{" { [ "pub" ] IDENT ":" type [ "=" expr ] } "}" ;   (* a field whose type is a statement holds a proof of it *)
alias_def  = "type" IDENT [ generics ] "=" type ;
enum_def   = "enum" IDENT [ generics ] "{" { IDENT [ ":" type ] } "}" ;
trait_def  = "trait" IDENT generics "{" { fn_sig } "}" ;
effect_def = "effect" IDENT "{" { fn_sig } "}" ;
group_def  = "effects" IDENT "=" effects ;
impl_def   = "impl" [ generics ] IDENT "[" type { "," type } "]" { "requires" expr }
             "{" { fn_def } "}" ;
derive     = "derive" IDENT "for" IDENT ;
without    = "without" IDENT "for" IDENT ;
use        = "use" IDENT { "." IDENT } [ "as" IDENT ]
             [ "(" use_name { "," use_name } [ "," ] ")" ] ;
use_name   = IDENT [ "as" IDENT ] ;
foreign    = "foreign" STRING fn_sig ;

block      = "{" { stmt } "}" ;
stmt       = define | mut_def | assign | with | "break" | "return" [ expr ] | expr ;
define     = bind_pat [ ":" type ] "=" expr ;
mut_def    = "mut" IDENT [ ":" type ] "=" expr ;
assign     = place ( "=" | "+=" | "-=" | "*=" | "/=" ) expr ;
place      = IDENT { "." IDENT | "[" index { "," index } "]" } ;
with       = "with" [ bind_pat "=" ] expr ;
pattern    = IDENT | "mut" IDENT | "." IDENT [ "(" pattern ")" ]
           | "(" pattern { "," pattern } ")"
           | LITERAL | string | "[" [ pattern { "," pattern } ] "]" ;

expr       = binary { "|>" [ "try" ] postfix } ;
binary     = operand { BINOP operand } ;
operand    = { "not" } unary ;
unary      = [ "-" | "try" ] postfix ;
postfix    = primary { call | "[" index { "," index } "]" | "." IDENT } ;
call       = "(" [ args ] ")" [ lambda ] | lambda ;
args       = arg { "," arg } [ "," ] ;
arg        = [ IDENT ":" ] [ "mut" ] expr ;   (* `mut` before a place a function changes *)
index      = expr | [ expr ] ".." [ expr ] ;
primary    = IDENT | LITERAL | string | "(" [ expr { "," expr } ] ")" | array
           | case | if | match | for | loop | lambda ;
array      = "[" [ expr { "," expr } [ "," ] ] "]" ;

string     = '"' { TEXT | ESCAPE | "{" expr "}" } '"' ;
case       = "." IDENT [ "(" [ args ] ")" ] ;
if         = "if" expr block [ "else" ( block | if ) ] ;
match      = "match" expr "{" { pattern block } "}" ;
for        = "for" IDENT { "," IDENT } block ;
loop       = "loop" [ ( IDENT | tuple_pat ) "in" expr { "invariant" expr } | "decreases" expr { "," expr } ] block ;
lambda     = [ "fn" [ generics ] "(" lambda_arg { "," lambda_arg } ")" [ "->" type ] ] block ;
lambda_arg = ( IDENT | tuple_pat ) [ ":" type ] ;

type       = statement
           | IDENT [ "[" type_arg { "," type_arg } "]" ]
           | bound ".." bound
           | "[" size { "," size } "]" type
           | "(" [ type { "," type } ] ")"
           | "fn" "(" [ fn_arg { "," fn_arg } ] ")" [ "->" type ] [ effects ] ;
bound      = [ "-" ] LITERAL | IDENT ;
fn_arg     = [ "take" | "mut" ] type ;
size       = expr ;
statement  = operand CMP operand ;
type_arg   = type | expr ;
```

## Parsing properties

- **A string that is a pattern holds no expression.** A literal is a pattern only in a `match`.
- **No name lookup.** Parsing never needs to know what a name refers to, so definitions parse independently and in parallel. `x = e` with a bare name is one form to the parser, and the checker decides whether it defines `x` or assigns to a `mut`.
- **One token of lookahead,** with one exception: a statement that starts like a tuple is read as an expression and converted to a pattern when `=` follows. This is still linear time.
- **A `<` after a parameter list or a result opens effects,** since no comparison can stand there, and a comma inside the effects never ends a parameter or a type argument. A function type, as `fn() <console>`, so ends at its `>`, and a parameter, a `,` or `]`, or a `{` can follow it, with one token of lookahead.
- **A `;` ends the parameters that may be given by position.** It stands once in a list of parameters, between two parameters or before the first, and before it a parameter is given by position or by name, and after it by name only, as [Functions](language/functions.md#parameters-by-position-and-by-name) says. It stands in no other place, so a call, a statement, and a function type, which has no names, never hold one.
- **A statement in a type position is a comparison,** such as `value < n`, where `CMP` is one of the comparison operators. It is the type of its proofs.
- **A bound of a range type is a whole number or a name,** as `0..256` or `0..n`, and `a..b` is a type only where a type is expected, as it is an expression elsewhere.
- **`f[n](x)` and `a[i]` are the same postfix form,** and the brackets may hold several expressions separated by commas. The type checker tells a type argument from an index. A `requires` that names a trait, as `requires Convert[A, B]`, is that form too, and the checker tells it from a statement about values.
- **`{` continues an expression in one way.** On the same line, directly after a call's `)`, or after what is called when a lambda is its only argument, it opens a lambda that is the call's last argument. After the condition of an `if`, the value of a `match`, the head of a `loop` or a `for`, or a contract it opens that construct's body instead, so a call there takes a lambda after it only inside parentheses. Anywhere else `{` starts a lambda. Construction is a call, so `{` never starts a literal.
- **An `invariant` is read where the loop's counter and the `mut`s it assigns are in scope.** Each `invariant` is an expression of `bool`, and a loop over a value, which has no counter, names only what is outside it.
- **Files split cheaply.** Top-level items start at column zero, with a keyword or with the name being defined, and only edited definitions are re-parsed.
