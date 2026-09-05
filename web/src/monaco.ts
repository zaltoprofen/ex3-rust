import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor/esm/vs/editor/editor.api";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import "monaco-editor/esm/vs/basic-languages/cpp/cpp.contribution";

type MonacoGlobal = typeof globalThis & {
  MonacoEnvironment?: {
    getWorker(): Worker;
  };
};

(self as MonacoGlobal).MonacoEnvironment = {
  getWorker: () => new EditorWorker(),
};

loader.config({ monaco });
