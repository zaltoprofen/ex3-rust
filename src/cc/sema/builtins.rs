use super::FunctionSignature;
use crate::cc::ast::Type;
use std::collections::HashMap;

pub(super) struct Builtin {
    pub name: &'static str,
    pub assembly_name: &'static str,
    pub return_type: Type,
    pub parameter_types: &'static [Type],
    pub needs_runtime: bool,
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "putchar",
        assembly_name: "__ex3_putchar",
        return_type: Type::Void,
        parameter_types: &[Type::INT],
        needs_runtime: true,
    },
    Builtin {
        name: "getchar",
        assembly_name: "__ex3_getchar",
        return_type: Type::INT,
        parameter_types: &[],
        needs_runtime: true,
    },
];

pub(super) fn find(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.name == name)
}

pub(super) fn signatures() -> HashMap<String, FunctionSignature> {
    BUILTINS
        .iter()
        .map(|builtin| {
            (
                builtin.name.to_owned(),
                FunctionSignature {
                    ret: builtin.return_type,
                    params: builtin.parameter_types.to_vec(),
                    defined: true,
                },
            )
        })
        .collect()
}
