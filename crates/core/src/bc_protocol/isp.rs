use super::{BcCamera, Error, Result};
use crate::bc::{model::*, xml::*};

impl BcCamera {
    /// Read the advanced image settings. Only the anti-flicker block is modelled; see
    /// [`InputAdvanceCfg`].
    pub async fn get_isp(&self) -> Result<InputAdvanceCfg> {
        let xml = self.get_config_message(MSG_ID_GET_ISP).await?;
        xml.input_advance_cfg
            .ok_or(Error::Other("Expected an InputAdvanceCfg in the reply"))
    }

    /// Write the advanced image settings back.
    pub async fn set_isp(&self, cfg: InputAdvanceCfg) -> Result<()> {
        self.set_config_message(
            MSG_ID_SET_ISP,
            BcXml {
                input_advance_cfg: Some(cfg),
                ..Default::default()
            },
        )
        .await
    }

    /// Send one bodyless read and return the reply's XML.
    ///
    /// Shared by every plain settings read added here, so the message envelope -- which is
    /// identical for all of them and easy to get subtly wrong -- exists once.
    pub(crate) async fn get_config_message(&self, msg_id: u32) -> Result<BcXml> {
        let connection = self.get_connection();
        let msg_num = self.new_message_num();
        let mut sub = connection.subscribe(msg_id, msg_num).await?;

        let get = Bc {
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
            Ok(xml)
        } else {
            Err(Error::UnintelligibleReply {
                reply: std::sync::Arc::new(Box::new(msg)),
                why: "Expected a settings xml in the reply",
            })
        }
    }

    /// Send one settings write and check the camera accepted it.
    ///
    /// A silent camera is treated as success, the same as the PIR write: several of these answer
    /// nothing at all on success.
    pub(crate) async fn set_config_message(&self, msg_id: u32, body: BcXml) -> Result<()> {
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
                payload: Some(BcPayloads::BcXml(body)),
            }),
        };

        sub.send(set).await?;
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
