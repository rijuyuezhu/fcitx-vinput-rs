# Audio contract

`vinput-audio` owns pure PCM data structures and deterministic byte-level audio helpers. Desktop capture backends such as PipeWire should feed this crate instead of duplicating PCM parsing or format policy.

## PCM layout

The canonical in-memory representation is signed 16-bit interleaved PCM carried by `PcmBuffer` with explicit `PcmSpec` metadata:

- `sample_rate_hz`: non-zero sample rate.
- `channels`: non-zero interleaved channel count, defaulting to mono when omitted from JSON.
- `samples`: raw `i16` samples whose length must align to the channel count.

Frame-oriented calculations, duration, and silence trimming use frames rather than raw sample count. Multi-channel buffers are preserved as complete interleaved frames.

## Byte formats

Raw PCM bytes are signed 16-bit little-endian. Use `PcmBuffer::from_pcm16le_bytes` to decode raw bytes with explicit `PcmSpec`, and `i16_samples_to_le_bytes` when command ASR helpers need raw PCM bytes. Odd byte counts are rejected before sample conversion.

WAV decoding supports uncompressed RIFF/WAVE PCM format tag 1 with 16-bit samples. The parser preserves sample rate and channel metadata, skips unknown chunks using RIFF padding rules, rejects odd data chunk byte counts, and validates `block_align` plus `byte_rate` against the parsed sample format.

## Processing order

`AudioProcessingOptions::process` applies deterministic transforms in this order:

1. Trim leading and trailing silent frames using the absolute silence threshold.
2. Optionally normalize to a target peak.
3. Apply input gain with saturating `i16` conversion.

This order is part of the backend contract because command ASR helpers, file-input E2E demos, and future PipeWire capture should observe the same PCM delivered to ASR sessions.
