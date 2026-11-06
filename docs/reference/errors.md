# Error handling

Core mathematical engines return `Result<T, ModuleError>`. Module errors expose stable `OL-CORE-*` codes; [Map Core](map-core.md) uses `OL-CORE-MAP-*`. Codes must not be renumbered or reused when adding variants. See [the error-code module](../../core/src/error_code.rs) and each module's error implementation.

WASM functions convert failures to `JsValue`, commonly including a coded error string. Browser OGC transport/decoding uses SDK-specific error classes such as `OgcHttpError`, `OgcTimeoutError` and `OgcResponseLimitError`. Callers must handle rejected loads; a failed layer replacement retains installed data where its guide specifies that lifecycle.

C statuses are function-specific. Zero commonly means success and negative values indicate failure, but there is no universal meaning for every negative integer across all functions. Positive status values can carry data quality, such as unknown terrain. Read the [generated header](../../sdk/native/libolayer_native.h) for each function's output and buffer-size contract.

Never treat an error, absent terrain or unavailable prediction as valid zero-valued data. Legacy scalar elevation getters are an explicit compatibility exception; status-aware APIs preserve unknown elevation. Units and status mappings are documented in [binding contracts](bindings.md).
