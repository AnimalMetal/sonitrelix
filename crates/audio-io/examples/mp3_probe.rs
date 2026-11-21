use windows::Win32::{Media::MediaFoundation::*, System::Com::*};
use windows::core::Interface;
fn main() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
        MFStartup(MF_VERSION, MFSTARTUP_FULL).unwrap();
        match MFTranscodeGetAudioOutputAvailableTypes(&MFAudioFormat_MP3, 0x3f, None) {
            Ok(types) => {
                let count = types.GetElementCount().unwrap();
                println!("Native MP3 output types: {count}");
                for i in 0..count.min(20) {
                    let t: IMFMediaType = types.GetElement(i).unwrap().cast().unwrap();
                    println!(
                        "rate {:?}, channels {:?}, byte rate {:?}",
                        t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND),
                        t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS),
                        t.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND)
                    );
                }
            }
            Err(e) => println!("Native MP3 encoder unavailable: {e}"),
        }
        let _ = MFShutdown();
        CoUninitialize();
    }
}
