use super::{BcCamera, Error, Result};
use crate::bc::{model::*, xml::*};

impl BcCamera {
    /// Read the FTP upload configuration: server, photo size and upload period.
    pub async fn get_ftp(&self) -> Result<Ftp> {
        let xml = self.get_config_message(MSG_ID_GET_FTP).await?;
        xml.ftp.ok_or(Error::Other("Expected an Ftp in the reply"))
    }

    /// Write the FTP upload configuration back.
    ///
    /// The password is never sent; see [`Ftp`] for why that matters.
    pub async fn set_ftp(&self, ftp: Ftp, msg_id: u32) -> Result<()> {
        self.set_config_message(
            msg_id,
            BcXml {
                ftp: Some(ftp),
                ..Default::default()
            },
        )
        .await
    }

    /// Read which triggers upload over FTP, and during which hours.
    pub async fn get_ftp_task(&self) -> Result<FtpTask> {
        let xml = self.get_config_message(MSG_ID_GET_FTP_TASK).await?;
        xml.ftp_task
            .ok_or(Error::Other("Expected an FtpTask in the reply"))
    }

    /// Write the FTP trigger schedule back.
    pub async fn set_ftp_task(&self, task: FtpTask) -> Result<()> {
        self.set_config_message(
            MSG_ID_SET_FTP_TASK,
            BcXml {
                ftp_task: Some(task),
                ..Default::default()
            },
        )
        .await
    }
}
