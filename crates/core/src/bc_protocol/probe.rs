use super::{BcCamera, Result};
use crate::bc::model::*;

impl BcCamera {
    /// Send one bodyless message by id and return the camera's response code.
    ///
    /// Deliberately returns only the code. The reply's content is not decoded here because the
    /// point of probing is to look at messages nothing models yet -- `BcXml` would drop exactly
    /// the unknown fields worth seeing. The payload is read from the `debug = true` log instead,
    /// which prints it raw before any parsing.
    pub async fn probe_message(&self, msg_id: u32) -> Result<u16> {
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
        Ok(msg.meta.response_code)
    }
}
