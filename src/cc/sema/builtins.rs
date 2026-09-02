use super::{symbols::FunctionSignature, BuiltinId};
use crate::cc::ast::Type;
use std::collections::HashMap;

pub(super) struct Builtin {
    pub id: BuiltinId,
    pub name: &'static str,
    pub return_type: Type,
    pub parameter_types: &'static [Type],
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        id: BuiltinId::Putchar,
        name: "putchar",
        return_type: Type::Void,
        parameter_types: &[Type::INT],
    },
    Builtin {
        id: BuiltinId::Getchar,
        name: "getchar",
        return_type: Type::INT,
        parameter_types: &[],
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
