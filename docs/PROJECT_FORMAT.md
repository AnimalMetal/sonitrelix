# Project format v6

UTF-8 JSON with a .sonitrelix extension. Versions 1 through 5 migrate in memory. Subsequent saves write version 6. Keep a copy of the earlier project if you need to reopen it in an older Sonitrelix build. Saving preserves the source files.

```json
{
  "version": 6,
  "sample_rate": 48000,
  "export_metadata": {"title": "Session", "artist": "Studio A", "track": "2"},
  "markers": [{"name": "Chorus", "frame": 2880000}],
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

Track clips are consecutive ordered regions in sample frames; timeline position is the sum of prior lengths. Source null represents silence. Each nonempty source slice has positive frames and lies inside its source. All slices of a track share rate/channels. Each track uses ripple edits and explicit silence for gaps. Use different tracks to layer audio at the same time. Copy/duplicate retains immutable references. An empty track may adopt the pasted format. Version-2 media becomes one full-source clip; version-1 tracks remain empty. The legacy media property has been omitted since version 3.

Project rate is 8000–384000; current tracks/decoder/capture are restricted to 8000–192000 Hz, mono/stereo. Names must not be blank; gain finite >=0; pan finite -1 to +1. Source PCM bits 8/16/24/32 or float32, absolute paths, bounded RIFF frame count. Invalid values, out-of-range slices, overflow, more than 100000 clips per track, unknown fields and unknown versions are rejected. Position/selection and undo history are view state, not saved.

Mute/solo/gain/pan affect the mix; arm selects a recording target. Referenced WAV headers are rechecked when playback opens them; keep their files unchanged and in place. Save As retains absolute paths. Source contents are not embedded or hashed. Missing files produce an error. Recordings are assets in data/recordings; deleting or undoing a track never deletes them.

Project saves require .sonitrelix and reject a destination resolving to a referenced source. Save writes a sibling exclusive temporary file, flushes and uses Windows MoveFileExW replace-existing/write-through. Failure preserves the existing destination. Stale temporary files cause an error rather than silent deletion. Network shares/power-loss guarantees remain unverified. Dirty metadata is checkpointed every 30 seconds; startup offers recovery from an exited instance. Recording WAV headers checkpoint separately. Ctrl+I can recover complete frames into a new copy when an interrupted Sonitrelix recording has a readable header.
Version 4 adds optional original_sources: an array of absolute original compressed-source paths, protected against save/export overwrite. Version 3 upgrades without changing clips. Paths remain references, not embedded assets.

Version 5 adds optional `markers`, sorted by project frame. Each entry holds its name and an absolute frame at the project sample rate. Markers keep their times when audio edits shift clips. Versions 1 through 4 open with an empty marker list.

Version 6 adds optional `export_metadata` with title, artist, album, track number, year, genre, and comment. Fields are Unicode strings. Blank fields are omitted from file tags. Projects from earlier versions open with empty tags. Exporting a marked excerpt can use temporary tags without changing the project's saved tags.
