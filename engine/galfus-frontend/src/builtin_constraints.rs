#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinConstraintFunctionSignature {
    IteratorNext,
    IterableIter,
    ComparableCompare,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinConstraintFunction {
    pub(crate) name: &'static str,
    pub(crate) signature: BuiltinConstraintFunctionSignature,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinConstraint {
    pub(crate) name: &'static str,
    pub(crate) generic_parameters: &'static [&'static str],
    pub(crate) functions: &'static [BuiltinConstraintFunction],
}

const ITERATOR_FUNCTIONS: &[BuiltinConstraintFunction] = &[BuiltinConstraintFunction {
    name: "next",
    signature: BuiltinConstraintFunctionSignature::IteratorNext,
}];

const ITERABLE_FUNCTIONS: &[BuiltinConstraintFunction] = &[BuiltinConstraintFunction {
    name: "iter",
    signature: BuiltinConstraintFunctionSignature::IterableIter,
}];

const COMPARABLE_FUNCTIONS: &[BuiltinConstraintFunction] = &[BuiltinConstraintFunction {
    name: "compare",
    signature: BuiltinConstraintFunctionSignature::ComparableCompare,
}];

pub(crate) const BUILTIN_CONSTRAINTS: &[BuiltinConstraint] = &[
    BuiltinConstraint {
        name: "Iterator",
        generic_parameters: &["T", "Item"],
        functions: ITERATOR_FUNCTIONS,
    },
    BuiltinConstraint {
        name: "Iterable",
        generic_parameters: &["T", "Item", "Iter"],
        functions: ITERABLE_FUNCTIONS,
    },
    BuiltinConstraint {
        name: "Comparable",
        generic_parameters: &["Pattern", "Value"],
        functions: COMPARABLE_FUNCTIONS,
    },
];

pub(crate) fn builtin_constraint(name: &str) -> Option<&'static BuiltinConstraint> {
    BUILTIN_CONSTRAINTS
        .iter()
        .find(|constraint| constraint.name == name)
}

pub(crate) fn is_builtin_constraint(name: &str) -> bool {
    builtin_constraint(name).is_some()
}
