import { useMonaco } from "@monaco-editor/react";
import { useEffect } from "react";
import type { editor as MonacoEditor } from "monaco-editor/esm/vs/editor/editor.api";
import type { Diagnostic } from "../ex3/types";

export function useDiagnosticMarkers(
  editor: MonacoEditor.IStandaloneCodeEditor | null,
  owner: string,
  diagnostics: Diagnostic[],
): void {
  const monaco = useMonaco();

  useEffect(() => {
    const model = editor?.getModel();
    if (!monaco || !model) return;

    const markers = diagnostics.map((diagnostic) => {
      const line = clamp(diagnostic.line ?? 1, 1, model.getLineCount());
      const column = clamp(diagnostic.column ?? 1, 1, model.getLineMaxColumn(line));
      return {
        severity: monaco.MarkerSeverity.Error,
        message: diagnostic.message,
        startLineNumber: line,
        startColumn: column,
        endLineNumber: line,
        endColumn: Math.min(column + 1, model.getLineMaxColumn(line)),
      };
    });
    monaco.editor.setModelMarkers(model, owner, markers);

    return () => {
      if (!model.isDisposed()) monaco.editor.setModelMarkers(model, owner, []);
    };
  }, [diagnostics, editor, monaco, owner]);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum);
}
