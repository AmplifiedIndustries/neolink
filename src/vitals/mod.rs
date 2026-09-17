///
/// # Neolink Vitals
///
/// Reads the battery, the PIR configuration, the video overlay and the per-object-type AI
/// detection settings, and prints them as one XML document.
///
/// This exists for a single reason: **login is the expensive part**. Every other subcommand opens
/// its own session, and a battery camera takes tens of seconds to wake and complete the relay
/// handshake before it answers anything. Reading the same five things one subcommand at a time
/// costs five wakes and roughly a minute; reading them inside one session costs one wake and about
/// twenty seconds. On a solar-charged 4G camera that difference is the design.
///
/// An AI type the camera refuses is skipped rather than failing the whole read: which types a
/// model supports is a property of the camera, not an error. A type it does not know is refused
/// with the same code as an unsupported feature, so the two cannot be told apart here.
///
/// # Usage
///
/// ```bash
/// neolink vitals --config=config.toml CameraName
/// neolink vitals --config=config.toml CameraName --ai-types people,vehicle
/// ```
use anyhow::{Context, Result};

mod cmdline;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Serialise one struct to an XML fragment.
fn to_xml<T: serde::Serialize>(value: &T) -> String {
    String::from_utf8({
        let mut buf = bytes::BytesMut::new();
        quick_xml::se::to_writer(&mut buf, value).expect("Should Ser the struct");
        buf.to_vec()
    })
    .expect("Should be UTF8")
}

/// Entry point for the vitals subcommand
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    let camera = reactor.get(&opt.camera).await?;
    let ai_types: Vec<String> = opt
        .ai_types
        .split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect();

    let parts = camera
        .run_task(move |cam| {
            let ai_types = ai_types.clone();
            Box::pin(async move {
                let mut parts: Vec<String> = Vec::new();

                let battery = cam
                    .battery_info()
                    .await
                    .context("Unable to get camera Battery state")?;
                parts.push(to_xml(&battery));

                // The PIR is not fatal: a camera without one still has a battery worth reporting.
                match cam.get_pirstate().await {
                    Ok(pir) => parts.push(to_xml(&pir)),
                    Err(err) => log::info!("No PIR configuration on this camera: {:?}", err),
                }

                // FTP: the upload period and which triggers upload.
                match cam.get_ftp().await {
                    Ok(ftp) => parts.push(to_xml(&ftp)),
                    Err(err) => log::info!("No FTP configuration on this camera: {:?}", err),
                }
                match cam.get_ftp_task().await {
                    Ok(task) => parts.push(to_xml(&task)),
                    Err(err) => log::info!("No FTP trigger schedule on this camera: {:?}", err),
                }

                // Anti-flicker, out of the wider image settings.
                match cam.get_isp().await {
                    Ok(isp) => parts.push(to_xml(&isp)),
                    Err(err) => log::info!("No image settings on this camera: {:?}", err),
                }

                // Cellular: signal on a 4G camera, and the SIM and modem identifiers. A camera
                // on wifi answers neither, which is not an error.
                match cam.get_cellular_status().await {
                    Ok(info) => parts.push(to_xml(&info)),
                    Err(err) => log::info!("No cellular status on this camera: {:?}", err),
                }
                match cam.get_cellular_module().await {
                    Ok(info) => parts.push(to_xml(&info)),
                    Err(err) => log::info!("No cellular module info on this camera: {:?}", err),
                }

                // The video overlay: the camera's own name and the logo watermark.
                match cam.get_osd().await {
                    Ok(osd) => {
                        if let Some(channel_name) = osd.channel_name.as_ref() {
                            parts.push(to_xml(channel_name));
                        }
                        if let Some(datetime) = osd.datetime.as_ref() {
                            parts.push(to_xml(datetime));
                        }
                    }
                    Err(err) => log::info!("No OSD configuration on this camera: {:?}", err),
                }

                for ai_type in ai_types.iter() {
                    match cam.get_ai_detect_cfg(ai_type).await {
                        Ok(cfg) => parts.push(to_xml(&cfg)),
                        Err(err) => {
                            log::info!("No AI type {} on this camera: {:?}", ai_type, err)
                        }
                    }
                }

                Ok(parts)
            })
        })
        .await?;

    for part in parts {
        println!("{}", part);
    }

    Ok(())
}
