import type { Diagnostic } from "../ex3/types";

interface DiagnosticsProps {
  diagnostics: Diagnostic[];
  message: string | null;
}

export function Diagnostics({ diagnostics, message }: DiagnosticsProps) {
  if (!message && diagnostics.length === 0) return null;
  return (
    <section className="diagnostics" role="alert">
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
