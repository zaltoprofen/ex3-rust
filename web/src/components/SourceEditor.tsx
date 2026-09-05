import Editor from "@monaco-editor/react";
import { useState } from "react";
import type { editor as MonacoEditor } from "monaco-editor/esm/vs/editor/editor.api";
import type { Diagnostic } from "../ex3/types";
import "../monaco";
import { useDiagnosticMarkers } from "./useDiagnosticMarkers";

interface SourceEditorProps {
  value: string;
  diagnostics: Diagnostic[];
  disabled: boolean;
  onChange(value: string): void;
}

export function SourceEditor({ value, diagnostics, disabled, onChange }: SourceEditorProps) {
  const [editor, setEditor] = useState<MonacoEditor.IStandaloneCodeEditor | null>(null);
  useDiagnosticMarkers(editor, "ex3-compiler", diagnostics);

  return (
    <section className="panel editor-panel" aria-label="C source editor">
      <h2>C Source</h2>
      <Editor
        height="100%"
        language="c"
        theme="vs-dark"
        value={value}
        onMount={setEditor}
        onChange={(next) => onChange(next ?? "")}
        options={{
          automaticLayout: true,
          fontSize: 14,
          minimap: { enabled: false },
          readOnly: disabled,
          scrollBeyondLastLine: false,
          tabSize: 4,
        }}
      />
    </section>
  );
}
