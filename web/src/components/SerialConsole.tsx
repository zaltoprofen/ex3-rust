import { useEffect, useRef } from "react";

interface SerialConsoleProps {
  output: string;
}

export function SerialConsole({ output }: SerialConsoleProps) {
  const outputRef = useRef<HTMLPreElement>(null);
  useEffect(() => {
    const element = outputRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [output]);

  return (
    <section className="panel serial-panel">
      <h2>Serial Output</h2>
      <pre ref={outputRef}>{output || "\u00a0"}</pre>
    </section>
  );
}
