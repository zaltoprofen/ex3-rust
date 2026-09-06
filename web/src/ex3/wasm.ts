import init, { Ex3Session } from "../generated/ex3-wasm/ex3_wasm";
import type { Ex3SessionApi } from "./types";

let sessionPromise: Promise<Ex3SessionApi> | null = null;

export function getEx3Session(): Promise<Ex3SessionApi> {
  if (sessionPromise === null) {
    // The generated declaration is refreshed by `npm run wasm:build`; keep this
    // boundary typed against the application API for source-only typechecks.
    sessionPromise = init().then(() => new Ex3Session() as unknown as Ex3SessionApi);
  }
  return sessionPromise;
}
