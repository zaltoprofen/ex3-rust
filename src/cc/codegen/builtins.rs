use crate::cc::sema::BuiltinId;

pub(super) struct BackendBuiltin {
    pub assembly_name: &'static str,
    pub needs_runtime: bool,
}

pub(super) fn lookup(id: BuiltinId) -> BackendBuiltin {
    match id {
        BuiltinId::Putchar => BackendBuiltin {
            assembly_name: "__ex3_putchar",
            needs_runtime: true,
        },
        BuiltinId::Getchar => BackendBuiltin {
            assembly_name: "__ex3_getchar",
            needs_runtime: true,
        },
    }
}
