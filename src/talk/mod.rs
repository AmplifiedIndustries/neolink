///
/// # Neolink Talk
///
/// This module can be used to send adpcm data for the camera to play
///
/// The adpcm data needs to be in DVI-4 layout
///
/// # Usage
///
/// ```bash
/// neolink talk --config=config.toml --adpcm-file=data.adpcm --sample-rate=16000 --block-size=512 CameraName
/// ```
///
use anyhow::{anyhow, Context, Result};
use neolink_core::bc::xml::TalkConfig;

mod cmdline;
mod gst;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Entry point for the talk subcommand
///
/// Opt is the command line options
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    if opt.stdin {
        // go2rtc spawns this child on every Frigate camera-page probe and
        // SIGKILLs it milliseconds later. reactor.get() below already opens
        // the BC connection, so do not touch the camera until there is audio.
        let has_audio = tokio::task::spawn_blocking(wait_for_stdin)
            .await
            .context("stdin poll task failed")?
            .context("Failed to poll stdin")?;
        if !has_audio {
            log::info!("stdin closed before any audio arrived, nothing to say");
            return Ok(());
        }
    }

    let camera = reactor.get(&opt.camera).await?;
    let config = camera.config().await?.borrow().clone();
    let name = config.name.clone();

    let talk_ability = camera
        .run_task(|cam| {
            Box::pin(async move {
                let talk_ability = cam.talk_ability().await?;
                Ok(talk_ability)
            })
        })
        .await
        .with_context(|| format!("Camera {} does not support talk", name))?;

    if talk_ability.duplex_list.is_empty()
        || talk_ability.audio_stream_mode_list.is_empty()
        || talk_ability.audio_config_list.is_empty()
    {
        return Err(anyhow!("Camera {} does not support talk", name));
    }

    // Just copy that data from the first talk ability in the config have never seen more
    // than one ability
    let config_id = 0;

    let talk_config = TalkConfig {
        channel_id: config.channel_id,
        duplex: talk_ability.duplex_list[config_id].duplex.clone(),
        audio_stream_mode: talk_ability.audio_stream_mode_list[config_id]
            .audio_stream_mode
            .clone(),
        audio_config: talk_ability.audio_config_list[config_id]
            .audio_config
            .clone(),
        ..Default::default()
    };

    let block_size = (talk_config.audio_config.length_per_encoder / 2) + 4;
    let sample_rate = talk_config.audio_config.sample_rate;
    if block_size == 0 || sample_rate == 0 {
        return Err(anyhow!(
            "The camera {} does not support talk with adpcm",
            name
        ));
    }

    let (mut set, rx) = match (&opt.file_path, opt.microphone, opt.stdin) {
        (Some(path), false, false) => gst::from_input(
            &format!(
                "filesrc location={}",
                path.to_str().expect("File path not UTF8 complient")
            ),
            opt.volume,
            block_size,
            sample_rate,
        )
        .with_context(|| format!("Failed to setup gst with the file: {:?}", path))?,
        (None, true, false) => gst::from_input(&opt.input_src, opt.volume, block_size, sample_rate)
            .context("Failed to setup gst using the microphone")?,
        (None, false, true) => {
            // go2rtc kept writing to our stdin while we spent seconds logging
            // in above. talk_stream is paced by the camera's acks and never
            // catches up, so anything already in the pipe would delay the
            // whole session by that much. Throw it away.
            drain_stdin().context("Failed to drain stdin before talk start")?;
            gst::from_input(
                &gst::stdin_src(opt.stdin_format, opt.stdin_rate),
                opt.volume,
                block_size,
                sample_rate,
            )
            .context("Failed to setup gst reading from stdin")?
        }
        _ => return Err(anyhow!("Use one of --file-path, --microphone or --stdin")),
    };

    camera
        .run_task(|cam| {
            let rx = rx.clone();
            let talk_config = talk_config.clone();
            Box::pin(async move {
                cam.talk_stream(rx, talk_config).await?;
                Ok(())
            })
        })
        .await
        .context("Talk stream ended early")?;

    drop(rx);
    while set.join_next().await.is_some() {}

    Ok(())
}

/// Block until stdin has data (`true`) or was closed without any (`false`).
/// Consumes nothing, so the audio is still there for the gst source.
fn wait_for_stdin() -> Result<bool> {
    let mut pfd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // SAFETY: one initialised pollfd, nfds 1, no timeout
        let n = unsafe { libc::poll(&mut pfd, 1, -1) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err.into());
        }
        if pfd.revents & libc::POLLIN != 0 {
            return Ok(true);
        }
        if pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            return Ok(false);
        }
    }
}

/// Discard whatever is already buffered on fd 0 without blocking, then put
/// the descriptor back the way it was so `fdsrc` sees a normal blocking fd.
fn drain_stdin() -> Result<()> {
    let fd = libc::STDIN_FILENO;
    // SAFETY: plain fcntl/read calls on our own stdin with a valid buffer
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let restore = || unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };

    let mut buf = [0u8; 4096];
    let mut dropped = 0usize;
    let result = loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n > 0 {
            dropped += n as usize;
            continue;
        }
        if n == 0 {
            // Already at EOF: fdsrc will see the same and end cleanly
            break Ok(());
        }
        let err = std::io::Error::last_os_error();
        match err.kind() {
            std::io::ErrorKind::WouldBlock => break Ok(()),
            std::io::ErrorKind::Interrupted => continue,
            _ => break Err(err.into()),
        }
    };
    restore();
    if dropped > 0 {
        log::info!("Discarded {dropped} bytes of stdin audio that arrived during login");
    }
    result
}
