use crate::WhisperError;
use std::ffi::{c_char, CStr};

/// Safely converts a C string pointer to a CStr reference with length limit
///
/// # Safety
/// - Validates pointer is non-null
/// - Ensures null terminator exists within max_len bytes
/// - Pointer must point to valid memory for at least max_len bytes
/// - Pointer must remain valid for the returned lifetime
///
/// # Arguments
/// * `ptr` - Raw C string pointer to validate
/// * `max_len` - Maximum length to search for null terminator
///
/// # Errors
/// - `WhisperError::NullPointer` if ptr is null
/// - `WhisperError::InvalidString` if no null terminator found within max_len
///
/// # Examples
/// ```ignore
/// let text_ptr = whisper_get_text(...);
/// let safe_str = unsafe { c_str_from_ptr_with_limit(text_ptr, 1024)? };
/// ```
pub(crate) unsafe fn c_str_from_ptr_with_limit<'a>(
    ptr: *const c_char,
    max_len: usize,
) -> Result<&'a CStr, WhisperError> {
    if ptr.is_null() {
        return Err(WhisperError::NullPointer);
    }

    // Find null terminator within max_len
    let bytes = std::slice::from_raw_parts(ptr as *const u8, max_len);
    let len = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or(WhisperError::InvalidString)?;

    let bounded = std::slice::from_raw_parts(ptr as *const u8, len + 1);
    CStr::from_bytes_with_nul(bounded).map_err(|_| WhisperError::InvalidString)
}

/// Convert an array of 16 bit mono audio samples to a vector of 32 bit floats.
///
/// # Arguments
/// * `samples` - The array of 16 bit mono audio samples.
/// * `output` - The vector of 32 bit floats to write the converted samples to.
///
/// # Panics
/// * if `samples.len != output.len()`
///
/// # Examples
/// ```
/// # use whisper_rs::convert_integer_to_float_audio;
/// let samples = [0i16; 1024];
/// let mut output = vec![0.0f32; samples.len()];
/// convert_integer_to_float_audio(&samples, &mut output).expect("input and output lengths should be equal");
/// ```
pub fn convert_integer_to_float_audio(
    samples: &[i16],
    output: &mut [f32],
) -> Result<(), WhisperError> {
    if samples.len() != output.len() {
        return Err(WhisperError::InputOutputLengthMismatch {
            input_len: samples.len(),
            output_len: output.len(),
        });
    }

    for (input, output) in samples.iter().zip(output.iter_mut()) {
        *output = *input as f32 / 32768.0;
    }

    Ok(())
}

/// Convert 32-bit floating point stereo PCM audio to 32-bit floating point mono PCM audio.
///
/// # Arguments
/// * `input` - The array of 32-bit floating point stereo PCM audio samples.
/// * `output` - An output place to write all the mono samples.
///
/// # Errors
/// * if `samples.len()` is odd ([`WhisperError::HalfSampleMissing`])
/// * if `input.len() / 2 < samples.len()` ([`WhisperError::InputOutputLengthMismatch`])
///
/// # Returns
/// A vector of 32-bit floating point mono PCM audio samples.
///
/// # Examples
/// ```
/// # use whisper_rs::convert_stereo_to_mono_audio;
/// let samples = [0.0f32; 1024];
/// let mono = convert_stereo_to_mono_audio(&samples).expect("should be no half samples missing");
/// ```
pub fn convert_stereo_to_mono_audio(input: &[f32], output: &mut [f32]) -> Result<(), WhisperError> {
    let (input, []) = input.as_chunks::<2>() else {
        // we only hit this branch if the second binding was not empty
        // or in other words, if input.len() % 2 != 0
        return Err(WhisperError::HalfSampleMissing(input.len()));
    };
    if output.len() != input.len() {
        return Err(WhisperError::InputOutputLengthMismatch {
            input_len: input.len(),
            output_len: output.len(),
        });
    }

    for ([left, right], output) in input.iter().zip(output) {
        *output = (left + right) / 2.0;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::distributions::{Distribution, Standard};
    use rand::Rng;
    use std::hint::black_box;

    extern crate test;

    fn random_sample_data<T>() -> Vec<T>
    where
        Standard: Distribution<T>,
    {
        const SAMPLE_SIZE: usize = 1_048_576;

        let mut rng = rand::thread_rng();
        let mut samples = Vec::with_capacity(SAMPLE_SIZE);
        for _ in 0..SAMPLE_SIZE {
            samples.push(rng.gen::<T>());
        }
        samples
    }

    #[test]
    pub fn assert_stereo_to_mono_success() {
        let samples = random_sample_data::<f32>();
        let mut output = vec![0.0; samples.len() / 2];
        let result = convert_stereo_to_mono_audio(&samples, &mut output);
        assert!(result.is_ok());
    }

    #[test]
    pub fn assert_stereo_to_mono_err() {
        let samples = random_sample_data::<f32>();
        let mut output = vec![0.0; (samples.len() / 2) - 1];
        let result = convert_stereo_to_mono_audio(&samples, &mut output);
        assert!(
            match result {
                Err(WhisperError::InputOutputLengthMismatch {
                    input_len,
                    output_len,
                }) => {
                    assert_eq!(
                        input_len,
                        samples.len() / 2,
                        "resulting input length is not half of num samples"
                    );
                    assert_eq!(
                        output_len,
                        output.len(),
                        "resulting output length is not the same as the output array"
                    );
                    true
                }
                _ => false,
            },
            "result was not a length mismatch: got {:?}",
            result
        );
    }

    #[bench]
    pub fn bench_stereo_to_mono(b: &mut test::Bencher) {
        let samples = random_sample_data::<f32>();
        let mut output = vec![0.0; samples.len() / 2];
        b.iter(|| {
            black_box(convert_stereo_to_mono_audio(
                black_box(&samples),
                black_box(&mut output),
            ))
        });
    }

    #[bench]
    pub fn bench_integer_to_float(b: &mut test::Bencher) {
        let samples = random_sample_data::<i16>();
        let mut output = vec![0.0f32; samples.len()];
        b.iter(|| {
            black_box(convert_integer_to_float_audio(
                black_box(&samples),
                black_box(&mut output),
            ))
        });
    }
}
