///
/// # Neolink OSD
///
/// Reads and changes the on-screen display: the camera name burned into the video, whether it is
/// drawn, and the Reolink logo watermark.
///
/// The read message is 44 and its schema was established by probing a real camera, not taken from
/// another project -- no published implementation writes this over Baichuan. The **write** id is
/// therefore a working assumption (`--set-cmd-id`, default 45, on the get+1 pattern every other
/// pair here follows), which is why it is a flag: a wrong guess should cost a retry, not a rebuild.
///
/// # Usage
///
/// ```bash
/// # Read
/// neolink osd --config=config.toml CameraName
/// # Turn the logo watermark off
/// neolink osd --config=config.toml CameraName --watermark off
/// # Rename the camera as it appears on the video
/// neolink osd --config=config.toml CameraName --name "Eliot Club 1"
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

/// Entry point for the osd subcommand
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    let camera = reactor.get(&opt.camera).await?;
    let (name, watermark, show_name, set_cmd_id) =
        (opt.name.clone(), opt.watermark, opt.show_name, opt.set_cmd_id);

    if name.is_none() && watermark.is_none() && show_name.is_none() {
        let config = camera
            .run_task(|cam| {
                Box::pin(async move {
                    cam.get_osd()
                        .await
                        .context("Unable to read the camera OSD configuration")
                })
            })
            .await?;
        if let Some(datetime) = config.datetime.as_ref() {
            println!("{}", to_xml(datetime));
        }
        if let Some(channel_name) = config.channel_name.as_ref() {
            println!("{}", to_xml(channel_name));
        }
        return Ok(());
    }

    camera
        .run_task(move |cam| {
            let name = name.clone();
            Box::pin(async move {
                // Read, change only what was asked for, and send both halves back: writing the
                // name must not drop the timestamp overlay beside it.
                let mut config = cam
                    .get_osd()
                    .await
                    .context("Unable to read the camera OSD configuration before changing it")?;
                let channel_name = config.channel_name.as_mut().context(
                    "This camera reports no OsdChannelName, so it has no name or watermark to set",
                )?;
                if let Some(name) = name {
                    channel_name.name = Some(name);
                }
                if let Some(watermark) = watermark {
                    channel_name.en_watermark = Some(u8::from(watermark));
                }
                if let Some(show_name) = show_name {
                    channel_name.enable = Some(u8::from(show_name));
                }
                cam.set_osd(config, set_cmd_id)
                    .await
                    .context("Unable to set the camera OSD configuration")
            })
        })
        .await?;

    Ok(())
}
