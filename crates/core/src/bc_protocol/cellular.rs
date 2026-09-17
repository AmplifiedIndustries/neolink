use super::{BcCamera, Error, Result};
use crate::bc::{model::*, xml::*};

impl BcCamera {
    /// Read the cellular link status: signal, radio mode and operator.
    pub async fn get_cellular_status(&self) -> Result<Net3g4gInfo> {
        let xml = self.get_cellular_message(MSG_ID_GET_NET_3G4G_INFO).await?;
        xml.net_3g4g_info.ok_or(Error::Other("Expected a Net3g4gInfo in the reply"))
    }

    /// Read the cellular module's identity: ICCID, IMEI and phone number.
    pub async fn get_cellular_module(&self) -> Result<Net3g4gModuleInfo> {
        let xml = self
            .get_cellular_message(MSG_ID_GET_NET_3G4G_MODULE_INFO)
            .await?;
        xml.net_3g4g_module_info
            .ok_or(Error::Other("Expected a Net3g4gModuleInfo in the reply"))
    }

    /// Send one bodyless cellular read and return the reply's XML.
    ///
    /// Both cellular messages are plain reads with the same shape, so they share this rather than
    /// each repeating the envelope.
    async fn get_cellular_message(&self, msg_id: u32) -> Result<BcXml> {
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
                why: "Expected a cellular xml in the reply",
            })
        }
    }
}
