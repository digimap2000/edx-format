import * as native from "../pkg/bundler/edx_format_wasm.js";
import makeApi from "./api.cjs";
const api = makeApi(native);
export const { EdxError, formatDiagnostic, SCHEMA, CODES, EDX_VERSION, check, loads, dumps, hashes, merge } = api;
export default api;
