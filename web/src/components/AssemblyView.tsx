import Editor, { type OnMount } from "@monaco-editor/react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { editor as MonacoEditor } from "monaco-editor/esm/vs/editor/editor.api";
import type { AssemblySourceMapRow, Diagnostic } from "../ex3/types";
import "../monaco";
import { useDiagnosticMarkers } from "./useDiagnosticMarkers";

interface AssemblyViewProps {
  assembly: string;
  sourceMap: AssemblySourceMapRow[];
  currentLine: number | null;
  diagnostics: Diagnostic[];
  breakpoints: number[];
  disabled: boolean;
  onToggleBreakpoint(address: number): void;
}

export function AssemblyView({
  assembly,
  sourceMap,
  currentLine,
  diagnostics,
  breakpoints,
  disabled,
  onToggleBreakpoint,
}: AssemblyViewProps) {
  const [editor, setEditor] = useState<MonacoEditor.IStandaloneCodeEditor | null>(null);
  const addressesByLine = useMemo(() => firstAddressForEachLine(sourceMap), [sourceMap]);
  const addressesByLineRef = useRef(addressesByLine);
  const onToggleRef = useRef(onToggleBreakpoint);
  const disabledRef = useRef(disabled);
  addressesByLineRef.current = addressesByLine;
  onToggleRef.current = onToggleBreakpoint;
  disabledRef.current = disabled;
  useDiagnosticMarkers(editor, "ex3-assembler", diagnostics);

  const handleMount: OnMount = (instance, monaco) => {
    setEditor(instance);
    instance.onMouseDown((event) => {
      if (
        disabledRef.current ||
        event.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN
      ) {
        return;
      }
      const line = event.target.position?.lineNumber;
      const address = line === undefined ? undefined : addressesByLineRef.current.get(line);
      if (address !== undefined) onToggleRef.current(address);
    });
  };

  useEffect(() => {
    if (!editor) return;
    const breakpointSet = new Set(breakpoints);
    const decorations: MonacoEditor.IModelDeltaDecoration[] = [];
    if (currentLine !== null) {
      decorations.push({
        range: { startLineNumber: currentLine, startColumn: 1, endLineNumber: currentLine, endColumn: 1 },
        options: { isWholeLine: true, className: "assembly-current-line" },
      });
      editor.revealLineInCenterIfOutsideViewport(currentLine);
    }
    for (const [line, address] of addressesByLine) {
      decorations.push({
        range: { startLineNumber: line, startColumn: 1, endLineNumber: line, endColumn: 1 },
        options: {
          glyphMarginClassName: breakpointSet.has(address)
            ? "assembly-breakpoint"
            : "assembly-executable",
          glyphMarginHoverMessage: {
            value: breakpointSet.has(address)
              ? `Remove breakpoint at 0x${hex16(address)}`
              : `Set breakpoint at 0x${hex16(address)}`,
          },
        },
      });
    }
    const collection = editor.createDecorationsCollection(decorations);
    return () => collection.clear();
  }, [addressesByLine, breakpoints, currentLine, editor]);

  return (
    <section className="panel editor-panel" aria-label="Generated assembly">
      <h2>Assembly</h2>
      <Editor
        height="100%"
        language="plaintext"
        theme="vs-dark"
        value={assembly}
        onMount={handleMount}
        options={{
          automaticLayout: true,
          fontSize: 14,
          glyphMargin: true,
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

function firstAddressForEachLine(sourceMap: AssemblySourceMapRow[]): Map<number, number> {
  const result = new Map<number, number>();
  for (const entry of sourceMap) {
    if (entry.executable && !result.has(entry.line)) result.set(entry.line, entry.address);
  }
  return result;
}

function hex16(value: number): string {
  return (value & 0xffff).toString(16).padStart(4, "0");
}
