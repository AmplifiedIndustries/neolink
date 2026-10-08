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
use futures::{future::BoxFuture, stream, FutureExt, StreamExt};

mod cmdline;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Reads in flight at once. Each one is a round trip through the relay, so sending them together
/// costs about as long as the slowest; the cap keeps a slow camera firmware from being flooded.
const MAX_IN_FLIGHT: usize = 4;

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
                // Every read below is independent and answered by its own message number, so they
                // are sent together rather than one round trip at a time. Output keeps this order.
                let mut optional: Vec<BoxFuture<'_, Vec<String>>> = vec![
                    // The PIR is not fatal: a camera without one still has a battery worth reporting.
                    async {
                        match cam.get_pirstate().await {
                            Ok(pir) => vec![to_xml(&pir)],
                            Err(err) => {
                                log::info!("No PIR configuration on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                    // FTP: the upload period and which triggers upload.
                    async {
                        match cam.get_ftp().await {
                            Ok(ftp) => vec![to_xml(&ftp)],
                            Err(err) => {
                                log::info!("No FTP configuration on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                    async {
                        match cam.get_ftp_task().await {
                            Ok(task) => vec![to_xml(&task)],
                            Err(err) => {
                                log::info!("No FTP trigger schedule on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                    // Anti-flicker, out of the wider image settings.
                    async {
                        match cam.get_isp().await {
                            Ok(isp) => vec![to_xml(&isp)],
                            Err(err) => {
                                log::info!("No image settings on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                    // Cellular: signal on a 4G camera, and the SIM and modem identifiers. A camera
                    // on wifi answers neither, which is not an error.
                    async {
                        match cam.get_cellular_status().await {
                            Ok(info) => vec![to_xml(&info)],
                            Err(err) => {
                                log::info!("No cellular status on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                    async {
                        match cam.get_cellular_module().await {
                            Ok(info) => vec![to_xml(&info)],
                            Err(err) => {
                                log::info!("No cellular module info on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                    // The video overlay: the camera's own name and the logo watermark.
                    async {
                        match cam.get_osd().await {
                            Ok(osd) => {
                                let mut parts = vec![];
                                if let Some(channel_name) = osd.channel_name.as_ref() {
                                    parts.push(to_xml(channel_name));
                                }
                                if let Some(datetime) = osd.datetime.as_ref() {
                                    parts.push(to_xml(datetime));
                                }
                                parts
                            }
                            Err(err) => {
                                log::info!("No OSD configuration on this camera: {:?}", err);
                                vec![]
                            }
                        }
                    }
                    .boxed(),
                ];
                for ai_type in ai_types.iter().cloned() {
                    optional.push(
                        async move {
                            match cam.get_ai_detect_cfg(&ai_type).await {
                                Ok(cfg) => vec![to_xml(&cfg)],
                                Err(err) => {
                                    log::info!("No AI type {} on this camera: {:?}", ai_type, err);
                                    vec![]
                                }
                            }
                        }
                        .boxed(),
                    );
                }

                let (battery, rest) = futures::join!(
                    cam.battery_info(),
                    stream::iter(optional)
                        .buffered(MAX_IN_FLIGHT)
                        .collect::<Vec<_>>()
                );
                let battery = battery.context("Unable to get camera Battery state")?;
                let mut parts: Vec<String> = vec![to_xml(&battery)];
                parts.extend(rest.into_iter().flatten());

                Ok(parts)
            })
        })
        .await?;

    for part in parts {
        println!("{}", part);
    }

    Ok(())
}
