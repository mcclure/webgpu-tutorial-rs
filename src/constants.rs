// Appearance constants

pub const DEFAULT_STROKE:f32 = 0.1;

pub const TILES_ACROSS:u32 = 5;

pub const GRID_ANIMATE_SPEED:f32 = 1.5;

// GPUImageCopyBuffer requires this to be a multiple of 256
pub const AUDIO_READBACK_BUFFER_LEN:usize = 1024;

pub const AUDIO_CHUNK_LEN:usize = AUDIO_READBACK_BUFFER_LEN*2;

pub type AudioChunk = [f32;AUDIO_CHUNK_LEN];
