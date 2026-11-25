//! `__end`, which ends the program with a status, and `__eprint`, which writes a
//! line to standard error, are the compiler's own: the prelude and the library
//! call them, and a file of a program that does is refused, whatever it passes
//! them.

use smith::check;

use crate::Test;

pub const ONCE: &[Test] = &[(
    "a_file_of_a_program_cannot_end_the_program_or_write_to_standard_error_with_the_calls_of_the_library",
    a_file_of_a_program_cannot_end_the_program_or_write_to_standard_error_with_the_calls_of_the_library,
)];

fn a_file_of_a_program_cannot_end_the_program_or_write_to_standard_error_with_the_calls_of_the_library()
{
    for call in ["__end(1)", "__eprint(\"it ended\")"] {
        let source = format!("fn main() <console> {{\n  {call}\n}}\n");
        let errors = check(source.as_bytes()).err().unwrap_or_default();
        assert!(!errors.is_empty(), "{call} was accepted");
        let name = call.split('(').next().unwrap();
        assert!(
            errors.iter().any(|e| e.message.contains(name)),
            "{call}: {errors:?}"
        );
    }
}
