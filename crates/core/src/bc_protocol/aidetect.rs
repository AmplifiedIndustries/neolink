use super::{BcCamera, Error, Result};
use crate::bc::{model::*, xml::*};
use tokio::time::{interval, Duration};

/// A battery camera answers 400 while it is still waking, so a first 400 says nothing about
/// whether the message is supported. Matches what the PIR reader does.
const RETRY_ATTEMPTS: usize = 5;

impl BcCamera {
    /// Read the AI detection config for one object type, e.g. "person" or "vehicle".
    ///
    /// Deliberately performs no ability check. Which cameras carry this is exactly the open
    /// question, and an ability name guessed wrong would refuse a camera that in fact supports it;
    /// letting the camera answer for itself is both the test and the feature.
    pub async fn get_ai_detect_cfg(&self, ai_type: &str) -> Result<AiDetectCfg> {
        let connection = self.get_connection();
        let mut retries: usize = 0;
        let mut retry_interval = interval(Duration::from_millis(500));
        loop {
        retry_interval.tick().await;
        let msg_num = self.new_message_num();
        let mut sub = connection
            .subscribe(MSG_ID_GET_AI_DETECT_CFG, msg_num)
            .await?;

        let get = Bc {
            meta: BcMeta {
                msg_id: MSG_ID_GET_AI_DETECT_CFG,
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
                    ai_detect_cfg: Some(AiDetectCfg {
                        version: xml_ver(),
                        chn: Some(self.channel_id),
                        ai_type: Some(ai_type.to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                })),
            }),
        };

        sub.send(get).await?;
        let msg = sub.recv().await?;
        if msg.meta.response_code == 400 && retries < RETRY_ATTEMPTS {
            retries += 1;
            continue;
        }
        if msg.meta.response_code != 200 {
            return Err(Error::CameraServiceUnavailable {
                id: msg.meta.msg_id,
                code: msg.meta.response_code,
            });
        }

        return if let BcBody::ModernMsg(ModernMsg {
            payload:
                Some(BcPayloads::BcXml(BcXml {
                    ai_detect_cfg: Some(cfg),
                    ..
                })),
            ..
        }) = msg.body
        {
            Ok(cfg)
        } else {
            Err(Error::UnintelligibleReply {
                reply: std::sync::Arc::new(Box::new(msg)),
                why: "Expected an AiDetectCfg xml in the reply",
            })
        };
        }
    }

    /// Write an AI detection config back to the camera.
    ///
    /// Takes the whole struct rather than the individual values so a caller does a read, changes
    /// what it means to change, and sends the rest back untouched -- the same get-modify-set the
    /// PIR uses, and for the same reason: anything dropped here is a setting silently reset.
    pub async fn set_ai_detect_cfg(&self, ai_detect_cfg: AiDetectCfg) -> Result<()> {
        let connection = self.get_connection();
        let msg_num = self.new_message_num();
        let mut sub = connection
            .subscribe(MSG_ID_SET_AI_DETECT_CFG, msg_num)
            .await?;

        let set = Bc {
            meta: BcMeta {
                msg_id: MSG_ID_SET_AI_DETECT_CFG,
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
                    ai_detect_cfg: Some(ai_detect_cfg),
                    ..Default::default()
                })),
            }),
        };

        sub.send(set).await?;
        // Some cameras answer a settings write with nothing at all, so a timeout is treated as
        // success here exactly as set_pirstate does.
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
