use anyhow::{Result, anyhow};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::Path;

pub fn import_wav<P: AsRef<Path>>(path: P, target_sample_rate: u32) -> Result<Vec<f32>> {
    let mut reader = WavReader::open(&path)?;
    let spec = reader.spec();

    // Read samples as f32 in interleaved order
    let raw_samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        SampleFormat::Int => {
            // Convert integer samples to float in [-1.0, 1.0]
            let max_value = 2_i32.pow((spec.bits_per_sample - 1) as u32) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / max_value)
                .collect()
        }
    };

    // Downmix to mono if needed by averaging channels per frame
    let mono_samples: Vec<f32> = if spec.channels > 1 {
        let ch = spec.channels as usize;
        raw_samples
            .chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        raw_samples
    };

    // If sample rates match, return mono as-is
    if spec.sample_rate == target_sample_rate {
        return Ok(mono_samples);
    }

    // Resample mono to target rate
    resample_audio(&mono_samples, spec.sample_rate, target_sample_rate, 1)
}

pub fn export_wav<P: AsRef<Path>>(path: P, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = WavSpec {
        channels: 1, // Mono
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut writer = WavWriter::create(&path, spec)?;

    for &sample in samples {
        writer.write_sample(sample)?;
    }

    writer.finalize()?;
    Ok(())
}

pub fn export_mixed_wav<P: AsRef<Path>>(
    path: P,
    layers: &[Vec<f32>],
    sample_rate: u32,
) -> Result<()> {
    if layers.is_empty() {
        return Err(anyhow!("No layers to export"));
    }

    // Find the maximum length
    let max_length = layers.iter().map(|layer| layer.len()).max().unwrap_or(0);

    if max_length == 0 {
        return Err(anyhow!("All layers are empty"));
    }

    // Mix all layers
    let mut mixed = vec![0.0; max_length];

    for layer in layers {
        for (i, &sample) in layer.iter().enumerate() {
            if i < max_length {
                mixed[i] += sample;
            }
        }
    }

    // Normalize and apply soft clipping
    let max_amplitude = mixed.iter().map(|&s| s.abs()).fold(0.0f32, |a, b| a.max(b));

    if max_amplitude > 0.0 {
        let normalization_factor = 0.95 / max_amplitude; // Leave some headroom
        for sample in &mut mixed {
            *sample *= normalization_factor;
            *sample = sample.clamp(-1.0, 1.0); // Soft clipping
        }
    }

    export_wav(path, &mixed, sample_rate)
}

fn resample_audio(
    samples: &[f32],
    input_rate: u32,
    output_rate: u32,
    channels: usize,
) -> Result<Vec<f32>> {
    if input_rate == output_rate {
        return Ok(samples.to_vec());
    }

    // Create resampler
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        output_rate as f64 / input_rate as f64,
        2.0, // Max ratio
        params,
        samples.len() / channels,
        channels,
    )?;

    // Resample
    let input = vec![samples];
    let output = resampler.process(&input, None)?;

    // Flatten the output
    Ok(output.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_import_export_roundtrip() {
        let original_samples = vec![0.1, -0.2, 0.3, -0.4, 0.5];
        let sample_rate = 44100;

        // Export to temporary file
        let temp_path = "test_roundtrip.wav";
        export_wav(temp_path, &original_samples, sample_rate).unwrap();

        // Import back
        let imported_samples = import_wav(temp_path, sample_rate).unwrap();

        // Clean up
        let _ = fs::remove_file(temp_path);

        // Compare (allowing for small floating point differences)
        assert_eq!(original_samples.len(), imported_samples.len());
        for (orig, imp) in original_samples.iter().zip(imported_samples.iter()) {
            assert!((orig - imp).abs() < 0.001);
        }
    }
}
