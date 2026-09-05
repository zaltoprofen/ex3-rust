import Editor from "@monaco-editor/react";
import "../monaco";

interface AssemblyViewProps {
  assembly: string;
}

export function AssemblyView({ assembly }: AssemblyViewProps) {
  return (
    <section className="panel editor-panel" aria-label="Generated assembly">
      <h2>Assembly</h2>
      <Editor
        height="100%"
        language="plaintext"
        theme="vs-dark"
        value={assembly}
        options={{
          automaticLayout: true,
          fontSize: 14,
          lineNumbers: "on",
          minimap: { enabled: false },
          readOnly: true,
          renderLineHighlight: "none",
          scrollBeyondLastLine: false,
        }}
      />
    </section>
  );
}
