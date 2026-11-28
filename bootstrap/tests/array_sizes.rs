//! An array whose size is a whole number parameter, `[n]f32` with `n: Nat`, or
//! `+`, `-` and `*` of such sizes and numbers: a call settles the size from an
//! argument or from the type the call must give, the function is built once
//! for each size as if the size were written as a number, and a read, a
//! write, or a size that does not hold for a size is refused.

use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::any;

pub const ONCE: &[Test] = &[
    (
        "product_of_two_arrays_of_one_size_gives_the_products_and_two_sizes_are_refused_at_the_call",
        product_of_two_arrays_of_one_size_gives_the_products_and_two_sizes_are_refused_at_the_call,
    ),
    (
        "a_function_of_a_size_builds_the_bytes_of_the_functions_written_with_each_size_put_in",
        a_function_of_a_size_builds_the_bytes_of_the_functions_written_with_each_size_put_in,
    ),
    (
        "a_size_worked_out_from_others_is_that_number_exactly_and_no_other",
        a_size_worked_out_from_others_is_that_number_exactly_and_no_other,
    ),
    (
        "an_index_proven_below_a_size_that_is_a_name_runs_and_one_that_is_not_is_refused",
        an_index_proven_below_a_size_that_is_a_name_runs_and_one_that_is_not_is_refused,
    ),
];

const PRODUCT: &str = "fn product[n: Nat](a: [n]f32, b: [n]f32) -> [n]f32 { for i { a[i] * b[i] } }\n";

fn stdout(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let out = scalar::run(&program);
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

fn compiles(source: &str) -> bool {
    check(source.as_bytes()).is_ok()
}

/// An array of floats written out, each a whole number below 10.
fn floats(g: &mut Gen, n: usize) -> Vec<u64> {
    (0..n).map(|_| g.below(10) as u64).collect()
}

fn written(xs: &[u64]) -> String {
    let items: Vec<String> = xs.iter().map(|x| format!("{x}.0")).collect();
    format!("[{}]", items.join(", "))
}

/// `product` of the issue, of two arrays of a size, gives their products, and
/// of arrays of two sizes is refused at the call with both sizes.
fn product_of_two_arrays_of_one_size_gives_the_products_and_two_sizes_are_refused_at_the_call() {
    generate::check(
        4901,
        6,
        |g: &mut Gen| {
            let n = 1 + g.below(5);
            let m = 1 + g.below(5);
            (floats(g, n), floats(g, n), m)
        },
        |(a, b, m)| {
            let n = a.len();
            let source = with_header(format!(
                "{PRODUCT}\nfn main() <console, alloc> {{\n  a: [{n}]f32 = {}\n  b: [{n}]f32 = {}\n  print(\"{{product(a, b)}}\")\n}}\n",
                written(a),
                written(b)
            ));
            let products: Vec<u64> = a.iter().zip(b).map(|(x, y)| x * y).collect();
            assert_eq!(stdout(&source), format!("{}\n", written(&products)), "for\n{source}");
            if *m == n {
                return;
            }
            let other: Vec<u64> = (0..*m as u64).collect();
            let source = with_header(format!(
                "{PRODUCT}\nfn main() <console, alloc> {{\n  a: [{n}]f32 = {}\n  b: [{m}]f32 = {}\n  print(\"{{product(a, b)}}\")\n}}\n",
                written(a),
                written(&other)
            ));
            let errors = check(source.as_bytes()).expect_err(&source);
            let call = source.lines().position(|l| l.contains("product(a, b)")).unwrap() + 1;
            assert!(
                errors.iter().any(|e| e.pos.line == call
                    && e.message.contains(&format!("`[{n}]f32`"))
                    && e.message.contains(&format!("`[{m}]f32`"))),
                "{errors:?} for\n{source}"
            );
        },
    );
}

/// A program whose functions take a size, settled from the arguments, from
/// the type the call must give, and written in brackets, is the executable of
/// the program with a function written for each size, its size put in by hand.
fn a_function_of_a_size_builds_the_bytes_of_the_functions_written_with_each_size_put_in() {
    generate::check(4902, 6, |g: &mut Gen| (1 + g.below(5), 1 + g.below(5)), |(k, j)| {
        // Two copies of one size are one function, which by hand is written once.
        if k == j {
            return;
        }
        let ones = |n: usize| written(&vec![1; n]);
        let main = |product: &str, zeros: &str, zeros_k: &str| {
            format!(
                "fn main() <console, alloc> {{\n  a: [{k}]f32 = {}\n  print(\"{{{product}(a, a)}}\")\n  z: [{j}]f32 = {zeros}()\n  print(\"{{z}}\")\n  print(\"{{{zeros_k}()}}\")\n}}\n",
                ones(*k)
            )
        };
        let with = with_header(format!(
            "{PRODUCT}\nfn zeros[n: Nat]() -> [n]f32 {{ for i {{ 0.0 }} }}\n\n{}",
            main("product", "zeros", &format!("zeros[{k}]"))
        ));
        let by_hand = with_header(format!(
            "fn product{k}(a: [{k}]f32, b: [{k}]f32) -> [{k}]f32 {{ for i {{ a[i] * b[i] }} }}\n\n\
             fn zeros{j}() -> [{j}]f32 {{ for i {{ 0.0 }} }}\n\n\
             fn zeros{k}() -> [{k}]f32 {{ for i {{ 0.0 }} }}\n\n{}",
            main(&format!("product{k}"), &format!("zeros{j}"), &format!("zeros{k}"))
        ));
        assert_eq!(
            stdout(&with),
            format!("{}\n{}\n{}\n", ones(*k), written(&vec![0; *j]), written(&vec![0; *k])),
            "for\n{with}"
        );
        let a = compile(with.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{with}"));
        let b = compile(by_hand.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{by_hand}"));
        assert!(a == b, "the executables differ for\n{with}\nand\n{by_hand}");
    });
}

/// A size written as `+`, `-` and `*` of the sizes `n` and `w` and numbers.
#[derive(Clone, Debug)]
enum Size {
    N,
    W,
    Number(i64),
    Add(Box<Size>, Box<Size>),
    Sub(Box<Size>, Box<Size>),
    Mul(Box<Size>, Box<Size>),
}

fn size(g: &mut Gen, depth: usize) -> Size {
    let pair = |g: &mut Gen| (Box::new(size(g, depth - 1)), Box::new(size(g, depth - 1)));
    match g.below(if depth == 0 { 3 } else { 6 }) {
        0 => Size::N,
        1 => Size::W,
        2 => Size::Number(g.below(4) as i64),
        3 => {
            let (x, y) = pair(g);
            Size::Add(x, y)
        }
        4 => {
            let (x, y) = pair(g);
            Size::Sub(x, y)
        }
        _ => {
            let (x, y) = pair(g);
            Size::Mul(x, y)
        }
    }
}

fn size_written(s: &Size) -> String {
    match s {
        Size::N => "n".to_string(),
        Size::W => "w".to_string(),
        Size::Number(k) => k.to_string(),
        Size::Add(x, y) => format!("({} + {})", size_written(x), size_written(y)),
        Size::Sub(x, y) => format!("({} - {})", size_written(x), size_written(y)),
        Size::Mul(x, y) => format!("({} * {})", size_written(x), size_written(y)),
    }
}

fn size_value(s: &Size, n: i64, w: i64) -> i64 {
    match s {
        Size::N => n,
        Size::W => w,
        Size::Number(k) => *k,
        Size::Add(x, y) => size_value(x, n, w) + size_value(y, n, w),
        Size::Sub(x, y) => size_value(x, n, w) - size_value(y, n, w),
        Size::Mul(x, y) => size_value(x, n, w) * size_value(y, n, w),
    }
}

/// A function whose result's size is worked out from the sizes of its
/// arguments gives an array of that number of elements, which is the type of
/// a binding of that size and of no other, and is refused for sizes that make
/// it below zero.
fn a_size_worked_out_from_others_is_that_number_exactly_and_no_other() {
    generate::check(
        4903,
        12,
        |g: &mut Gen| (size(g, 2), g.below(4) as i64, g.below(4) as i64),
        |(s, n, w)| {
            let value = size_value(s, *n, *w);
            // An array of many elements prints many; such a size is not one
            // that tells more.
            if value > 12 {
                return;
            }
            let program = |stated: i64| {
                with_header(format!(
                    "fn widen[n: Nat, w: Nat](a: [n]f32, kernel: [w]f32) -> [{}]f32 {{ for i {{ 1.0 }} }}\n\n\
                     fn main() <console, alloc> {{\n  a: [{n}]f32 = {}\n  k: [{w}]f32 = {}\n  x: [{stated}]f32 = widen(a, k)\n  print(\"{{x}}\")\n}}\n",
                    size_written(s),
                    written(&vec![0; *n as usize]),
                    written(&vec![0; *w as usize])
                ))
            };
            if value < 0 {
                let source = program(0);
                let errors = check(source.as_bytes()).expect_err(&source);
                assert!(
                    errors.iter().any(|e| e.message.contains("below zero")),
                    "{errors:?} for\n{source}"
                );
                return;
            }
            let source = program(value);
            assert_eq!(stdout(&source), format!("{}\n", written(&vec![1; value as usize])), "for\n{source}");
            let other = program(value + 1);
            assert!(!compiles(&other), "for\n{other}");
        },
    );
}

/// A read and a write of an array whose size is a name, at an index `arith`
/// proves below the size, from a number or from the range of a `loop`, give
/// what the same function with the size written as a number gives, and an
/// index not proven below the size is refused.
fn an_index_proven_below_a_size_that_is_a_name_runs_and_one_that_is_not_is_refused() {
    generate::check(4904, 10, |g: &mut Gen| (1 + g.below(5), g.below(7)), |(n, i)| {
        let a = written(&(0..*n as u64).collect::<Vec<u64>>());
        let program = |head: &str, size: &str| {
            with_header(format!(
                "fn bump{head}(a: [{size}]f32) -> [{size}]f32 {{\n  mut b = a\n  b[{i}] = 9.0\n  b\n}}\n\n\
                 fn total{head}(a: [{size}]f32) -> f32 {{\n  mut sum: f32 = 0.0\n  loop k in 0..{size} {{ sum = sum + a[k] }}\n  sum\n}}\n\n\
                 fn main() <console, alloc> {{\n  a: [{n}]f32 = {a}\n  b = bump(a)\n  print(\"{{b[{i}]}} {{total(b)}} {{total(a)}}\")\n}}\n"
            ))
        };
        let named = program("[n: Nat]", "n");
        let numbered = program("", &n.to_string());
        assert_eq!(compiles(&named), i < n, "for\n{named}");
        if i < n {
            assert_eq!(stdout(&named), stdout(&numbered), "for\n{named}");
        } else {
            let errors = check(named.as_bytes()).expect_err(&named);
            assert!(
                errors.iter().any(|e| e.message.contains(&format!("`{i} < {n}`"))),
                "{errors:?} for\n{named}"
            );
        }
    });
}
