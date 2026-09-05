import type { Diagnostic, Ex3Error } from "../ex3/types";

interface DiagnosticsProps {
  diagnostics: Diagnostic[];
  stage: Ex3Error["stage"] | null;
  message: string | null;
}

export function Diagnostics({ diagnostics, stage, message }: DiagnosticsProps) {
  if (!message && diagnostics.length === 0) return null;
  return (
    <section className="diagnostics" role="alert">
      {stage && <strong className="diagnostic-stage">{stageLabel(stage)}</strong>}
      {message && <p>{message}</p>}
      {diagnostics.length > 0 && (
        <ul>
          {diagnostics.map((diagnostic, index) => (
            <li key={`${diagnostic.line}:${diagnostic.column}:${index}`}>
              {diagnostic.line !== null && (
                <span>
                  {diagnostic.line}:{diagnostic.column ?? "?"}:{" "}
                </span>
              )}
              {diagnostic.message}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function stageLabel(stage: Ex3Error["stage"]): string {
  return `${stage.charAt(0).toUpperCase()}${stage.slice(1)} error`;
}
