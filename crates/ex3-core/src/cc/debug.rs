use super::ScalarType;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionDebugId(usize);

impl FunctionDebugId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compilation {
    pub assembly: String,
    pub debug_info: CompilerDebugInfo,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompilerDebugInfo {
    pub functions: Vec<FunctionDebugSymbols>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDebugSymbols {
    pub id: FunctionDebugId,
    pub name: String,
    pub parameters: Vec<ParameterDebugSymbol>,
    pub locals: Vec<LocalDebugSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterDebugSymbol {
    pub index: usize,
    pub name: String,
    pub ty: ScalarType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDebugSymbol {
    pub slot: usize,
    pub name: String,
    pub ty: ScalarType,
}
