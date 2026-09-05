//! A voice message, recorded through the platform's own audio stack.
//!
//! QML on Qt 5.6 has no audio recorder: `QAudioRecorder` is C++ only, and
//! qmetaobject does not bind it. So this is the tree's second `cpp!`
//! block, after the translator (`postivene-app/src/translations.rs`), and
//! the reason the shim has a C++ build step: the recorder is made, driven
//! and asked about from the few lines of C++ below, and everything else
//! is Rust. The exception is kept to this module, and to blocks short
//! enough to be checked by reading.
//!
//! What is recorded goes through `GStreamer` on a device, in whatever
//! codec and container it offers: Opus in Ogg where it can, the way the
//! desktop client records, and down the list from there. The file lands
//! in the captures directory (`capture.rs`) and is sent as a voice
//! message -- the core's `Voice` view type, which is what draws it as one
//! at the other end rather than as a music file.
//!
//! Nothing is connected to the recorder's signals: the page polls, on a
//! timer it runs only while recording, which is one call rather than a
//! signal bridge. Stopping is asynchronous in the `GStreamer` backend --
//! the file is finished a moment after `stop()` returns -- so the
//! recording is reported once the recorder says it is no longer
//! finalising, from that same poll.

// `cpp!` expands to a call across the FFI boundary, which is `unsafe` by
// construction. Scoped to this file: the workspace denies it everywhere
// else, and docs/BUILDING.md says why this one is allowed.
#![allow(unsafe_code)]

use std::ffi::c_void;

use cpp::cpp;
use qmetaobject::*;

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QString>
    #include <QtCore/QStringList>
    #include <QtCore/QUrl>
    #include <QtMultimedia/QAudioEncoderSettings>
    #include <QtMultimedia/QAudioRecorder>
    #include <QtMultimedia/QMediaRecorder>
}}

/// The codecs worth recording a voice message in, best first, each with
/// the container that carries it and the extension that names it. What
/// `GStreamer` calls them: a codec is matched by whether its name contains
/// the word, since the exact strings vary between releases.
const CODECS: [(&str, &str, &str); 5] = [
    // What the desktop client records, and what every current client
    // plays.
    ("opus", "ogg", "ogg"),
    ("vorbis", "ogg", "ogg"),
    ("mpeg", "mp4", "m4a"),
    ("flac", "ogg", "ogg"),
    // Uncompressed, and large, but a phone with nothing else still
    // records.
    ("wav", "wav", "wav"),
];

/// Pick a codec and container from what the recorder offers.
///
/// A codec that has no container to go in is skipped, and nothing at
/// all -- the headless test runner, or a phone with no `GStreamer`
/// encoders -- is `None`, which the page shows as no microphone button.
fn choose_format(
    codecs: &[String],
    containers: &[String],
) -> Option<(String, String, &'static str)> {
    let lower = |name: &String| name.to_ascii_lowercase();
    for (codec, container, extension) in CODECS {
        let Some(found_codec) = codecs.iter().find(|name| lower(name).contains(codec)) else {
            continue;
        };
        let Some(found_container) = containers.iter().find(|name| {
            let name = lower(name);
            // "wav" is offered as "audio/x-wav" and as "wav"; "mp4" as
            // "video/quicktime, variant=(string)iso" as often as not.
            name.contains(container) || (container == "mp4" && name.contains("quicktime"))
        }) else {
            continue;
        };
        return Some((found_codec.clone(), found_container.clone(), extension));
    }
    None
}

/// `QMediaRecorder::Status`, as the C++ reports it.
const FINALIZING_STATUS: i32 = 7;
/// `QMediaRecorder::State::RecordingState`.
const RECORDING_STATE: i32 = 1;

/// Records a voice message.
///
/// ```qml
/// VoiceRecorder { id: recorder; onRecorded: messages.send_voice(path) }
/// IconButton { visible: recorder.available; onClicked: recorder.start(path) }
/// Timer { running: recorder.recording; onTriggered: recorder.poll() }
/// ```
#[derive(QObject, Default)]
// `available` and `recording` are two facts QML binds to on their own;
// `finishing` and `probed` are the recorder's own bookkeeping. They are
// not states of one thing, and a state machine over them would hide the
// two bindings behind it.
#[allow(clippy::struct_excessive_bools)]
pub struct VoiceRecorder {
    base: qt_base_class!(trait QObject),

    /// Whether anything can be recorded at all: an audio input and an
    /// encoder to write it with. False headlessly, and on a phone with
    /// no encoders, and then the page offers no microphone.
    pub available: qt_property!(bool; READ is_available),
    /// The extension a recording will have, from the codec chosen:
    /// `ogg`, `m4a` or `wav`. Empty when nothing can be recorded.
    pub extension: qt_property!(QString; READ extension_name),
    /// What is recording, for the reader to see under the time: the
    /// codec, the container and the audio input, as `GStreamer` names
    /// them. Empty when nothing can be recorded.
    pub format: qt_property!(QString; READ format_name),

    /// True from `start` until the recording is reported or dropped.
    pub recording: qt_property!(bool; NOTIFY recording_changed),
    /// Emitted when [`Self::recording`] changes.
    pub recording_changed: qt_signal!(),
    /// Milliseconds recorded so far, as of the last `poll`.
    pub duration_ms: qt_property!(u32; NOTIFY duration_changed),
    /// Emitted when [`Self::duration_ms`] changes.
    pub duration_changed: qt_signal!(),

    /// Start recording into `path`. Answers on `recorded` once `stop`
    /// has been called and the file is finished, or on `error`.
    pub start: qt_method!(fn(&mut self, path: QString)),
    /// Stop, and report the file once it is finished.
    pub stop: qt_method!(fn(&mut self)),
    /// Stop, and throw the file away.
    pub cancel: qt_method!(fn(&mut self)),
    /// Re-read the duration, and finish a stop that is under way. The
    /// page calls this on a timer while recording.
    pub poll: qt_method!(fn(&mut self)),

    /// The recording is finished and at `path`.
    pub recorded: qt_signal!(path: QString),
    /// Recording could not start, or failed on the way.
    pub error: qt_signal!(message: QString),

    /// The `QAudioRecorder`, made on first use: it needs the application
    /// object, which does not exist when QML builds this.
    handle: usize,
    /// Where the recording is going.
    path: String,
    /// `stop` has been called and the file is still being finished.
    finishing: bool,
    /// Cached from the recorder, so QML's bindings need not ask C++.
    chosen: Option<(String, String, &'static str)>,
    /// The audio input the recorder was told to use; empty for its own
    /// default.
    input: String,
    probed: bool,
}

impl VoiceRecorder {
    /// Whether anything can be recorded at all.
    pub fn is_available(&mut self) -> bool {
        self.probe();
        self.chosen.is_some()
    }

    /// The extension a recording will have.
    pub fn extension_name(&mut self) -> QString {
        self.probe();
        self.chosen
            .as_ref()
            .map_or_else(QString::default, |(_, _, extension)| {
                QString::from(*extension)
            })
    }

    /// The codec, container and input in use, for the reader to see.
    pub fn format_name(&mut self) -> QString {
        self.probe();
        let Some((codec, container, _)) = &self.chosen else {
            return QString::default();
        };
        let input = if self.input.is_empty() {
            "default input"
        } else {
            self.input.as_str()
        };
        QString::from(format!("{codec} \u{b7} {container} \u{b7} {input}"))
    }

    /// Make the recorder if it is not there yet, and find out what it
    /// can write.
    fn probe(&mut self) {
        if self.probed {
            return;
        }
        self.probed = true;
        let handle = self.recorder();
        if handle == 0 {
            return;
        }
        let codecs = read_list(handle, ListKind::Codecs);
        let containers = read_list(handle, ListKind::Containers);
        self.chosen = choose_format(&codecs, &containers);
        let inputs = read_list(handle, ListKind::Inputs);
        let default = read_default_input(handle);
        if let Some(input) = choose_input(&inputs, &default) {
            set_input(handle, &input);
            self.input = input;
        }
    }

    /// The `QAudioRecorder`, made on first use. 0 when there is no
    /// application object to hang it off, which is when nothing can be
    /// recorded either.
    fn recorder(&mut self) -> usize {
        if self.handle == 0 {
            // SAFETY: nothing is passed in, and the recorder made here is
            // owned by this object, which deletes it in `Drop`. It is
            // only ever used from the Qt thread, which is where every
            // method of this object runs.
            let handle = cpp!(unsafe [] -> *mut c_void as "void*" {
                if (!QCoreApplication::instance()) {
                    return nullptr;
                }
                return new QAudioRecorder();
            });
            self.handle = handle as usize;
        }
        self.handle
    }

    /// Start recording into `path`.
    pub fn start(&mut self, path: QString) {
        if self.recording {
            return;
        }
        self.probe();
        let Some((codec, container, _)) = self.chosen.clone() else {
            self.error(QString::from("nothing here can record sound"));
            return;
        };
        let path = path.to_string();
        if path.is_empty() {
            self.error(QString::from("nowhere to record to"));
            return;
        }
        let handle = self.recorder();
        if handle == 0 {
            self.error(QString::from("nothing here can record sound"));
            return;
        }
        let codec = QString::from(codec);
        let container = QString::from(container);
        let location = QString::from(path.clone());
        let recorder = handle as *mut c_void;
        // SAFETY: `recorder` is the QAudioRecorder this object made and
        // still owns; the three QStrings are owned by this frame and read
        // by value.
        let started = cpp!(unsafe [recorder as "QAudioRecorder*", codec as "QString",
                                   container as "QString", location as "QString"] -> bool as "bool" {
            QAudioEncoderSettings settings;
            settings.setCodec(codec);
            // One channel: a voice, and half the bytes of two.
            settings.setChannelCount(1);
            settings.setEncodingMode(QMultimedia::ConstantQualityEncoding);
            settings.setQuality(QMultimedia::NormalQuality);
            recorder->setEncodingSettings(settings, QVideoEncoderSettings(), container);
            recorder->setOutputLocation(QUrl::fromLocalFile(location));
            recorder->record();
            return recorder->error() == QMediaRecorder::NoError;
        });
        if !started {
            self.error(QString::from(read_error(handle)));
            return;
        }
        self.path = path;
        self.finishing = false;
        self.duration_ms = 0;
        self.duration_changed();
        self.recording = true;
        self.recording_changed();
    }

    /// Stop, and report the file once it is finished.
    pub fn stop(&mut self) {
        if !self.recording || self.finishing {
            return;
        }
        let handle = self.recorder();
        stop_recording(handle);
        self.finishing = true;
        // The backend may already be done; a poll now saves a tick.
        self.poll();
    }

    /// Stop, and throw the file away.
    pub fn cancel(&mut self) {
        if !self.recording {
            return;
        }
        let handle = self.recorder();
        stop_recording(handle);
        let _ = std::fs::remove_file(&self.path);
        self.path.clear();
        self.finishing = false;
        self.recording = false;
        self.recording_changed();
    }

    /// Re-read the duration, and finish a stop that is under way.
    pub fn poll(&mut self) {
        if !self.recording {
            return;
        }
        let handle = self.recorder();
        let (duration, status, state, error) = read_progress(handle);
        if duration != self.duration_ms {
            self.duration_ms = duration;
            self.duration_changed();
        }
        if !error.is_empty() {
            let _ = std::fs::remove_file(&self.path);
            self.path.clear();
            self.finishing = false;
            self.recording = false;
            self.recording_changed();
            self.error(error.into());
            return;
        }
        if self.finishing && state != RECORDING_STATE && status != FINALIZING_STATUS {
            // Where the backend put it, which is where it was asked to
            // unless the backend had its own idea about the extension.
            let actual = read_actual_location(handle);
            let path = if actual.is_empty() {
                std::mem::take(&mut self.path)
            } else {
                self.path.clear();
                actual
            };
            self.finishing = false;
            self.recording = false;
            self.recording_changed();
            if std::fs::metadata(&path).map_or(0, |meta| meta.len()) == 0 {
                let _ = std::fs::remove_file(&path);
                self.error(QString::from("nothing was recorded"));
            } else {
                self.recorded(path.into());
            }
        }
    }
}

impl Drop for VoiceRecorder {
    fn drop(&mut self) {
        if self.handle == 0 {
            return;
        }
        let recorder = self.handle as *mut c_void;
        // SAFETY: the recorder was made by `recorder()` and is deleted
        // exactly once, here; the handle is zeroed so nothing can reach
        // it afterwards.
        cpp!(unsafe [recorder as "QAudioRecorder*"] {
            recorder->stop();
            delete recorder;
        });
        self.handle = 0;
    }
}

/// The lists a recorder has: what it can write, and what it can read.
#[derive(Clone, Copy)]
enum ListKind {
    Codecs,
    Containers,
    Inputs,
}

/// One of the recorder's lists, by name.
fn read_list(handle: usize, kind: ListKind) -> Vec<String> {
    let recorder = handle as *mut c_void;
    let kind = kind as i32;
    // SAFETY: `recorder` is a live QAudioRecorder owned by the caller's
    // object; the answer is joined into one QString owned by this frame.
    let joined = cpp!(unsafe [recorder as "QAudioRecorder*", kind as "int"] -> QString as "QString" {
        QStringList names;
        switch (kind) {
        case 0: names = recorder->supportedAudioCodecs(); break;
        case 1: names = recorder->supportedContainers(); break;
        default: names = recorder->audioInputs(); break;
        }
        return names.join(QLatin1Char('\n'));
    });
    joined
        .to_string()
        .lines()
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// The input the recorder would use left to itself.
fn read_default_input(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let name = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        return recorder->defaultAudioInput();
    });
    name.to_string()
}

/// Tell the recorder which input to record from.
fn set_input(handle: usize, input: &str) {
    let recorder = handle as *mut c_void;
    let input = QString::from(input);
    // SAFETY: as `read_list`; the QString is owned by this frame and
    // read by value.
    cpp!(unsafe [recorder as "QAudioRecorder*", input as "QString"] {
        recorder->setAudioInput(input);
    });
}

/// Pick the audio input from what the recorder offers.
///
/// The sound server by name where it is offered: on a device that is
/// `PulseAudio`, which is what the microphone is reached through. The
/// backend's own default is a `GStreamer` element that picks a source
/// for itself, and a recording made through it came out silent on a
/// phone. Failing that, the backend's default, and failing that the
/// first thing on the list. Nothing offered is `None`, and the recorder
/// is left alone.
fn choose_input(inputs: &[String], default: &str) -> Option<String> {
    inputs
        .iter()
        .find(|name| name.to_ascii_lowercase().contains("pulse"))
        .or_else(|| inputs.iter().find(|name| name.as_str() == default))
        .or_else(|| inputs.first())
        .cloned()
}

/// Ask the recorder to stop. The file is finished a moment later.
fn stop_recording(handle: usize) {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    cpp!(unsafe [recorder as "QAudioRecorder*"] {
        recorder->stop();
    });
}

/// Milliseconds recorded, the status, the state, and the error message
/// when there is one.
fn read_progress(handle: usize) -> (u32, i32, i32, String) {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let duration = cpp!(unsafe [recorder as "QAudioRecorder*"] -> i64 as "qint64" {
        return recorder->duration();
    });
    let status = cpp!(unsafe [recorder as "QAudioRecorder*"] -> i32 as "int" {
        return static_cast<int>(recorder->status());
    });
    let state = cpp!(unsafe [recorder as "QAudioRecorder*"] -> i32 as "int" {
        return static_cast<int>(recorder->state());
    });
    let duration = u32::try_from(duration.max(0)).unwrap_or(u32::MAX);
    (duration, status, state, read_error(handle))
}

/// The recorder's error message, empty when it has none.
fn read_error(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let message = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        if (recorder->error() == QMediaRecorder::NoError) {
            return QString();
        }
        QString text = recorder->errorString();
        return text.isEmpty() ? QStringLiteral("recording failed") : text;
    });
    message.to_string()
}

/// Where the recorder wrote, once it has: the local path, or empty.
fn read_actual_location(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let location = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        return recorder->actualLocation().toLocalFile();
    });
    location.to_string()
}

#[cfg(test)]
mod tests {
    use super::{choose_format, choose_input};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn opus_in_ogg_is_chosen_where_it_can_be_and_the_list_walked_otherwise() {
        // What a Sailfish GStreamer reports, near enough.
        let codecs = names(&[
            "audio/x-flac",
            "audio/x-vorbis",
            "audio/x-opus",
            "audio/mpeg, mpegversion=(int)4",
        ]);
        let containers = names(&[
            "audio/ogg",
            "video/quicktime, variant=(string)iso",
            "audio/x-wav",
        ]);
        assert_eq!(
            choose_format(&codecs, &containers),
            Some(("audio/x-opus".into(), "audio/ogg".into(), "ogg"))
        );
        // No opus: vorbis, still in ogg.
        let without_opus = names(&["audio/x-vorbis", "audio/mpeg, mpegversion=(int)4"]);
        assert_eq!(
            choose_format(&without_opus, &containers),
            Some(("audio/x-vorbis".into(), "audio/ogg".into(), "ogg"))
        );
        // A codec whose container is missing is skipped for one that fits.
        let no_ogg = names(&["video/quicktime, variant=(string)iso", "audio/x-wav"]);
        assert_eq!(
            choose_format(&codecs, &no_ogg),
            Some((
                "audio/mpeg, mpegversion=(int)4".into(),
                "video/quicktime, variant=(string)iso".into(),
                "m4a"
            ))
        );
        // Nothing offered, as headlessly: nothing chosen.
        assert_eq!(choose_format(&[], &containers), None);
        assert_eq!(choose_format(&codecs, &[]), None);
    }

    #[test]
    fn the_sound_server_is_the_input_where_it_is_offered() {
        // What Qt's GStreamer backend lists on a device: its own
        // automatic source first, then the sound server, then ALSA.
        let inputs = names(&["default:", "pulseaudio:", "alsa:hw:0,0"]);
        assert_eq!(
            choose_input(&inputs, "default:"),
            Some("pulseaudio:".to_string())
        );
        // No sound server: the backend's default, where it is listed.
        let without = names(&["alsa:hw:0,0", "default:"]);
        assert_eq!(
            choose_input(&without, "default:"),
            Some("default:".to_string())
        );
        // A default that is not on the list: the first thing that is.
        assert_eq!(
            choose_input(&without, "oss:"),
            Some("alsa:hw:0,0".to_string())
        );
        // Nothing at all, as headlessly: nothing chosen.
        assert_eq!(choose_input(&[], "default:"), None);
    }
}
