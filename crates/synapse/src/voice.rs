//! Dictation into the console, on the machine and nowhere else.
//!
//! Push to talk: start, say something, stop. What comes back is text in the
//! composer, which you then read before sending — the same as if you had typed
//! it. Nothing is sent to the mesh because you spoke.
//!
//! Two rules shape this, and both are about what Synapse is.
//!
//! **It never leaves the Mac.** `requiresOnDeviceRecognition` is set on every
//! request, and a recogniser that cannot honour it is treated as no recogniser
//! at all rather than quietly falling back to Apple's servers. Synapse is a
//! local memory store that has spent its whole existence promising the data
//! stays here; speech is data.
//!
//! **It costs nothing.** macOS already has the recogniser and the model. There
//! is no vendor, no key, and no per-minute meter — which is why this is the
//! backend that ships rather than one of the four services `nora-voice` can
//! also talk to.
//!
//! The microphone is the reason the whole feature is off by default. Synapse
//! holds a Keychain reference to every secret its user owns, and a build that
//! cannot listen is a better default for that program than one that can.

use anyhow::{Context, Result};
use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2_foundation::{NSString, NSURL};
use objc2_speech::{
    SFSpeechRecognitionResult, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
    SFSpeechURLRecognitionRequest,
};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use voice::{Capture, level::Meter};

/// The rate `nora-voice` resamples to on the way in, and what the WAV says.
const RATE: u32 = 16_000;

/// The longest utterance a dictation may be, in samples.
///
/// A capture that was started and forgotten is a microphone left open and a
/// file growing on disk. Two minutes is far longer than anything anyone dictates
/// into a message box, and short enough that forgetting costs nothing.
const CEILING: usize = RATE as usize * 120;

/// Whether this machine can do it at all.
///
/// Three separate things have to be true, and they fail differently: there is a
/// recogniser for the user's language, the service is up, and it can work
/// without the network. The last is the one that matters here — a recogniser
/// that only works online is not one this feature will use.
pub fn available() -> bool {
    unsafe {
        let recognizer = SFSpeechRecognizer::new();
        recognizer.isAvailable() && recognizer.supportsOnDeviceRecognition()
    }
}

/// What the user has said about the microphone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Unknown,
    Allowed,
    Refused,
}

pub fn access() -> Access {
    let status = unsafe { SFSpeechRecognizer::authorizationStatus() };
    match status {
        SFSpeechRecognizerAuthorizationStatus::Authorized => Access::Allowed,
        SFSpeechRecognizerAuthorizationStatus::NotDetermined => Access::Unknown,
        _ => Access::Refused,
    }
}

/// Ask, if nobody has. The answer arrives on a queue of the system's choosing
/// and this does not wait for it: the button that triggered this reads
/// [`access`] again on the next frame, which is soon enough and does not block
/// the window on a modal the user may leave sitting there.
pub fn ask() {
    let handler = RcBlock::new(|_status: SFSpeechRecognizerAuthorizationStatus| {});
    unsafe { SFSpeechRecognizer::requestAuthorization(&handler) };
}

/// A dictation in progress, or one being transcribed.
///
/// Recording and transcribing are both off the UI thread, and the result comes
/// back through a channel the console polls on the tick it already runs. A
/// window that freezes for the length of what you said is not dictation.
#[derive(Default)]
pub struct Dictation {
    recording: Option<Recording>,
    pending: Option<Receiver<Result<String>>>,
}

struct Recording {
    /// Dropping this closes the device, which is the whole of stopping.
    _capture: Capture,
    frames: Arc<Mutex<Vec<i16>>>,
}

impl Dictation {
    pub fn listening(&self) -> bool {
        self.recording.is_some()
    }

    pub fn transcribing(&self) -> bool {
        self.pending.is_some()
    }

    /// Open the microphone and start collecting. Fails loudly rather than
    /// recording silence: a capture that opened no device would otherwise look
    /// identical to a room nobody spoke in.
    pub fn start(&mut self) -> Result<()> {
        anyhow::ensure!(!self.listening(), "already listening");
        anyhow::ensure!(
            access() == Access::Allowed,
            "Synapse has not been allowed to use the microphone"
        );
        let frames: Arc<Mutex<Vec<i16>>> = Arc::default();
        let (tap, incoming) = channel::<Vec<i16>>();
        let collector = Arc::clone(&frames);
        std::thread::spawn(move || {
            for chunk in incoming {
                let Ok(mut held) = collector.lock() else {
                    return;
                };
                if held.len() >= CEILING {
                    continue;
                }
                held.extend_from_slice(&chunk);
            }
        });

        let device = voice::Devices::default_input().unwrap_or_default();
        let capture = Capture::open_with(&device, Meter::new(), Some(tap));
        self.recording = Some(Recording {
            _capture: capture,
            frames,
        });
        Ok(())
    }

    /// Close the microphone and start transcribing what it heard. The text
    /// arrives from [`Dictation::poll`].
    pub fn stop(&mut self) -> Result<()> {
        let recording = self.recording.take().context("nothing was being said")?;
        let samples = recording
            .frames
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default();
        // Dropping the capture closes the device before the work starts, so the
        // microphone is not held open for the length of the transcription.
        drop(recording);

        anyhow::ensure!(!samples.is_empty(), "no audio was captured");
        let (sender, receiver): (Sender<Result<String>>, _) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(transcribe(&samples));
        });
        self.pending = Some(receiver);
        Ok(())
    }

    /// What came back, once. `None` means still working, or nothing started.
    pub fn poll(&mut self) -> Option<Result<String>> {
        let receiver = self.pending.as_ref()?;
        match receiver.try_recv() {
            Ok(outcome) => {
                self.pending = None;
                Some(outcome)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                Some(Err(anyhow::anyhow!("the transcription stopped early")))
            }
        }
    }

    /// Throw away a dictation without transcribing it.
    pub fn cancel(&mut self) {
        self.recording = None;
        self.pending = None;
    }
}

/// Write the samples somewhere macOS can read them, then ask it what they say.
///
/// A file rather than a live audio buffer on purpose. Streaming would want an
/// `AVAudioEngine` and a format bridge between two audio stacks, to deliver
/// partial results nobody reads — this is a message box, not a conversation, and
/// the whole utterance is what goes in it.
fn transcribe(samples: &[i16]) -> Result<String> {
    let file = tempwav(samples)?;
    let outcome = recognize(&file.path);
    // The recording is the user's voice. It exists for as long as the sentence
    // takes to read and not one call longer.
    let _ = std::fs::remove_file(&file.path);
    outcome
}

struct Scratch {
    path: std::path::PathBuf,
}

fn tempwav(samples: &[i16]) -> Result<Scratch> {
    let mut path = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    path.push(format!("synapse-dictation-{stamp}.wav"));
    std::fs::write(&path, wav(samples)).with_context(|| format!("could not write {path:?}"))?;
    Ok(Scratch { path })
}

/// A 16-bit mono PCM WAV, by hand.
///
/// Forty-four bytes of header and the samples. A crate for this would be one
/// more dependency on an optional feature for something the format spells out.
fn wav(samples: &[i16]) -> Vec<u8> {
    let bytes = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + bytes);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + bytes) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM header length
    out.extend_from_slice(&1u16.to_le_bytes()); // uncompressed
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(bytes as u32).to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Hand one file to the on-device recogniser and wait for the final result.
fn recognize(path: &std::path::Path) -> Result<String> {
    let recognizer = unsafe { SFSpeechRecognizer::new() };
    anyhow::ensure!(
        unsafe { recognizer.isAvailable() },
        "the speech recogniser is not available right now"
    );
    anyhow::ensure!(
        unsafe { recognizer.supportsOnDeviceRecognition() },
        "this Mac cannot transcribe without sending audio to Apple, so Synapse will not transcribe at all"
    );

    let (sender, receiver) = channel::<Result<String>>();
    let done = Mutex::new(Some(sender));
    let handler = RcBlock::new(
        move |result: *mut SFSpeechRecognitionResult, error: *mut objc2_foundation::NSError| {
            // Called for partial results too. Only the final one is the answer,
            // and only the first send counts — the block may outlive the wait.
            let finished = unsafe {
                if let Some(result) = result.as_ref() {
                    result.isFinal().then(|| {
                        result
                            .bestTranscription()
                            .formattedString()
                            .to_string()
                            .trim()
                            .to_owned()
                    })
                } else if error.is_null() {
                    None
                } else {
                    Some(String::new())
                }
            };
            let message = match (finished, error.is_null()) {
                (None, _) => return,
                (Some(text), true) => Ok(text),
                (Some(_), false) => Err(anyhow::anyhow!("the recogniser could not read that")),
            };
            if let Ok(mut held) = done.lock()
                && let Some(sender) = held.take()
            {
                let _ = sender.send(message);
            }
        },
    );

    let text = unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let request = SFSpeechURLRecognitionRequest::initWithURL(
            SFSpeechURLRecognitionRequest::alloc(),
            &url,
        );
        request.setRequiresOnDeviceRecognition(true);
        request.setShouldReportPartialResults(false);
        let _task = recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler);
        receiver
            .recv_timeout(std::time::Duration::from_secs(60))
            .context("the recogniser never answered")?
    }?;

    anyhow::ensure!(!text.is_empty(), "nothing was said");
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_says_what_it_holds() {
        let file = wav(&[0, 1, -1, 32767]);
        assert_eq!(&file[0..4], b"RIFF");
        assert_eq!(&file[8..12], b"WAVE");
        assert_eq!(&file[36..40], b"data");
        assert_eq!(file.len(), 44 + 8);
        // The two length fields have to agree with the body, or every reader
        // either truncates the audio or runs off the end of it.
        assert_eq!(
            u32::from_le_bytes(file[4..8].try_into().unwrap()),
            44 - 8 + 8
        );
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 8);
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), RATE);
    }

    #[test]
    fn an_empty_recording_is_still_a_valid_file() {
        let file = wav(&[]);
        assert_eq!(file.len(), 44);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 0);
    }
}
