This is a Rust+WebGPU "Hello world" dual-mode desktop/web app, based on the wgpu [hello-triangle](https://github.com/gfx-rs/wgpu/tree/trunk/examples/hello-triangle) example. Make sure to edit license.txt if you fork unless you want to release in the public domain.

Run with `--features audio_log` to emit an on-disk live recording of the sound output. This is raw data (mono 32 bit floats) and can be opened with (for example) Audacity. It may not save correctly if the app crashes.

Created by Andi McClure.

[Build/usage instructions](run.txt)

[License](LICENSE.txt)
