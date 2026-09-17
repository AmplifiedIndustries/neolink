///
/// # Neolink AI detection
///
/// Reads and changes the per-object-type AI detection settings: how sensitive the camera is to a
/// person, a vehicle, and whatever other types its firmware supports, plus how long the object
/// must stay before it is reported.
///
/// The message ids and schema come from reolink_aio's Baichuan implementation rather than from a
/// packet capture. Which cameras actually answer is what this command establishes.
///
/// # Usage
///
/// ```bash
/// # Read the person settings
/// neolink ai-detect --config=config.toml CameraName --ai-type person
/// # Change the vehicle sensitivity
/// neolink ai-detect --config=config.toml CameraName --ai-type vehicle --sensitivity 40
/// ```
use anyhow::{Context, Result};

mod cmdline;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Entry point for the ai-detect subcommand
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    let camera = reactor.get(&opt.camera).await?;
    let ai_type = opt.ai_type.clone();

    if opt.sensitivity.is_some() || opt.delay.is_some() {
        // Read, change only what was asked for, and send the rest back untouched: anything
        // dropped here is a camera setting silently reset.
        let (sensitivity, delay) = (opt.sensitivity, opt.delay);
        camera
            .run_task(move |cam| {
                let ai_type = ai_type.clone();
                Box::pin(async move {
                    let mut cfg = cam
                        .get_ai_detect_cfg(&ai_type)
                        .await
                        .context("Unable to read the camera AI detection config before changing it")?;
                    if let Some(sensitivity) = sensitivity {
                        cfg.sensitivity = Some(sensitivity);
                    }
                    if let Some(delay) = delay {
                        cfg.stay_time = Some(delay);
                    }
                    cam.set_ai_detect_cfg(cfg)
                        .await
                        .context("Unable to set the camera AI detection config")
                })
            })
            .await?;
    } else {
        let cfg = camera
            .run_task(move |cam| {
                let ai_type = ai_type.clone();
                Box::pin(async move {
                    cam.get_ai_detect_cfg(&ai_type)
                        .await
                        .context("Unable to read the camera AI detection config")
                })
            })
            .await?;
        let serialised = String::from_utf8(
            {
                let mut buf = bytes::BytesMut::new();
                quick_xml::se::to_writer(&mut buf, &cfg).map(|_| buf.to_vec())
            }
            .expect("Should Ser the struct"),
        )
        .expect("Should be UTF8");
        println!("{}", serialised);
    }

    Ok(())
}
