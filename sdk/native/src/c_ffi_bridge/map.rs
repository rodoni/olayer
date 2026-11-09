//! C ABI adapters for the Map Core wire facade.

use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

const MAX_MAP_INPUT_BYTES: usize = 1024 * 1024;

fn overlaps(a: usize, a_len: usize, b: usize, b_len: usize) -> bool {
    a_len != 0 && b_len != 0 && a < b.saturating_add(b_len) && b < a.saturating_add(a_len)
}

fn error_number(error: &olayer_map_core::MapError) -> c_int {
    error
        .code
        .as_str()
        .rsplit('-')
        .next()
        .and_then(|suffix| suffix.parse::<c_int>().ok())
        .filter(|code| (1..=12).contains(code))
        .unwrap_or(0)
}

/// Runs a Map Core wire operation into caller-owned storage.
///
/// `error_code` is optional (null is accepted); when present it is set to zero
/// on all statuses other than `-2`, and to the exact Map Core numeric suffix
/// on `-2`. `required_length` is mandatory and is set to the serialized byte
/// count on success or insufficient capacity.
unsafe fn map_wire_call<F>(
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    output_capacity: usize,
    required_length: *mut usize,
    error_code: *mut c_int,
    operation: F,
) -> c_int
where
    F: FnOnce(&[u8]) -> Result<String, olayer_map_core::MapError>,
{
    if required_length.is_null()
        || input_len > MAX_MAP_INPUT_BYTES
        || input_len > isize::MAX as usize
        || output_capacity > isize::MAX as usize
        || (input_len != 0 && input.is_null())
        || (output_capacity != 0 && output.is_null())
    {
        return -1;
    }
    let input_start = input as usize;
    let output_start = output as usize;
    if input_len.checked_add(input_start).is_none()
        || output_capacity.checked_add(output_start).is_none()
        || (input_len != 0
            && output_capacity != 0
            && overlaps(input_start, input_len, output_start, output_capacity))
        || overlaps(
            required_length as usize,
            std::mem::size_of::<usize>(),
            input_start,
            input_len,
        )
        || overlaps(
            required_length as usize,
            std::mem::size_of::<usize>(),
            output_start,
            output_capacity,
        )
        || (!error_code.is_null()
            && (overlaps(
                error_code as usize,
                std::mem::size_of::<c_int>(),
                input_start,
                input_len,
            ) || overlaps(
                error_code as usize,
                std::mem::size_of::<c_int>(),
                output_start,
                output_capacity,
            ) || overlaps(
                error_code as usize,
                std::mem::size_of::<c_int>(),
                required_length as usize,
                std::mem::size_of::<usize>(),
            )))
    {
        return -1;
    }
    if !(required_length as usize).is_multiple_of(std::mem::align_of::<usize>())
        || (!error_code.is_null()
            && !(error_code as usize).is_multiple_of(std::mem::align_of::<c_int>()))
    {
        return -1;
    }

    // SAFETY: The ABI contract requires `required_length` and optional
    // `error_code` to be valid, aligned writable locations; overlap checks above
    // guarantee those locations are disjoint from every byte slice and each
    // other, so writes preserve exclusive access.
    unsafe {
        *required_length = 0;
        if !error_code.is_null() {
            *error_code = 0;
        }
    }
    // SAFETY: A non-empty input is required to be non-null, readable and
    // properly aligned for u8 by the ABI contract; the bounded length fits
    // isize::MAX, and overlap checks prohibit mutation through output aliases.
    let bytes = if input_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(input, input_len) }
    };
    let result = catch_unwind(AssertUnwindSafe(|| operation(bytes)));
    let serialized = match result {
        Ok(Ok(serialized)) => serialized,
        Ok(Err(error)) => {
            // SAFETY: Optional error output was checked for alignment and is
            // disjoint from inputs, output, and required_length.
            if !error_code.is_null() {
                unsafe { *error_code = error_number(&error) };
            }
            return -2;
        }
        Err(_) => return -3,
    };
    let needed = serialized.len();
    // SAFETY: required_length was validated above and does not overlap any
    // referenced range; it remains writable for one usize for this call.
    unsafe { *required_length = needed };
    if output.is_null() && output_capacity == 0 {
        return 1;
    }
    if output_capacity < needed {
        return 1;
    }
    // SAFETY: Caller contract guarantees output is writable for capacity bytes;
    // capacity is bounded by isize::MAX, and validated ranges are non-overlap.
    unsafe { ptr::copy_nonoverlapping(serialized.as_ptr(), output, needed) };
    0
}

fn build_request(bytes: &[u8]) -> Result<String, olayer_map_core::MapError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        olayer_map_core::MapError::new(
            olayer_map_core::errors::INVALID_CAPABILITIES,
            "input is not valid UTF-8",
        )
    })?;
    olayer_map_core::interop::build_request_json(text)
}

fn parse_capabilities(bytes: &[u8]) -> Result<String, olayer_map_core::MapError> {
    olayer_map_core::interop::parse_capabilities_json(bytes)
}

fn validate_model(bytes: &[u8]) -> Result<String, olayer_map_core::MapError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        olayer_map_core::MapError::new(
            olayer_map_core::errors::INVALID_CAPABILITIES,
            "input is not valid UTF-8",
        )
    })?;
    olayer_map_core::interop::validate_model_json(text)
}

/// Builds a map request URL from JSON into caller-owned storage.
///
/// Returns 0 on success, 1 if capacity is insufficient (including a null/zero
/// capacity size query), -1 for invalid ABI arguments, -2 for MapError, and -3
/// for a caught panic. Optional `error_code` receives the exact 1..=12 MapError
/// suffix on -2 and is zero otherwise; `required_length` is mandatory.
///
/// # Safety
/// `input` must be readable and aligned for `input_len` bytes when nonempty;
/// `input_len` must not exceed 1 MiB. A non-null `output` must be aligned and
/// writable for `output_capacity` bytes. `required_length` must be non-null,
/// aligned, and writable for one `usize`; non-null `error_code` must be aligned
/// and writable for one `c_int`. Input/output/output locations must not overlap;
/// ranges must fit the address space and remain valid for this call.
#[no_mangle]
pub unsafe extern "C" fn olayer_map_build_request_json(
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    output_capacity: usize,
    required_length: *mut usize,
    error_code: *mut c_int,
) -> c_int {
    // SAFETY: Caller must uphold this API's documented pointer validity,
    // alignment, range, lifetime, and non-overlap contract.
    unsafe {
        map_wire_call(
            input,
            input_len,
            output,
            output_capacity,
            required_length,
            error_code,
            build_request,
        )
    }
}

/// Parses OGC capabilities XML into caller-owned versioned JSON storage.
///
/// Returns 0 on success, 1 if capacity is insufficient (including a null/zero
/// capacity size query), -1 for invalid ABI arguments, -2 for MapError, and -3
/// for a caught panic. Optional `error_code` receives the exact 1..=12 MapError
/// suffix on -2 and is zero otherwise; `required_length` is mandatory.
///
/// # Safety
/// `input` must be readable and aligned for `input_len` bytes when nonempty;
/// `input_len` must not exceed 1 MiB. A non-null `output` must be aligned and
/// writable for `output_capacity` bytes. `required_length` must be non-null,
/// aligned, and writable for one `usize`; non-null `error_code` must be aligned
/// and writable for one `c_int`. Input/output/output locations must not overlap;
/// ranges must fit the address space and remain valid for this call.
#[no_mangle]
pub unsafe extern "C" fn olayer_map_parse_capabilities_json(
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    output_capacity: usize,
    required_length: *mut usize,
    error_code: *mut c_int,
) -> c_int {
    // SAFETY: Caller must uphold this API's documented pointer validity,
    // alignment, range, lifetime, and non-overlap contract.
    unsafe {
        map_wire_call(
            input,
            input_len,
            output,
            output_capacity,
            required_length,
            error_code,
            parse_capabilities,
        )
    }
}

/// Validates a versioned map model JSON document into caller-owned JSON storage.
///
/// Returns 0 on success, 1 if capacity is insufficient (including a null/zero
/// capacity size query), -1 for invalid ABI arguments, -2 for MapError, and -3
/// for a caught panic. Optional `error_code` receives the exact 1..=12 MapError
/// suffix on -2 and is zero otherwise; `required_length` is mandatory.
///
/// # Safety
/// `input` must be readable and aligned for `input_len` bytes when nonempty;
/// `input_len` must not exceed 1 MiB. A non-null `output` must be aligned and
/// writable for `output_capacity` bytes. `required_length` must be non-null,
/// aligned, and writable for one `usize`; non-null `error_code` must be aligned
/// and writable for one `c_int`. Input/output/output locations must not overlap;
/// ranges must fit the address space and remain valid for this call.
#[no_mangle]
pub unsafe extern "C" fn olayer_map_validate_model_json(
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    output_capacity: usize,
    required_length: *mut usize,
    error_code: *mut c_int,
) -> c_int {
    // SAFETY: Caller must uphold this API's documented pointer validity,
    // alignment, range, lifetime, and non-overlap contract.
    unsafe {
        map_wire_call(
            input,
            input_len,
            output,
            output_capacity,
            required_length,
            error_code,
            validate_model,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(input: &[u8], capacity: usize) -> (c_int, usize, c_int, Vec<u8>) {
        let mut output = vec![0xa5; capacity];
        let mut required = usize::MAX;
        let mut code = -1;
        // SAFETY: Test-owned buffers and output locations are valid, aligned,
        // and separate for the duration of the call.
        let status = unsafe {
            olayer_map_validate_model_json(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                capacity,
                &mut required,
                &mut code,
            )
        };
        (status, required, code, output)
    }

    #[test]
    fn valid_exact_short_and_no_partial_write() {
        let input =
            br#"{"version":1,"operation":{"kind":"feature_envelope","epsg":4326,"features":[]}}"#;
        let (query_status, required, _, _) = invoke(input, 0);
        assert_eq!(query_status, 1);
        let (status, exact, code, output) = invoke(input, required);
        assert_eq!((status, exact, code), (0, required, 0));
        assert_eq!(&output, br#"{"version":1,"valid":true}"#);
        let (short_status, _, _, short) = invoke(input, required - 1);
        assert_eq!(short_status, 1);
        assert!(short.iter().all(|byte| *byte == 0xa5));
    }

    #[test]
    fn null_invalid_utf8_and_domain_error_codes() {
        let mut required = 0;
        // SAFETY: Null input with nonzero length is rejected before dereference;
        // required is a valid aligned writable location.
        assert_eq!(
            unsafe {
                olayer_map_validate_model_json(
                    ptr::null(),
                    1,
                    ptr::null_mut(),
                    0,
                    &mut required,
                    ptr::null_mut(),
                )
            },
            -1
        );
        let (status, _, code, _) = invoke(&[0xff], 0);
        assert_eq!(status, -2);
        assert_eq!(code, 12);
        let (status, _, code, _) = invoke(
            br#"{"version":1,"operation":{"kind":"feature_envelope","epsg":999999,"features":[]}}"#,
            0,
        );
        assert_eq!((status, code), (-2, 1));
    }

    #[test]
    fn request_and_capabilities_operations_work() {
        let request = include_bytes!("../../../../tests/fixtures/map-core/wms-4326.json");
        let mut required = 0;
        // SAFETY: Null/zero output is the documented size query, input and
        // required output are disjoint test-owned storage.
        let status = unsafe {
            olayer_map_build_request_json(
                request.as_ptr(),
                request.len(),
                ptr::null_mut(),
                0,
                &mut required,
                ptr::null_mut(),
            )
        };
        assert_eq!(status, 1);
        let mut result = vec![0; required];
        // SAFETY: Test-owned input/output/length storage are valid, aligned and disjoint.
        let status = unsafe {
            olayer_map_build_request_json(
                request.as_ptr(),
                request.len(),
                result.as_mut_ptr(),
                result.len(),
                &mut required,
                ptr::null_mut(),
            )
        };
        assert_eq!(status, 0);
        let result = std::str::from_utf8(&result).unwrap();
        assert!(result.contains("BBOX=20%2C10%2C40%2C30"));
        assert!(result.ends_with("#map\"}"));
        let xml = b"<WMS_Capabilities version=\"1.3.0\"><Service><Title>demo</Title></Service><Capability><Layer><Layer><Name>a</Name></Layer></Layer></Capability></WMS_Capabilities>";
        let mut output = vec![0; 512];
        // SAFETY: Test-owned buffers are valid and disjoint.
        let status = unsafe {
            olayer_map_parse_capabilities_json(
                xml.as_ptr(),
                xml.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut required,
                ptr::null_mut(),
            )
        };
        assert_eq!(status, 0);
        assert!(std::str::from_utf8(&output[..required]).is_ok());
    }
}
