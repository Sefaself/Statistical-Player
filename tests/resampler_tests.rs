//! Resampler tests: 440 Hz sine tone at 44.1 kHz and 48 kHz
//! resampled to device rates 44100, 48000, and 192000 Hz.
//!
//! Verifies:
//! 1. Frame count accuracy (target_rate * duration within 1 frame)
//! 2. Pitch accuracy (estimated frequency matches 440 Hz within 0.5 Hz)
//! 3. Equal-rate bit-identical passthrough

use statistical_player::audio::AudioResampler;

fn generate_440hz_stereo(sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let total_frames = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = Vec::with_capacity(total_frames * 2);
    let freq = 440.0_f32;
    for i in 0..total_frames {
        let t = i as f32 / sample_rate as f32;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin();
        samples.push(s); // Left
        samples.push(s); // Right
    }
    samples
}

/// Estimates fundamental pitch from positive-going zero crossings with linear interpolation.
fn estimate_pitch(samples: &[f32], channels: usize, sample_rate: u32) -> f32 {
    let frames = samples.len() / channels;
    // Skip boundary transients by taking the steady-state central 80%
    let start_frame = frames / 10;
    let end_frame = (frames * 9) / 10;

    let mut crossings = Vec::new();
    for f in start_frame..end_frame.saturating_sub(1) {
        let s1 = samples[f * channels];
        let s2 = samples[(f + 1) * channels];
        if s1 <= 0.0 && s2 > 0.0 {
            let frac = -s1 / (s2 - s1);
            crossings.push(f as f64 + frac as f64);
        }
    }

    assert!(
        crossings.len() >= 10,
        "Too few zero crossings to estimate pitch"
    );
    let total_periods = (crossings.len() - 1) as f64;
    let total_samples = crossings.last().unwrap() - crossings.first().unwrap();
    let avg_period_samples = total_samples / total_periods;
    let freq = sample_rate as f64 / avg_period_samples;
    freq as f32
}

#[test]
fn test_equal_rate_bit_identical() {
    for &rate in &[44100, 48000, 192000] {
        let mut resampler = AudioResampler::new(rate, rate, 2)
            .unwrap_or_else(|e| panic!("Failed to create equal-rate resampler at {rate}: {e}"));

        assert!(
            resampler.is_passthrough(),
            "Equal-rate {rate} -> {rate} must be passthrough"
        );

        let input = generate_440hz_stereo(rate, 1.0);
        let output = resampler
            .resample_all(&input)
            .unwrap_or_else(|e| panic!("resample_all failed for {rate} -> {rate}: {e}"));

        assert_eq!(
            input.len(),
            output.len(),
            "Equal-rate output length mismatch"
        );
        // Bit-identical verification
        assert_eq!(
            input, output,
            "Equal-rate {rate} -> {rate} must be 100% bit-identical"
        );
    }
}

#[test]
fn test_resampler_frame_count_and_pitch() {
    let source_rates = [44100, 48000];
    let device_rates = [44100, 48000, 192000];
    let duration = 1.0_f32;

    for &src_rate in &source_rates {
        for &dst_rate in &device_rates {
            let mut resampler = AudioResampler::new(src_rate, dst_rate, 2).unwrap_or_else(|e| {
                panic!("Failed to create resampler for {src_rate} -> {dst_rate}: {e}")
            });

            let input = generate_440hz_stereo(src_rate, duration);
            let output = resampler.resample_all(&input).unwrap_or_else(|e| {
                panic!("resample_all failed for {src_rate} -> {dst_rate}: {e}")
            });

            let channels = 2;
            let output_frames = output.len() / channels;
            let expected_frames = (dst_rate as f32 * duration) as usize;

            // 1. Frame count verification (within 1 frame of expected duration)
            let frame_diff = (output_frames as isize - expected_frames as isize).abs();
            assert!(
                frame_diff <= 1,
                "Frame count mismatch for {src_rate} -> {dst_rate}: got {output_frames}, expected {expected_frames}"
            );

            // 2. Pitch verification (440 Hz within 0.5 Hz tolerance)
            let estimated_pitch = estimate_pitch(&output, channels, dst_rate);
            let pitch_diff = (estimated_pitch - 440.0).abs();
            assert!(
                pitch_diff < 0.5,
                "Pitch mismatch for {src_rate} -> {dst_rate}: estimated {estimated_pitch:.2} Hz, expected 440.0 Hz"
            );
        }
    }
}
