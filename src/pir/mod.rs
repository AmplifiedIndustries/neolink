///
/// # Neolink PIR
///
/// This module handles the controls of the pir sensor alarm
///
///
/// # Usage
///
/// ```bash
/// # To turn the pir sensor on
/// neolink pir --config=config.toml CameraName on
/// # Or off
/// neolink pir --config=config.toml CameraName off
/// # To change the sensitivity, with or without a state change
/// neolink pir --config=config.toml CameraName --sensitivity 30
/// # To read the current configuration
/// neolink pir --config=config.toml CameraName
/// ```
///
use anyhow::{Context, Result};

mod cmdline;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Entry point for the pir subcommand
///
/// Opt is the command line options
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    let camera = reactor.get(&opt.camera).await?;

    if opt.on.is_some() || opt.sensitivity.is_some() {
        // One get-modify-set for both fields: they live in the same rfAlarmCfg message, so
        // applying them separately would wake the camera twice and race on the second read.
        let (on, sensitivity) = (opt.on, opt.sensitivity);
        camera
            .run_task(move |cam| {
                Box::pin(async move {
                    let mut pir_state = cam
                        .get_pirstate()
                        .await
                        .context("Unable to read camera PIR state before changing it")?;
                    if let Some(on) = on {
                        pir_state.enable = u8::from(on);
                    }
                    if let Some(sensitivity) = sensitivity {
                        pir_state.sensiValue = sensitivity;
                    }
                    cam.set_pirstate(pir_state)
                        .await
                        .context("Unable to set camera PIR state")
                })
            })
            .await?;
    } else {
        let pir_state = camera
            .run_task(|cam| {
                Box::pin(async move {
                    cam.get_pirstate()
                        .await
                        .context("Unable to get camera PIR state")
                })
            })
            .await?;
        let pir_ser = String::from_utf8(
            {
                let mut buf = bytes::BytesMut::new();
                quick_xml::se::to_writer(&mut buf, &pir_state).map(|_| buf.to_vec())
            }
            .expect("Should Ser the struct"),
        )
        .expect("Should be UTF8");
        println!("{}", pir_ser);
    }

    Ok(())
}
