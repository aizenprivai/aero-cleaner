//! Sounds: an original pack made for AeroClean. Every sound is synthesized
//! here from sine waves (bell and marimba tones), so there are no sound
//! files and nothing borrowed. They're played by the system's own player
//! (PipeWire, PulseAudio or ALSA on Linux, afplay on macOS, PlaySound on
//! Windows), so no audio library is needed to build AeroClean.

use std::f32::consts::TAU;
use std::path::PathBuf;

pub const RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    /// A clean finished: a rising chime.
    CleanDone,
    /// A scan finished: two soft notes.
    ScanDone,
    /// Switching screens: a gentle click.
    Click,
    /// Something went wrong: a friendly, falling two-tone.
    Warning,
}

impl Event {
    pub const ALL: [Event; 4] = [Event::CleanDone, Event::ScanDone, Event::Click, Event::Warning];

    pub fn code(self) -> &'static str {
        match self {
            Event::CleanDone => "clean",
            Event::ScanDone => "scan",
            Event::Click => "click",
            Event::Warning => "warning",
        }
    }

    pub fn from_code(s: &str) -> Option<Event> {
        Event::ALL.into_iter().find(|e| e.code() == s.trim())
    }
}

/// The voice of the sounds, chosen by the theme (`[sound] pack`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pack {
    /// Bright glassy bells, the classic Aero sparkle.
    Glass,
    /// Warm, round marimba tones.
    Soft,
    /// This theme is silent.
    None,
}

impl Pack {
    pub fn from_name(s: &str) -> Pack {
        match s.trim() {
            "soft" => Pack::Soft,
            "none" => Pack::None,
            _ => Pack::Glass,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Pack::Glass => "glass",
            Pack::Soft => "soft",
            Pack::None => "none",
        }
    }
}

/// One struck note: partials (frequency ratio, loudness, decay time in
/// seconds) added at `start` seconds into `out`.
fn strike(out: &mut Vec<f32>, start: f32, freq: f32, partials: &[(f32, f32, f32)], gain: f32) {
    let longest = partials.iter().map(|p| p.2).fold(0.0, f32::max);
    let len = ((longest * 6.0) * RATE as f32) as usize;
    let at = (start * RATE as f32) as usize;
    if out.len() < at + len {
        out.resize(at + len, 0.0);
    }
    let attack = 0.004 * RATE as f32;
    for i in 0..len {
        let t = i as f32 / RATE as f32;
        let env_in = (i as f32 / attack).min(1.0);
        let mut v = 0.0;
        for &(ratio, amp, decay) in partials {
            v += amp * (TAU * freq * ratio * t).sin() * (-t / decay).exp();
        }
        out[at + i] += gain * env_in * v;
    }
}

/// A sound, as samples from -1 to 1 at 44.1 kHz. Silent packs give nothing.
pub fn synth(event: Event, pack: Pack, volume: f32) -> Vec<f32> {
    // Bell partials (inharmonic, glassy) and marimba ones (round, short).
    let bell: &[(f32, f32, f32)] = &[(1.0, 1.0, 0.3), (2.76, 0.32, 0.15), (5.4, 0.1, 0.08), (8.93, 0.04, 0.04)];
    let marimba: &[(f32, f32, f32)] = &[(1.0, 1.0, 0.28), (3.93, 0.22, 0.07), (9.2, 0.05, 0.02)];
    let (voice, octave) = match pack {
        Pack::Glass => (bell, 1.0),
        Pack::Soft => (marimba, 0.5),
        Pack::None => return vec![],
    };
    let mut out = vec![];
    match event {
        Event::CleanDone => {
            // C, E, G, C: a rising sparkle.
            for (k, f) in [1046.5, 1318.5, 1568.0, 2093.0].into_iter().enumerate() {
                strike(&mut out, k as f32 * 0.09, f * octave, voice, 0.8 - k as f32 * 0.08);
            }
        }
        Event::ScanDone => {
            strike(&mut out, 0.0, 784.0 * octave, voice, 0.6);
            strike(&mut out, 0.11, 1046.5 * octave, voice, 0.55);
        }
        Event::Click => {
            // A tiny, quick tap.
            strike(&mut out, 0.0, 2200.0 * octave.max(0.7), &[(1.0, 1.0, 0.012), (2.4, 0.3, 0.006)], 0.35);
        }
        Event::Warning => {
            // E then C, rounder and lower: noticeable, never harsh.
            let round: &[(f32, f32, f32)] = &[(1.0, 1.0, 0.3), (2.0, 0.25, 0.12), (3.0, 0.08, 0.05)];
            strike(&mut out, 0.0, 659.3 * octave.max(0.75), round, 0.7);
            strike(&mut out, 0.16, 523.3 * octave.max(0.75), round, 0.7);
        }
    }
    // Trim the silent tail, then scale to the volume (with headroom).
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    // (-50 dB is inaudible under the rest.)
    let last = out.iter().rposition(|v| v.abs() > peak * 0.003).map(|i| i + 1).unwrap_or(0);
    out.truncate(last);
    let gain = 0.8 * volume.clamp(0.0, 1.0) / peak;
    // A short fade-out so the end never clicks.
    let fade = (0.01 * RATE as f32) as usize;
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        *v *= gain * ((n - i) as f32 / fade as f32).min(1.0);
    }
    out
}

/// Samples as a 16-bit mono WAV file.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    b
}

/// The sound's file, made the first time it's needed.
fn file_for(event: Event, pack: Pack, volume: f32) -> Option<PathBuf> {
    let step = (volume.clamp(0.0, 1.0) * 10.0).round() as u32;
    let dir = crate::platform::data_dir().join("sounds");
    // The version in the name: if the sounds ever change, old files aren't used.
    let path = dir.join(format!("v1-{}-{}-{step}.wav", pack.name(), event.code()));
    if !path.exists() {
        let samples = synth(event, pack, step as f32 / 10.0);
        if samples.is_empty() {
            return None;
        }
        std::fs::create_dir_all(&dir).ok()?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, wav(&samples)).ok()?;
        std::fs::rename(&tmp, &path).ok()?;
    }
    Some(path)
}

/// Plays a sound in the background. Never blocks, never fails loudly.
pub fn play(event: Event, pack: Pack, volume: f32) {
    if cfg!(test) || pack == Pack::None || volume <= 0.0 {
        return;
    }
    // The same sound at most every 80 ms (a burst of clicks stays one click).
    {
        use std::collections::HashMap;
        use std::sync::Mutex;
        use std::time::{Duration, Instant};
        static LAST: Mutex<Option<HashMap<Event, Instant>>> = Mutex::new(None);
        let Ok(mut last) = LAST.lock() else { return };
        let map = last.get_or_insert_with(HashMap::new);
        if map.get(&event).is_some_and(|t| t.elapsed() < Duration::from_millis(80)) {
            return;
        }
        map.insert(event, Instant::now());
    }
    std::thread::spawn(move || {
        if let Some(path) = file_for(event, pack, volume) {
            play_file(&path);
        }
    });
}

#[cfg(target_os = "linux")]
fn play_file(path: &std::path::Path) {
    use std::sync::OnceLock;
    // The first player found: PipeWire, then PulseAudio, then plain ALSA.
    static PLAYER: OnceLock<Option<(PathBuf, &'static [&'static str])>> = OnceLock::new();
    let player = PLAYER.get_or_init(|| {
        let dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        let options: [(&str, &'static [&'static str]); 3] = [("pw-play", &[]), ("paplay", &[]), ("aplay", &["-q"])];
        options.iter().find_map(|(name, args)| dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()).map(|p| (p, *args)))
    });
    if let Some((exe, args)) = player {
        run(std::process::Command::new(exe).args(*args).arg(path));
    }
}

#[cfg(target_os = "macos")]
fn play_file(path: &std::path::Path) {
    run(std::process::Command::new("/usr/bin/afplay").arg(path));
}

#[cfg(windows)]
fn play_file(path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SND_ASYNC returns at once; the system keeps playing.
    unsafe {
        PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), SND_ASYNC | SND_FILENAME | SND_NODEFAULT);
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn play_file(_path: &std::path::Path) {}

/// Runs a player quietly and waits for it (so it doesn't linger as a zombie).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run(cmd: &mut std::process::Command) {
    use std::process::Stdio;
    if let Ok(mut child) = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_clean_and_short() {
        for pack in [Pack::Glass, Pack::Soft] {
            for e in Event::ALL {
                let s = synth(e, pack, 1.0);
                let secs = s.len() as f32 / RATE as f32;
                assert!(!s.is_empty() && secs < 2.2, "{e:?} {pack:?}: {secs}s");
                assert!(s.iter().all(|v| v.is_finite() && v.abs() <= 0.81), "{e:?} {pack:?} stays under the ceiling");
                let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(peak > 0.7, "{e:?} {pack:?} is loud enough at full volume");
                assert!(s.last().unwrap().abs() < 0.01, "{e:?} {pack:?} ends softly");
                assert_eq!(Event::from_code(e.code()), Some(e));
            }
            let click = synth(Event::Click, pack, 1.0).len();
            let chime = synth(Event::CleanDone, pack, 1.0).len();
            assert!(click * 10 < chime, "a click is a tap, a finished clean is a chime");
        }
        assert_ne!(synth(Event::CleanDone, Pack::Glass, 1.0), synth(Event::CleanDone, Pack::Soft, 1.0));
        assert!(synth(Event::CleanDone, Pack::None, 1.0).is_empty());
        let quiet = synth(Event::Warning, Pack::Glass, 0.25);
        assert!(quiet.iter().fold(0.0f32, |m, v| m.max(v.abs())) < 0.21);
        assert_eq!(Pack::from_name("soft"), Pack::Soft);
        assert_eq!(Pack::from_name("something-new"), Pack::Glass);
    }

    /// Writes every sound to AERO_SOUNDS (a folder), to listen to them.
    #[test]
    #[ignore]
    fn export_sounds() {
        let dir = PathBuf::from(std::env::var("AERO_SOUNDS").unwrap_or_else(|_| "target/sounds".into()));
        std::fs::create_dir_all(&dir).unwrap();
        for pack in [Pack::Glass, Pack::Soft] {
            for e in Event::ALL {
                std::fs::write(dir.join(format!("{}-{}.wav", pack.name(), e.code())), wav(&synth(e, pack, 0.8))).unwrap();
            }
        }
    }

    #[test]
    fn wav_files_are_well_formed() {
        let s = synth(Event::ScanDone, Pack::Glass, 0.5);
        let w = wav(&s);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), RATE);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize, s.len() * 2);
        assert_eq!(w.len(), 44 + s.len() * 2);
    }
}
