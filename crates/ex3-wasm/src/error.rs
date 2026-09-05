use ex3_core::{assembler::AsmErrors, cc::CcErrors, emulator::CpuError};
use serde::Serialize;
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorStage {
    Compiler,
    Assembler,
    Emulator,
    Session,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ex3Error {
    pub stage: ErrorStage,
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl Ex3Error {
    pub fn session(message: impl Into<String>) -> Self {
        Self {
            stage: ErrorStage::Session,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }
}

impl fmt::Display for Ex3Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for Ex3Error {}

impl From<CcErrors> for Ex3Error {
    fn from(errors: CcErrors) -> Self {
        let message = errors.to_string();
        let diagnostics = errors
            .0
            .into_iter()
            .map(|error| Diagnostic {
                line: Some(error.span.line),
                column: Some(error.span.column),
                message: error.message,
            })
            .collect();
        Self {
            stage: ErrorStage::Compiler,
            message,
            diagnostics,
        }
    }
}

impl From<AsmErrors> for Ex3Error {
    fn from(errors: AsmErrors) -> Self {
        let message = errors.to_string();
        let diagnostics = errors
            .0
            .into_iter()
            .map(|error| Diagnostic {
                line: Some(error.span.line),
                column: Some(error.span.column),
                message: error.to_string(),
            })
            .collect();
        Self {
            stage: ErrorStage::Assembler,
            message,
            diagnostics,
        }
    }
}

impl From<CpuError> for Ex3Error {
    fn from(error: CpuError) -> Self {
        Self {
            stage: ErrorStage::Emulator,
            message: error.to_string(),
            diagnostics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ex3_core::{assembler::Assembler, cc, isa::decode};

    #[test]
    fn compiler_errors_keep_structured_locations() {
        let error = Ex3Error::from(cc::compile("int main( {").unwrap_err());
        assert_eq!(error.stage, ErrorStage::Compiler);
        assert!(!error.diagnostics.is_empty());
        assert!(error.diagnostics[0].line.is_some());
        assert!(error.diagnostics[0].column.is_some());
    }

    #[test]
    fn assembler_errors_keep_structured_locations() {
        let error = Ex3Error::from(Assembler::new().assemble("BOGUS\nEND\n").unwrap_err());
        assert_eq!(error.stage, ErrorStage::Assembler);
        assert_eq!(error.diagnostics.len(), 1);
        assert_eq!(error.diagnostics[0].line, Some(1));
        assert_eq!(error.diagnostics[0].column, Some(1));
    }

    #[test]
    fn emulator_errors_do_not_invent_source_diagnostics() {
        let decode_error = decode(0xe000_0000).unwrap_err();
        let error = Ex3Error::from(CpuError::Decode(decode_error));
        assert_eq!(error.stage, ErrorStage::Emulator);
        assert!(error.diagnostics.is_empty());
        assert!(!error.message.is_empty());
    }

    #[test]
    fn session_errors_do_not_have_source_diagnostics() {
        let error = Ex3Error::session("not loaded");
        assert_eq!(error.stage, ErrorStage::Session);
        assert_eq!(error.message, "not loaded");
        assert!(error.diagnostics.is_empty());
    }
}
