use clap::Parser;

/// The probe command sends raw Baichuan messages and reports what came back
#[derive(Parser, Debug)]
pub struct Opt {
    /// The name of the camera. Must be a name in the config
    pub camera: String,
    /// Comma separated Baichuan message ids to send, e.g. `44,70,26`
    #[arg(long, value_delimiter = ',')]
    pub cmd_ids: Vec<u32>,
}
