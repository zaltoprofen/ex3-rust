import init, { Ex3Session } from "../generated/ex3-wasm/ex3_wasm";
import type { Ex3SessionApi } from "./types";

let sessionPromise: Promise<Ex3SessionApi> | null = null;

export function getEx3Session(): Promise<Ex3SessionApi> {
  if (sessionPromise === null) {
    sessionPromise = init().then(() => new Ex3Session());
  }
  return sessionPromise;
}
