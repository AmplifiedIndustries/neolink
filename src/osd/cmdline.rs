use anyhow::{anyhow, Result};
use clap::Parser;

fn onoff_parse(src: &str) -> Result<bool> {
    match src {
        "true" | "on" | "yes" => Ok(true),
        "false" | "off" | "no" => Ok(false),
        _ => Err(anyhow!(
            "Could not understand {}, check your input, should be true/false, on/off or yes/no",
            src
        )),
    }
}

/// The osd command reads or changes the camera name and the logo watermark
#[derive(Parser, Debug)]
pub struct Opt {
    /// The name of the camera. Must be a name in the config
    pub camera: String,
    /// The name drawn on the video
    #[arg(long)]
    pub name: Option<String>,
    /// Whether the Reolink logo watermark is drawn
    #[arg(long, value_parser = onoff_parse)]
    pub watermark: Option<bool>,
    /// Whether the name overlay is drawn at all
    #[arg(long, value_parser = onoff_parse)]
    pub show_name: Option<bool>,
    /// Message id to write with. The setter's id is not published; every other pair here is
    /// get+1, so 45 is the default and this exists to try another without a rebuild.
    #[arg(long, default_value = "45")]
    pub set_cmd_id: u32,
}
