use crate::AnyResult;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use gstreamer::{
    element_error, parse::launch_full, prelude::*, Caps, ClockTime, FlowError, FlowSuccess,
    MessageView, ParseFlags, Pipeline, ResourceError, State,
};
use gstreamer_app::{AppSink, AppSinkCallbacks};
use tokio::task::JoinSet;

use byte_slice_cast::*;

use super::cmdline::StdinFormat;

/// gst source string for headerless audio arriving on stdin (fd 0)
pub(super) fn stdin_src(format: StdinFormat, rate: u32) -> String {
    raw_src("fdsrc fd=0", format, rate)
}

/// Wrap a headerless byte source in `rawaudioparse` so the rest of the
/// pipeline (`decodebin ! audioconvert ! ...`) sees typed, timestamped audio.
///
/// `blocksize` is 20 ms of audio, the size of one RTP audio packet, so the
/// source never sits on more than one packet before handing it on.
fn raw_src(source: &str, format: StdinFormat, rate: u32) -> String {
    let (parse, bytes_per_sample) = match format {
        StdinFormat::Pcmu => ("format=mulaw", 1),
        StdinFormat::Pcma => ("format=alaw", 1),
        StdinFormat::S16le => ("format=pcm pcm-format=s16le", 2),
    };
    let blocksize = rate / 50 * bytes_per_sample;
    format!(
        "{} blocksize={} ! rawaudioparse use-sink-caps=false {} sample-rate={} num-channels=1",
        source, blocksize, parse, rate
    )
}

#[allow(clippy::type_complexity)]
pub(super) fn from_input(
    input_src: &str,
    volume: f32,
    block_align: u16,
    sample_rate: u16,
) -> Result<(JoinSet<AnyResult<()>>, Receiver<Vec<u8>>)> {
    let pipeline = create_pipeline(input_src, volume, block_align, sample_rate)?;
    input(pipeline)
}

#[allow(clippy::type_complexity)]
fn input(pipeline: Pipeline) -> Result<(JoinSet<AnyResult<()>>, Receiver<Vec<u8>>)> {
    let appsink = get_sink(&pipeline)?;
    let (tx, rx) = bounded(30);
    let mut set = JoinSet::<AnyResult<()>>::new();
    set_data_channel(&appsink, tx);

    set.spawn_blocking(move || {
        let _ = start_pipeline(pipeline);
        AnyResult::Ok(())
    });

    Ok((set, rx))
}

fn start_pipeline(pipeline: Pipeline) -> Result<()> {
    pipeline.set_state(State::Playing)?;

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    for msg in bus.iter_timed(ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(..) => break,
            MessageView::Error(err) => {
                pipeline
                    .set_state(State::Null)
                    .context("Error in gstreamer when setting state to Null")?;
                log::warn!(
                    "Error from gstreamer when setting the play state {:?} setting to Null instead",
                    err
                );
            }
            _ => (),
        }
    }

    pipeline
        .set_state(State::Null)
        .context("Error in gstreamer when setting state to Null")?;

    Ok(())
}

fn get_sink(pipeline: &Pipeline) -> Result<AppSink> {
    let sink = pipeline
        .by_name("thesink")
        .expect("There shoud be a `thesink`");
    sink.dynamic_cast::<AppSink>()
        .map_err(|_| anyhow!("Cannot find appsink in gstreamer, check your gstreamer plugins"))
}

fn set_data_channel(appsink: &AppSink, tx: Sender<Vec<u8>>) {
    // Getting data out of the appsink is done by setting callbacks on it.
    // The appsink will then call those handlers, as soon as data is available.
    appsink.set_callbacks(
        AppSinkCallbacks::builder()
            // Add a handler to the "new-sample" signal.
            .new_sample(move |appsink| {
                // Pull the sample in question out of the appsink's buffer.
                let sample = appsink.pull_sample().map_err(|_| FlowError::Eos)?;
                let buffer = sample.buffer().ok_or_else(|| {
                    element_error!(
                        appsink,
                        ResourceError::Failed,
                        ("Failed to get buffer from appsink")
                    );

                    FlowError::Error
                })?;

                // At this point, buffer is only a reference to an existing memory region somewhere.
                // When we want to access its content, we have to map it while requesting the required
                // mode of access (read, read/write).
                // This type of abstraction is necessary, because the buffer in question might not be
                // on the machine's main memory itself, but rather in the GPU's memory.
                // So mapping the buffer makes the underlying memory region accessible to us.
                // See: https://gstreamer.freedesktop.org/documentation/plugin-development/advanced/allocation.html
                let map = buffer.map_readable().map_err(|_| {
                    element_error!(
                        appsink,
                        ResourceError::Failed,
                        ("Failed to map buffer readable")
                    );

                    FlowError::Error
                })?;

                // We know what format the data in the memory region has, since we requested
                // it by setting the appsink's caps. So what we do here is interpret the
                // memory region we mapped as an array of signed 8 bit integers.
                let samples = map.as_slice_of::<u8>().map_err(|_| {
                    element_error!(
                        appsink,
                        ResourceError::Failed,
                        ("Failed to interprete buffer as u8 ADPCM")
                    );

                    FlowError::Error
                })?;

                // Ready!
                let _ = tx.send(samples.to_vec());

                Ok(FlowSuccess::Ok)
            })
            .build(),
    );
}

fn create_pipeline(
    source: &str,
    volume: f32,
    block_align: u16,
    sample_rate: u16,
) -> Result<Pipeline> {
    gstreamer::init()
        .context("Unable to start gstreamer ensure it and all plugins are installed")?;

    let launch_str = format!(
        "{} \
        ! decodebin \
        ! audioconvert \
        ! audioresample \
        ! audio/x-raw,rate={},channels=1 \
        ! volume volume={:.2} \
        ! queue  \
        ! adpcmenc blockalign={} layout=dvi \
        ! appsink name=thesink",
        source, sample_rate, volume, block_align
    );

    log::info!("{}", launch_str);

    // Parse the pipeline we want to probe from a static in-line string.
    // Here we give our audiotestsrc a name, so we can retrieve that element
    // from the resulting pipeline.
    let pipeline = launch_full(&launch_str, None, ParseFlags::empty())
        .context("Unable to load gstreamer pipeline ensure all gstramer plugins are installed")?;
    let pipeline = pipeline.dynamic_cast::<Pipeline>().map_err(|_| {
        anyhow!("Unable to create gstreamer pipeline ensure all gstramer plugins are installed")
    })?;

    let appsink = get_sink(&pipeline)?;

    // Tell the appsink what format we want. It will then be the audiotestsrc's job to
    // provide the format we request.
    // This can be set after linking the two objects, because format negotiation between
    // both elements will happen during pre-rolling of the pipeline.
    appsink.set_caps(Some(
        &Caps::builder("audio/x-adpcm")
            .field("layout", "dvi")
            .field("block_align", block_align as i32)
            .field("channels", 1i32)
            .field("rate", sample_rate as i32)
            .build(),
    ));

    Ok(pipeline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    /// Camera-side framing used by every Reolink seen so far:
    /// length_per_encoder 1024 -> 512 ADPCM bytes + 4 byte DVI4 header
    const BLOCK: u16 = 516;
    const CAM_RATE: u16 = 16000;

    #[test]
    fn stdin_src_pcmu_string() {
        assert_eq!(
            stdin_src(StdinFormat::Pcmu, 8000),
            "fdsrc fd=0 blocksize=160 ! rawaudioparse use-sink-caps=false format=mulaw \
             sample-rate=8000 num-channels=1"
        );
    }

    #[test]
    fn stdin_src_s16le_string() {
        assert_eq!(
            stdin_src(StdinFormat::S16le, 16000),
            "fdsrc fd=0 blocksize=640 ! rawaudioparse use-sink-caps=false format=pcm \
             pcm-format=s16le sample-rate=16000 num-channels=1"
        );
    }

    /// Push `bytes` through the real pipeline from a file and return the size
    /// of every chunk the appsink produced. Needs the gst plugins
    /// (coreelements, rawparse, mulaw, alaw, audioconvert, audioresample,
    /// volume, playback, adpcmenc, app), so this runs in the Docker test image.
    async fn encode(bytes: &[u8], format: StdinFormat, rate: u32) -> Vec<usize> {
        let path = std::env::temp_dir().join(format!(
            "neolink-talk-{}-{:?}.raw",
            std::process::id(),
            format
        ));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        let source = raw_src(
            &format!("filesrc location={}", path.display()),
            format,
            rate,
        );

        let (mut set, rx) = from_input(&source, 1.0, BLOCK, CAM_RATE).unwrap();
        let mut lens = vec![];
        // The sender lives in the appsink callback and is dropped with the
        // pipeline after EOS, which is what ends this loop. The timeout only
        // guards against a wedged pipeline turning into a hung test.
        while let Ok(chunk) = rx.recv_timeout(Duration::from_secs(10)) {
            lens.push(chunk.len());
        }
        while set.join_next().await.is_some() {}
        let _ = std::fs::remove_file(&path);
        lens
    }

    /// One second in must come out as whole camera blocks: 16000 samples at
    /// 1025 samples per DVI4 block is 15 full blocks, 16 if the tail is padded.
    fn assert_one_second_of_blocks(lens: &[usize]) {
        assert!(!lens.is_empty(), "pipeline produced no ADPCM");
        for len in lens {
            assert_eq!(
                len % BLOCK as usize,
                0,
                "chunk of {len} bytes is not whole blocks"
            );
        }
        let blocks = lens.iter().sum::<usize>() / BLOCK as usize;
        assert!(
            (15..=16).contains(&blocks),
            "expected 15 or 16 blocks, got {blocks}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mulaw_8k_encodes_to_whole_blocks() {
        // 0xFF is u-law silence. 8000 bytes = 1 s at 8 kHz.
        let lens = encode(&[0xFFu8; 8000], StdinFormat::Pcmu, 8000).await;
        assert_one_second_of_blocks(&lens);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s16le_16k_encodes_to_whole_blocks() {
        // 32000 bytes = 16000 zero samples = 1 s at 16 kHz.
        let lens = encode(&[0u8; 32000], StdinFormat::S16le, 16000).await;
        assert_one_second_of_blocks(&lens);
    }
}
