pub mod lexer;
mod parser;

use smallvec::SmallVec;
use std::ops::Range;

/// A predicate function, used to combine 1 or more predicates
/// into a single value
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub enum Func {
    /// `not()` with a configuration predicate. It is true if its predicate
    /// is false and false if its predicate is true.
    Not,
    /// `all()` with a comma separated list of configuration predicates. It
    /// is false if at least one predicate is false. If there are no predicates,
    /// it is true.
    ///
    /// The associated `usize` is the number of predicates inside the `all()`.
    All(usize),
    /// `any()` with a comma separated list of configuration predicates. It
    /// is true if at least one predicate is true. If there are no predicates,
    /// it is false.
    ///
    /// The associated `usize` is the number of predicates inside the `any()`.
    Any(usize),
}

use crate::targets as targ;

/// All predicates that pertains to a target, except for `target_feature`
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TargetPredicate<'a> {
    /// [target_arch](https://doc.rust-lang.org/reference/conditional-compilation.html#target_arch)
    Arch(targ::Arch<'a>),
    /// [target_endian](https://doc.rust-lang.org/reference/conditional-compilation.html#target_endian)
    Endian(targ::Endian),
    /// [target_env](https://doc.rust-lang.org/reference/conditional-compilation.html#target_env)
    Env(targ::Env<'a>),
    /// [target_family](https://doc.rust-lang.org/reference/conditional-compilation.html#target_family)
    /// This also applies to the bare [`unix` and `windows`](https://doc.rust-lang.org/reference/conditional-compilation.html#unix-and-windows)
    /// predicates.
    Family(targ::Family),
    /// [target_os](https://doc.rust-lang.org/reference/conditional-compilation.html#target_os)
    Os(targ::Os<'a>),
    /// [target_pointer_width](https://doc.rust-lang.org/reference/conditional-compilation.html#target_pointer_width)
    PointerWidth(u8),
    /// [target_vendor](https://doc.rust-lang.org/reference/conditional-compilation.html#target_vendor)
    Vendor(targ::Vendor<'a>),
}

impl<'a> TargetPredicate<'a> {
    /// Returns true of the predicate matches the specified target
    ///
    /// ```
    /// use cfg_expr::{targets::*, expr::TargetPredicate as tp};
    /// let win = get_builtin_target_by_triple("x86_64-pc-windows-msvc").unwrap();
    ///
    /// assert!(
    ///     tp::Arch(Arch::x86_64).matches(win) &&
    ///     tp::Endian(Endian::little).matches(win) &&
    ///     tp::Env(Env::msvc).matches(win) &&
    ///     tp::Family(Family::windows).matches(win) &&
    ///     tp::Os(Os::windows).matches(win) &&
    ///     tp::PointerWidth(64).matches(win) &&
    ///     tp::Vendor(Vendor::pc).matches(win)
    /// );
    /// ```
    pub fn matches(self, target: &targ::TargetInfo<'a>) -> bool {
        use TargetPredicate::*;

        match self {
            Arch(a) => a == target.arch,
            Endian(end) => end == target.endian,
            // The environment is allowed to be an empty string
            Env(env) => match target.env {
                Some(e) => env == e,
                None => env.0 == "",
            },
            Family(fam) => Some(fam) == target.family,
            Os(os) => Some(os) == target.os,
            PointerWidth(w) => w == target.pointer_width,
            Vendor(ven) => Some(ven) == target.vendor,
        }
    }

    #[cfg(feature = "targets")]
    pub fn matches_target(self, target: &target_lexicon::Triple) -> bool {
        use target_lexicon::*;
        use TargetPredicate::*;

        match self {
            Arch(arch) => {
                if arch.0 == "x86" {
                    match target.architecture {
                        Architecture::X86_32(_) => true,
                        _ => false,
                    }
                } else {
                    match arch.0.parse::<Architecture>() {
                        Ok(a) => target.architecture == a,
                        Err(_) => false,
                    }
                }
            }
            Endian(end) => match target.architecture.endianness() {
                Ok(endian) => match (end, endian) {
                    (crate::targets::Endian::little, Endianness::Little) => true,
                    (crate::targets::Endian::big, Endianness::Big) => true,
                    _ => false,
                },

                Err(_) => false,
            },
            Env(env) => {
                if env.0.is_empty() {
                    target.environment == Environment::Unknown
                } else {
                    match env.0.parse::<Environment>() {
                        Ok(e) => {
                            // Rustc shortens multiple "gnu*" environments to just "gnu"
                            if env.0 == "gnu" {
                                match target.environment {
                                    Environment::Gnu
                                    | Environment::Gnuabi64
                                    | Environment::Gnueabi
                                    | Environment::Gnuspe
                                    | Environment::Gnux32 => true,
                                    _ => false,
                                }
                            } else {
                                target.environment == e
                            }
                        }
                        Err(_) => false,
                    }
                }
            }
            Family(fam) => {
                use target_lexicon::OperatingSystem::*;
                Some(fam)
                    == match target.operating_system {
                        Unknown | AmdHsa | Bitrig | Cloudabi | Cuda | Hermit | Nebulet | None_
                        | Uefi | Wasi => None,
                        Darwin
                        | Dragonfly
                        | Emscripten
                        | Freebsd
                        | Fuchsia
                        | Haiku
                        | Ios
                        | L4re
                        | Linux
                        | MacOSX { .. }
                        | Netbsd
                        | Openbsd
                        | Redox
                        | Solaris
                        | VxWorks => Some(crate::targets::Family::unix),
                        Windows => Some(crate::targets::Family::windows),
                        // I really dislike non-exhaustive :(
                        _ => None,
                    }
            }
            Os(os) => match os.0.parse::<OperatingSystem>() {
                Ok(o) => target.operating_system == o,
                Err(_) => {
                    // Handle special case for darwin/macos, where the triple is
                    // "darwin", but rustc identifies the OS as "macos"
                    if os.0 == "macos" && target.operating_system == OperatingSystem::Darwin {
                        true
                    } else {
                        // For android, the os is still linux, but the environment is android
                        os.0 == "android"
                            && target.operating_system == OperatingSystem::Linux
                            && target.environment == Environment::Android
                    }
                }
            },
            Vendor(ven) => match ven.0.parse::<target_lexicon::Vendor>() {
                Ok(v) => target.vendor == v,
                Err(_) => false,
            },
            PointerWidth(pw) => {
                // The gnux32 environment is a special case, where it has an
                // x86_64 architecture, but a 32-bit pointer width
                if target.environment != Environment::Gnux32 {
                    pw == match target.pointer_width() {
                        Ok(pw) => pw.bits(),
                        Err(_) => return false,
                    }
                } else {
                    pw == 32
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Which {
    Arch,
    Endian(targ::Endian),
    Env,
    Family(targ::Family),
    Os,
    PointerWidth(u8),
    Vendor,
}

#[derive(Clone, Debug)]
pub(crate) struct InnerTarget {
    which: Which,
    span: Option<Range<usize>>,
}

/// A single predicate in a `cfg()` expression
#[derive(Debug, PartialEq)]
pub enum Predicate<'a> {
    /// A target predicate, with the `target_` prefix
    Target(TargetPredicate<'a>),
    /// Whether rustc's test harness is [enabled](https://doc.rust-lang.org/reference/conditional-compilation.html#test)
    Test,
    /// [Enabled](https://doc.rust-lang.org/reference/conditional-compilation.html#debug_assertions)
    /// when compiling without optimizations.
    DebugAssertions,
    /// [Enabled](https://doc.rust-lang.org/reference/conditional-compilation.html#proc_macro) for
    /// crates of the proc_macro type.
    ProcMacro,
    /// A [`feature = "<name>"`](https://doc.rust-lang.org/nightly/cargo/reference/features.html)
    Feature(&'a str),
    /// [target_feature](https://doc.rust-lang.org/reference/conditional-compilation.html#target_feature)
    TargetFeature(&'a str),
    /// A generic bare predicate key that doesn't match one of the known options, eg `cfg(bare)`
    Flag(&'a str),
    /// A generic key = "value" predicate that doesn't match one of the known options, eg `cfg(foo = "bar")`
    KeyValue { key: &'a str, val: &'a str },
}

#[derive(Clone, Debug)]
pub(crate) enum InnerPredicate {
    Target(InnerTarget),
    Test,
    DebugAssertions,
    ProcMacro,
    Feature(Range<usize>),
    TargetFeature(Range<usize>),
    Other {
        identifier: Range<usize>,
        value: Option<Range<usize>>,
    },
}

impl InnerPredicate {
    fn to_pred<'a>(&self, s: &'a str) -> Predicate<'a> {
        use InnerPredicate as IP;
        use Predicate::*;

        match self {
            IP::Target(it) => match &it.which {
                Which::Arch => Target(TargetPredicate::Arch(targ::Arch(
                    &s[it.span.clone().unwrap()],
                ))),
                Which::Os => Target(TargetPredicate::Os(targ::Os(&s[it.span.clone().unwrap()]))),
                Which::Vendor => Target(TargetPredicate::Vendor(targ::Vendor(
                    &s[it.span.clone().unwrap()],
                ))),
                Which::Env => Target(TargetPredicate::Env(targ::Env(
                    &s[it.span.clone().unwrap()],
                ))),
                Which::Endian(end) => Target(TargetPredicate::Endian(*end)),
                Which::Family(fam) => Target(TargetPredicate::Family(*fam)),
                Which::PointerWidth(pw) => Target(TargetPredicate::PointerWidth(*pw)),
            },
            IP::Test => Test,
            IP::DebugAssertions => DebugAssertions,
            IP::ProcMacro => ProcMacro,
            IP::Feature(rng) => Feature(&s[rng.clone()]),
            IP::TargetFeature(rng) => TargetFeature(&s[rng.clone()]),
            IP::Other { identifier, value } => match value {
                Some(vs) => KeyValue {
                    key: &s[identifier.clone()],
                    val: &s[vs.clone()],
                },
                None => Flag(&s[identifier.clone()]),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ExprNode {
    Fn(Func),
    Predicate(InnerPredicate),
}

/// A parsed `cfg()` expression that can evaluated
#[derive(Debug)]
pub struct Expression {
    pub(crate) expr: SmallVec<[ExprNode; 5]>,
    // We keep the original string around for providing the arbitrary
    // strings that can make up an expression
    pub(crate) original: String,
}

impl Expression {
    /// An iterator over each predicate in the expression
    pub fn predicates(&self) -> impl Iterator<Item = Predicate<'_>> {
        self.expr.iter().filter_map(move |item| match item {
            ExprNode::Predicate(pred) => {
                let pred = pred.clone().to_pred(&self.original);
                Some(pred)
            }
            _ => None,
        })
    }

    /// Evaluates the expression, using the provided closure to determine the value of
    /// each predicate, which are then combined into a final result depending on the
    /// functions not(), all(), or any() in the expression.
    ///
    /// `eval_predicate` typically returns `bool`, but may return any type that implements
    /// the `Logic` trait.
    ///
    /// ## Examples
    ///
    /// ```
    /// use cfg_expr::{targets::*, Expression, Predicate};
    ///
    /// let linux_musl = get_builtin_target_by_triple("x86_64-unknown-linux-musl").unwrap();
    ///
    /// let expr = Expression::parse(r#"all(not(windows), target_env = "musl", any(target_arch = "x86", target_arch = "x86_64"))"#).unwrap();
    ///
    /// assert!(expr.eval(|pred| {
    ///     match pred {
    ///         Predicate::Target(tp) => tp.matches(linux_musl),
    ///         _ => false,
    ///     }
    /// }));
    /// ```
    ///
    /// Returning `Option<bool>`, where `None` indicates the result is unknown:
    ///
    /// ```
    /// use cfg_expr::{targets::*, Expression, Predicate};
    ///
    /// let expr = Expression::parse(r#"any(target_feature = "sse2", target_env = "musl")"#).unwrap();
    ///
    /// let linux_gnu = get_builtin_target_by_triple("x86_64-unknown-linux-gnu").unwrap();
    /// let linux_musl = get_builtin_target_by_triple("x86_64-unknown-linux-musl").unwrap();
    ///
    /// fn eval(expr: &Expression, target: &TargetInfo) -> Option<bool> {
    ///     expr.eval(|pred| {
    ///         match pred {
    ///             Predicate::Target(tp) => Some(tp.matches(target)),
    ///             Predicate::TargetFeature(_) => None,
    ///             _ => panic!("unexpected predicate"),
    ///         }
    ///     })
    /// }
    ///
    /// // Whether the target feature is present is unknown, so the whole expression evaluates to
    /// // None (unknown).
    /// assert_eq!(eval(&expr, linux_gnu), None);
    ///
    /// // Whether the target feature is present is irrelevant for musl, since the any() always
    /// // evaluates to true.
    /// assert_eq!(eval(&expr, linux_musl), Some(true));
    /// ```
    pub fn eval<EP, T>(&self, mut eval_predicate: EP) -> T
    where
        EP: FnMut(&Predicate<'_>) -> T,
        T: Logic + std::fmt::Debug,
    {
        let mut result_stack = SmallVec::<[T; 8]>::new();

        // We store the expression as postfix, so just evaluate each license
        // requirement in the order it comes, and then combining the previous
        // results according to each operator as it comes
        for node in self.expr.iter() {
            match node {
                ExprNode::Predicate(pred) => {
                    let pred = pred.to_pred(&self.original);

                    result_stack.push(dbg!(eval_predicate(&dbg!(pred))));
                }
                ExprNode::Fn(Func::All(count)) => {
                    // all() with a comma separated list of configuration predicates.
                    let mut result = T::top();

                    for _ in 0..*count {
                        let r = result_stack.pop().unwrap();
                        result = result.and(r);
                    }

                    result_stack.push(result);
                }
                ExprNode::Fn(Func::Any(count)) => {
                    // any() with a comma separated list of configuration predicates.
                    let mut result = T::bottom();

                    for _ in 0..*count {
                        let r = result_stack.pop().unwrap();
                        result = result.or(r);
                    }

                    result_stack.push(result);
                }
                ExprNode::Fn(Func::Not) => {
                    // not() with a configuration predicate.
                    // It is true if its predicate is false
                    // and false if its predicate is true.
                    let r = result_stack.pop().unwrap();
                    result_stack.push(r.not());
                }
            }
        }

        result_stack.pop().unwrap()
    }
}

/// A propositional logic used to evaluate `Expression` instances.
///
/// An `Expression` consists of some predicates and the `any`, `all` and `not` operators. An
/// implementation of `Logic` defines how the `any`, `all` and `not` operators should be evaluated.
pub trait Logic {
    /// The result of an `all` operation with no operands, akin to Boolean `true`.
    fn top() -> Self;

    /// The result of an `any` operation with no operands, akin to Boolean `false`.
    fn bottom() -> Self;

    /// `AND`, which corresponds to the `all` operator.
    fn and(self, other: Self) -> Self;

    /// `OR`, which corresponds to the `any` operator.
    fn or(self, other: Self) -> Self;

    /// `NOT`, which corresponds to the `not` operator.
    fn not(self) -> Self;
}

/// A boolean logic.
impl Logic for bool {
    #[inline]
    fn top() -> Self {
        true
    }

    #[inline]
    fn bottom() -> Self {
        false
    }

    #[inline]
    fn and(self, other: Self) -> Self {
        self && other
    }

    #[inline]
    fn or(self, other: Self) -> Self {
        self || other
    }

    #[inline]
    fn not(self) -> Self {
        !self
    }
}

/// A three-valued logic -- `None` stands for the value being unknown.
///
/// The truth tables for this logic are described on
/// [Wikipedia](https://en.wikipedia.org/wiki/Three-valued_logic#Kleene_and_Priest_logics).
impl Logic for Option<bool> {
    #[inline]
    fn top() -> Self {
        Some(true)
    }

    #[inline]
    fn bottom() -> Self {
        Some(false)
    }

    #[inline]
    fn and(self, other: Self) -> Self {
        match (self, other) {
            // If either is false, the expression is false.
            (Some(false), _) => Some(false),
            (_, Some(false)) => Some(false),
            // If both are true, the expression is true.
            (Some(true), Some(true)) => Some(true),
            // One or both are unknown -- the result is unknown.
            _ => None,
        }
    }

    #[inline]
    fn or(self, other: Self) -> Self {
        match (self, other) {
            // If either is true, the expression is true.
            (Some(true), _) => Some(true),
            (_, Some(true)) => Some(true),
            // If both are false, the expression is false.
            (Some(false), Some(false)) => Some(false),
            // One or both are unknown -- the result is unknown.
            _ => None,
        }
    }

    #[inline]
    fn not(self) -> Self {
        match self {
            Some(v) => Some(!v),
            None => None,
        }
    }
}
