use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::str::FromStr;

/// Sample format of the headerless mono audio read by `--stdin`
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum StdinFormat {
    /// G.711 u-law, 8 bit. What a browser sends through a go2rtc
    /// `#backchannel=1#audio=pcmu/8000` exec line
    Pcmu,
    /// G.711 A-law, 8 bit (`#audio=pcma/8000`)
    Pcma,
    /// Signed 16 bit little endian PCM (`#audio=s16le/16000`)
    S16le,
}

/// The talk command will send audio for the camera to say
///
/// This data should be encoded in a way that gstreamer can understand.
/// This should be ok with most common formats.
///
/// `gst-launch` can be used to prepare this data
#[derive(Parser, Debug)]
pub struct Opt {
    /// The name of the camera to talk through. Must be a name in the config
    pub camera: String,
    /// The path to the audio file.
    #[arg(short, long, value_parser = PathBuf::from_str, conflicts_with = "microphone")]
    pub file_path: Option<PathBuf>,
    /// Use the microphone as the source. Defaults to autoaudiosrc - Which microphone depends
    /// on [gstreamer](https://gstreamer.freedesktop.org/documentation/autodetect/autoaudiosrc.html?gi-language=c#autoaudiosrc-page)
    #[arg(short, long, conflicts_with = "file_path")]
    pub microphone: bool,
    /// Use a specific microphone like "alsasrc device=hw:1"
    #[arg(
        short,
        long,
        default_value = "autoaudiosrc",
        conflicts_with = "file_path"
    )]
    pub input_src: String,
    /// Use to change the volume of the input
    #[arg(short, long, default_value = "1.0")]
    pub volume: f32,
    /// Read headerless mono audio from stdin. Built for go2rtc `exec:`
    /// backchannel lines, which write the viewer's audio to the child's stdin.
    /// The camera is not contacted until the first byte arrives, and the
    /// process exits when stdin closes
    #[arg(long, conflicts_with_all = ["file_path", "microphone"])]
    pub stdin: bool,
    /// Sample format of the audio arriving on stdin
    #[arg(long, value_enum, default_value_t = StdinFormat::Pcma)]
    pub stdin_format: StdinFormat,
    /// Sample rate in Hz of the audio arriving on stdin
    #[arg(long, default_value_t = 8000)]
    pub stdin_rate: u32,
}
