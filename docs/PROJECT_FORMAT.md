# Project format v4

UTF-8 JSON with a .sonitrelix extension. Versions 1/2/3 migrate in memory; subsequent saves write version 4, which older builds cannot read. No source file is rewritten.

```json
{
  "version": 4,
  "sample_rate": 48000,
  "tracks": [{
    "name": "Vocals", "mute": false, "solo": false, "armed": true,
    "gain": 1.0, "pan": 0.0, "sample_rate": 48000, "channels": 1,
    "clips": [
      {"source": null, "source_start": 0, "frames": 48000},
      {"source": {
        "path": "C:\\Audio\\voice.wav", "sample_rate": 48000,
        "channels": 1, "bits": 16, "frames": 144000, "float": false
      }, "source_start": 24000, "frames": 96000}
    ]
  }]
}
```

Track clips are consecutive ordered regions in sample frames; timeline position is the sum of prior lengths. Source null represents silence. Each nonempty source slice has positive frames and lies inside its source. All slices of a track share rate/channels. This milestone uses ripple edits, not overlapping clips; gaps are explicit silence. Copy/duplicate retains immutable references. An empty track may adopt the pasted format. Version-2 media becomes one full-source clip; version-1 tracks remain empty. The legacy media property is omitted in version 3.

Project rate is 8000–384000; current tracks/decoder/capture are restricted to 8000–192000 Hz, mono/stereo. Names must not be blank; gain finite >=0; pan finite -1 to +1. Source PCM bits 8/16/24/32 or float32, absolute paths, bounded RIFF frame count. Invalid values, out-of-range slices, overflow, more than 100000 clips per track, unknown fields and unknown versions are rejected. Position/selection and undo history are view state, not saved.

Mute/solo/gain/pan affect the mix; arm selects a recording target. Referenced WAV headers are rechecked when playback opens them; keep their files unchanged and in place. Save As retains absolute paths. Source contents are not embedded or hashed. Missing files produce an error. Recordings are assets in data/recordings; deleting or undoing a track never deletes them.

Project saves require .sonitrelix and reject a destination resolving to a referenced source. Save writes a sibling exclusive temporary file, flushes and uses Windows MoveFileExW replace-existing/write-through. Failure preserves the existing destination. Stale temporary files cause an error rather than silent deletion. Network shares/power-loss guarantees remain unverified. Dirty metadata is checkpointed every 30 seconds; startup offers recovery from an exited instance. Recording WAV header checkpoints are separate and do not implement full project recovery.
Version 4 adds optional original_sources: an array of absolute original compressed-source paths, protected against save/export overwrite. Version 3 upgrades without changing clips. Paths remain references, not embedded assets.
