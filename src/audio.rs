// Construct CPAL stuff

use std::fmt::Display;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SizedSample,
};

fn audio_write<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> f32)
where
    T: Sample + FromSample<f32>,
{
    for frame in output.chunks_mut(channels) {
        let value: T = T::from_sample(next_sample());
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}

#[derive(Debug)]
enum CpalError {
	Build(cpal::BuildStreamError),
	Play(cpal::PlayStreamError),
	Unknown
}

impl std::error::Error for CpalError {}
impl Display for CpalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    	write!(f, "{:?}", self)
    }
}
impl From<cpal::BuildStreamError> for CpalError { fn from(e: cpal::BuildStreamError) -> Self { CpalError::Build(e) } }
impl From<cpal::PlayStreamError> for CpalError { fn from(e: cpal::PlayStreamError) -> Self { CpalError::Play(e) } }

fn audio_run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<(), CpalError>
where
    T: SizedSample + FromSample<f32>,
{
//    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // Produce a sinusoid of maximum amplitude.
//    let mut sample_clock = 0f32;

    const F_COUNT:usize = 6;
    let mut phase:[f32;F_COUNT] = Default::default();
    let mut next_value = move || {
        // -- SYNTHESIS HERE --
        let mut out: f32 = 0.0;
        for i in 0..F_COUNT {
            phase[i] += (i+1) as f32*220./44100.0;
            if phase[i] <= -1.0 || phase[i] > 1.0 {
                phase[i] = (phase[i] + 1.0).rem_euclid(2.0) - 1.0;
            }
            out += phase[i]/F_COUNT as f32;
        }
        out
        // -- BOILERPLATE --
    };

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            audio_write(data, channels, &mut next_value)
        },
        err_fn,
        None,
    )?;
    stream.play()?;

    Ok(())
}

pub fn audio_spawn() -> cpal::Host {
    let host = cpal::default_host();
    if let Some(device) = host.default_output_device() {
        let config = device.default_output_config().unwrap();

        if let Err(e) = match config.sample_format() {
            cpal::SampleFormat::I8 => audio_run::<i8>(&device, &config.into()),
            cpal::SampleFormat::I16 => audio_run::<i16>(&device, &config.into()),
            // cpal::SampleFormat::I24 => audio_run::<I24>(&device, &config.into()),
            cpal::SampleFormat::I32 => audio_run::<i32>(&device, &config.into()),
            // cpal::SampleFormat::I48 => audio_run::<I48>(&device, &config.into()),
            cpal::SampleFormat::I64 => audio_run::<i64>(&device, &config.into()),
            cpal::SampleFormat::U8 => audio_run::<u8>(&device, &config.into()),
            cpal::SampleFormat::U16 => audio_run::<u16>(&device, &config.into()),
            // cpal::SampleFormat::U24 => audio_run::<U24>(&device, &config.into()),
            cpal::SampleFormat::U32 => audio_run::<u32>(&device, &config.into()),
            // cpal::SampleFormat::U48 => audio_run::<U48>(&device, &config.into()),
            cpal::SampleFormat::U64 => audio_run::<u64>(&device, &config.into()),
            cpal::SampleFormat::F32 => audio_run::<f32>(&device, &config.into()),
            cpal::SampleFormat::F64 => audio_run::<f64>(&device, &config.into()),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        } {
            println!("Failure: {}", e);
        } else {
            println!("Boot");
        }
    } else {
        println!("Failure: No device");
    }

    host
}
