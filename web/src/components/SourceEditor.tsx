import Editor from "@monaco-editor/react";
import "../monaco";

interface SourceEditorProps {
  value: string;
  disabled: boolean;
  onChange(value: string): void;
}

export function SourceEditor({ value, disabled, onChange }: SourceEditorProps) {
  return (
    <section className="panel editor-panel" aria-label="C source editor">
      <h2>C Source</h2>
      <Editor
        height="100%"
        language="c"
        theme="vs-dark"
        value={value}
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
