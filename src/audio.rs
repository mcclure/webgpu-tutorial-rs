// Construct CPAL stuff

use std::fmt::Display;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SizedSample,
};
use std::sync::mpsc;

use crate::constants::*;

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
    NoDevice,
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

fn audio_run<T>(device: &cpal::Device, config: &cpal::StreamConfig, audio_chunk_recv: mpsc::Receiver<Box<AudioChunk>>) -> Result<cpal::Stream, CpalError>
where
    T: SizedSample + FromSample<f32>,
{
//    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // Produce a sinusoid of maximum amplitude.
//    let mut sample_clock = 0f32;

    // AUDIO STATE HERE
    // One box is "current", the other is "previous"
    let mut chunks:[Box<AudioChunk>;2] = [Box::new(std::array::from_fn(|_|0.)), Box::new(std::array::from_fn(|_|0.))];
    let mut box_idx = 1;
    let mut sample_idx = AUDIO_CHUNK_LEN;
    let mut transitioning = false;
    let trail_by = AUDIO_CHUNK_LEN + AUDIO_CHUNK_LEN/2;

    let mut next_value = move || {
        // -- SYNTHESIS HERE --
        if sample_idx >= AUDIO_CHUNK_LEN {
            if let Ok(incoming_chunk) = audio_chunk_recv.try_recv() {
                chunks[box_idx] = incoming_chunk;
            }
            sample_idx = 0;
            transitioning = true;
        }
        // Chunks from the graphics thread are pre-windowed and pre-divided by two so we just need to sum them
        let out = chunks[if transitioning {(box_idx+1)%2} else {box_idx}][(sample_idx+trail_by)%AUDIO_CHUNK_LEN] + chunks[box_idx][sample_idx];
        sample_idx += 1;
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

    Ok(stream)
}

pub fn audio_spawn(audio_chunk_recv: mpsc::Receiver<Box<AudioChunk>>) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    if let Some(device) = host.default_output_device() {
        let config = device.default_output_config().unwrap();

        let stream_result = match config.sample_format() {
            cpal::SampleFormat::I8 => audio_run::<i8>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::I16 => audio_run::<i16>(&device, &config.into(), audio_chunk_recv),
            // cpal::SampleFormat::I24 => audio_run::<I24>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::I32 => audio_run::<i32>(&device, &config.into(), audio_chunk_recv),
            // cpal::SampleFormat::I48 => audio_run::<I48>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::I64 => audio_run::<i64>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::U8 => audio_run::<u8>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::U16 => audio_run::<u16>(&device, &config.into(), audio_chunk_recv),
            // cpal::SampleFormat::U24 => audio_run::<U24>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::U32 => audio_run::<u32>(&device, &config.into(), audio_chunk_recv),
            // cpal::SampleFormat::U48 => audio_run::<U48>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::U64 => audio_run::<u64>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::F32 => audio_run::<f32>(&device, &config.into(), audio_chunk_recv),
            cpal::SampleFormat::F64 => audio_run::<f64>(&device, &config.into(), audio_chunk_recv),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        };

        match stream_result {
            Err(e) => {
                println!("Failure: {}", e);
                None
            },
            Ok(v) => {
                println!("Boot");
                Some(v)
            }
        }
    } else {
        println!("Failure: No device");
        None
    }
}
