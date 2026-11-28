//! Lowers a read of a value at the top level of a file, which is worked out
//! when the program is built, as `knowledge/language/structs.md` says.
//!
//! The function that gives the value is lowered as a program of its own and
//! run in the evaluator, once for each program, and a read of it is constants:
//! its numbers, and for a text and a list, the address of bytes among the
//! program's data and the size. A list has room for none, so `push` copies
//! its elements before it writes, and nothing writes the data. An
//! element that holds a text or a list holds the address of a datum of its
//! own, which the target writes where it puts the data.

use super::*;
use crate::scalar::{Scalar, Worked};

/// What a value is: a constant of each of its scalars.
pub(super) type Fixed = Vec<(form::Constant, form::Type)>;

impl<'a> Lowering<'a> {
    /// Lowers a read of the value the instance gives, of the type.
    pub(super) fn read_value(
        &mut self,
        out: &mut Out<'a>,
        instance: &Instance,
        ty: &Ty,
    ) -> (Ty, Vec<form::Value>) {
        let fixed = match self.worked.get(instance) {
            Some(fixed) => fixed.clone(),
            None => {
                let fixed = self.work_out(instance, ty);
                self.worked.insert(instance.clone(), fixed.clone());
                fixed
            }
        };
        let values = (fixed.iter()).map(|(c, t)| out.constant(*c, *t)).collect();
        (ty.clone(), values)
    }

    /// Works out the value, or says why it cannot be, and gives zeros then.
    fn work_out(&mut self, instance: &Instance, ty: &Ty) -> Fixed {
        let pos = self.typed.defs.view(instance.template).f.pos;
        let zeros: Fixed = (self.scalars(ty).into_iter())
            .map(|t| (zero(t), t))
            .collect();
        let program = match lower_with(self.typed, instance.clone(), false) {
            Ok(program) => program,
            Err(errors) => {
                self.errors.extend(errors);
                return zeros;
            }
        };
        let Some(worked) = form::work_out(&program, program.main) else {
            self.errors.push(Error::new(
                pos,
                "this value ended the program that works it out when the program is built",
            ));
            return zeros;
        };
        let mut fixed = Vec::new();
        let mut scalars = worked.scalars.iter().copied();
        match self.fixed(ty, &mut scalars, &worked, &mut fixed) {
            Ok(()) => fixed,
            Err(what) => {
                self.errors.push(Error::unsupported(pos, what));
                zeros
            }
        }
    }

    /// Adds the constants of a value of the type, whose scalars are the next
    /// of `scalars`, or says what is not built of it.
    fn fixed(
        &mut self,
        ty: &Ty,
        scalars: &mut dyn Iterator<Item = Scalar>,
        worked: &Worked,
        into: &mut Fixed,
    ) -> Result<(), &'static str> {
        const SELF: &str = "a value at the top level of a type that holds itself";
        if self.boxed(ty) {
            return Err(SELF);
        }
        let defs = self.typed.defs;
        let word = |scalars: &mut dyn Iterator<Item = Scalar>| match scalars.next() {
            Some(Scalar::Int(n)) => n,
            other => unreachable!("worked out as its scalars: {other:?}"),
        };
        match ty {
            Ty::Text | Ty::Slice(_) | Ty::List(_) => {
                let at = match scalars.next() {
                    Some(Scalar::Address(at)) => at,
                    _ => 0,
                };
                let size = word(scalars);
                if matches!(ty, Ty::List(_)) {
                    word(scalars);
                }
                let data = self.stored(ty, at, size as usize, worked)?;
                let length = form::Type::Int(form::Bits::B64);
                into.push((form::Constant::Data(data), form::Type::Address));
                into.push((form::Constant::Int(size), length));
                // A list has room for none, so a `push` copies it first, and
                // leaves the data as they are, which no program may write.
                if matches!(ty, Ty::List(_)) {
                    into.push((form::Constant::Int(0), length));
                }
            }
            Ty::Int(_) | Ty::Float(_) => {
                for t in self.scalars(ty) {
                    let n = word(scalars);
                    into.push(match t {
                        form::Type::Float(_) => (form::Constant::Float(n), t),
                        form::Type::Int(bits) => (form::Constant::Int(n & mask(bits)), t),
                        _ => unreachable!("a number is integers or a float"),
                    });
                }
            }
            Ty::Struct(s, args) => {
                for f in &defs.fields[*s] {
                    self.fixed(&f.subst(args), scalars, worked, into)?;
                }
            }
            Ty::Tuple(items) => {
                for t in items {
                    self.fixed(t, scalars, worked, into)?;
                }
            }
            Ty::Array(..) | Ty::Matrix(..) => {
                let (item, n) = ty.elements().expect("an array or a matrix");
                for _ in 0..n {
                    self.fixed(item, scalars, worked, into)?;
                }
            }
            Ty::Enum(e, args) => {
                let case = word(scalars);
                into.push((form::Constant::Int(case), form::Type::Int(form::Bits::B8)));
                let layout = self.layout(*e, args);
                let held: Vec<Scalar> = scalars.take(layout.places.len()).collect();
                // The places the case does not use hold the zero of their type.
                let mut places: Fixed = (layout.places.iter()).map(|t| (zero(*t), *t)).collect();
                if let Some(Some(payload)) = defs.cases[*e].get(case as usize) {
                    let used = &layout.cases[case as usize];
                    let mut inner = used.iter().map(|&p| held[p]);
                    let mut payload_fixed = Vec::new();
                    self.fixed(&payload.subst(args), &mut inner, worked, &mut payload_fixed)?;
                    for (&p, c) in used.iter().zip(payload_fixed) {
                        places[p] = c;
                    }
                }
                into.extend(places);
            }
            Ty::Param(_) | Ty::Fn(_) | Ty::Named(_) | Ty::Nothing | Ty::Unknown | Ty::Flex(_) => {
                for _ in self.scalars(ty) {
                    scalars.next();
                }
                if !self.scalars(ty).is_empty() {
                    return Err("a value at the top level of this type");
                }
            }
        }
        Ok(())
    }
}

impl Lowering<'_> {
    /// The place among the program's data of the `size` elements of the text,
    /// slice, or list at `at` in the memory the run left. An element that holds
    /// a text or a list holds the address of a datum of its own, which the
    /// datum says, as the target writes each address where it puts the data.
    fn stored(
        &mut self,
        ty: &Ty,
        at: usize,
        size: usize,
        worked: &Worked,
    ) -> Result<usize, &'static str> {
        let element = self.element(ty);
        let (places, stride) = form::element(&element);
        let stride = stride as usize;
        let bytes = match size * stride {
            0 => Vec::new(),
            count => (worked.bytes(at, count))
                .expect("the bytes of a list worked out")
                .to_vec(),
        };
        let mut datum = form::Datum::from(bytes);
        if let Ty::List(of) | Ty::Slice(of) = ty
            && element.contains(&form::Type::Address)
        {
            for k in 0..size {
                let mut held = (element.iter().zip(&places)).map(|(&t, &p)| {
                    let from = k * stride + p as usize;
                    read(&datum.bytes[from..from + t.bytes() as usize], t)
                });
                let mut fixed = Vec::new();
                self.fixed(&of[0], &mut held, worked, &mut fixed)?;
                for ((c, t), &p) in fixed.into_iter().zip(&places) {
                    let from = k * stride + p as usize;
                    let place = &mut datum.bytes[from..from + t.bytes() as usize];
                    match c {
                        form::Constant::Int(n) | form::Constant::Float(n) => {
                            place.copy_from_slice(&n.to_le_bytes()[..place.len()]);
                        }
                        form::Constant::Vector(low, high) => {
                            let both = u128::from(low) | (u128::from(high) << 64);
                            place.copy_from_slice(&both.to_le_bytes());
                        }
                        form::Constant::Null => place.fill(0),
                        form::Constant::Data(d) => {
                            place.fill(0);
                            datum.addresses.push((from, d));
                        }
                    }
                }
            }
        }
        Ok(self.datum(datum))
    }
}

/// The scalar of the type that the bytes of an element hold.
fn read(bytes: &[u8], t: form::Type) -> Scalar {
    let mut all = [0u8; 16];
    all[..bytes.len()].copy_from_slice(bytes);
    let n = u128::from_le_bytes(all);
    match t {
        form::Type::Address => Scalar::Address(n as usize),
        form::Type::Vector(_) => Scalar::Vector(n),
        form::Type::Int(_) | form::Type::Float(_) => Scalar::Int(n as u64),
    }
}

/// The zero of a scalar type, which a place that holds nothing holds.
fn zero(t: form::Type) -> form::Constant {
    match t {
        form::Type::Int(_) => form::Constant::Int(0),
        form::Type::Float(_) => form::Constant::Float(0),
        form::Type::Address => form::Constant::Null,
        form::Type::Vector(_) => form::Constant::Vector(0, 0),
    }
}

/// The bits an integer of the size holds.
fn mask(bits: form::Bits) -> u64 {
    match bits.count() {
        64 => u64::MAX,
        n => (1 << n) - 1,
    }
}
