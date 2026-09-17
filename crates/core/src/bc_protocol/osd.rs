use super::{BcCamera, Error, Result};
use crate::bc::{model::*, xml::*};

/// The on-screen display of a camera: its name, the timestamp, and the logo watermark.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OsdConfig {
    /// The timestamp overlay, when the camera reports one
    pub datetime: Option<OsdDatetime>,
    /// The name overlay and the watermark flag, when the camera reports them
    pub channel_name: Option<OsdChannelName>,
}

impl BcCamera {
    /// Read the on-screen display configuration.
    pub async fn get_osd(&self) -> Result<OsdConfig> {
        let connection = self.get_connection();
        let msg_num = self.new_message_num();
        let mut sub = connection.subscribe(MSG_ID_GET_OSD, msg_num).await?;

        let get = Bc {
            meta: BcMeta {
                msg_id: MSG_ID_GET_OSD,
                channel_id: self.channel_id,
                msg_num,
                response_code: 0,
                stream_type: 0,
                class: 0x6414,
            },
            body: BcBody::ModernMsg(ModernMsg {
                extension: Some(Extension {
                    channel_id: Some(self.channel_id),
                    ..Default::default()
                }),
                payload: None,
            }),
        };

        sub.send(get).await?;
        let msg = sub.recv().await?;
        if msg.meta.response_code != 200 {
            return Err(Error::CameraServiceUnavailable {
                id: msg.meta.msg_id,
                code: msg.meta.response_code,
            });
        }

        if let BcBody::ModernMsg(ModernMsg {
            payload: Some(BcPayloads::BcXml(xml)),
            ..
        }) = msg.body
        {
            Ok(OsdConfig {
                datetime: xml.osd_datetime,
                channel_name: xml.osd_channel_name,
            })
        } else {
            Err(Error::UnintelligibleReply {
                reply: std::sync::Arc::new(Box::new(msg)),
                why: "Expected an OSD xml in the reply",
            })
        }
    }

    /// Write the on-screen display configuration back.
    ///
    /// *msg_id* is a parameter rather than a constant because the setter's id is not published
    /// anywhere: every other pair in this protocol is get+1, so [`MSG_ID_SET_OSD`] assumes that,
    /// and being able to try another one without a rebuild is what makes the assumption testable.
    ///
    /// Both halves are sent together, as the camera returns them, so writing the name cannot drop
    /// the timestamp overlay.
    pub async fn set_osd(&self, config: OsdConfig, msg_id: u32) -> Result<()> {
        let connection = self.get_connection();
        let msg_num = self.new_message_num();
        let mut sub = connection.subscribe(msg_id, msg_num).await?;

        let set = Bc {
            meta: BcMeta {
                msg_id,
                channel_id: self.channel_id,
                msg_num,
                response_code: 0,
                stream_type: 0,
                class: 0x6414,
            },
            body: BcBody::ModernMsg(ModernMsg {
                extension: Some(Extension {
                    channel_id: Some(self.channel_id),
                    ..Default::default()
                }),
                payload: Some(BcPayloads::BcXml(BcXml {
                    osd_datetime: config.datetime,
                    osd_channel_name: config.channel_name,
                    ..Default::default()
                })),
            }),
        };

        sub.send(set).await?;
        // A settings write is answered by some cameras and silently accepted by others, the same
        // as the PIR: a timeout is not treated as a failure.
        if let Ok(reply) =
            tokio::time::timeout(tokio::time::Duration::from_millis(1500), sub.recv()).await
        {
            let msg = reply?;
            if msg.meta.response_code != 200 {
                return Err(Error::CameraServiceUnavailable {
                    id: msg.meta.msg_id,
                    code: msg.meta.response_code,
                });
            }
        }
        Ok(())
    }
}
