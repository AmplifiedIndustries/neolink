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

/// One AI object type and the sensitivity to give it, written `type=value`
#[derive(Clone, Debug)]
pub struct AiSetting {
    pub ai_type: String,
    pub sensitivity: u8,
}

fn ai_setting_parse(src: &str) -> Result<AiSetting> {
    let (ai_type, value) = src
        .split_once('=')
        .ok_or_else(|| anyhow!("Expected <type>=<sensitivity>, got {}", src))?;
    let sensitivity: u8 = value
        .parse()
        .map_err(|_| anyhow!("Sensitivity {} is not a number between 0 and 100", value))?;
    if sensitivity > 100 {
        return Err(anyhow!("Sensitivity {} is above 100", sensitivity));
    }
    Ok(AiSetting {
        ai_type: ai_type.to_string(),
        sensitivity,
    })
}

/// One AI object type and a free-form value for it, written `type=value`
///
/// Used for the settings whose value the CLI has no business interpreting: the detection zone is
/// base64 the camera produced, and the target-size bounds carry more digits than a float holds, so
/// both are passed through as the strings they arrived as.
#[derive(Clone, Debug)]
pub struct AiValue {
    pub ai_type: String,
    pub value: String,
}

fn ai_value_parse(src: &str) -> Result<AiValue> {
    let (ai_type, value) = src
        .split_once('=')
        .ok_or_else(|| anyhow!("Expected <type>=<value>, got {}", src))?;
    Ok(AiValue {
        ai_type: ai_type.to_string(),
        value: value.to_string(),
    })
}

/// The apply command writes several settings in a single camera session
#[derive(Parser, Debug)]
pub struct Opt {
    /// The name of the camera. Must be a name in the config
    pub camera: String,
    /// Turn the PIR on or off
    #[arg(long, value_parser = onoff_parse)]
    pub pir: Option<bool>,
    /// PIR sensitivity, on the camera's own inverted sensiValue scale
    #[arg(long)]
    pub pir_sensitivity: Option<u8>,
    /// AI sensitivity for one object type, written `people=30`. Repeatable
    #[arg(long = "ai", value_parser = ai_setting_parse)]
    pub ai: Vec<AiSetting>,
    /// Detection zone for one object type, written `people=<base64>`, exactly as the camera
    /// encodes it. Repeatable
    #[arg(long = "ai-zone", value_parser = ai_value_parse)]
    pub ai_zone: Vec<AiValue>,
    /// Smallest detected width for one object type as a fraction of the frame, written
    /// `people=0.08`. Repeatable
    #[arg(long = "ai-min-width", value_parser = ai_value_parse)]
    pub ai_min_width: Vec<AiValue>,
    /// Smallest detected height for one object type, as a fraction of the frame. Repeatable
    #[arg(long = "ai-min-height", value_parser = ai_value_parse)]
    pub ai_min_height: Vec<AiValue>,
    /// Largest detected width for one object type, as a fraction of the frame. Repeatable
    #[arg(long = "ai-max-width", value_parser = ai_value_parse)]
    pub ai_max_width: Vec<AiValue>,
    /// Largest detected height for one object type, as a fraction of the frame. Repeatable
    #[arg(long = "ai-max-height", value_parser = ai_value_parse)]
    pub ai_max_height: Vec<AiValue>,
    /// The camera name drawn on the video
    #[arg(long)]
    pub osd_name: Option<String>,
    /// Whether the Reolink logo watermark is drawn
    #[arg(long, value_parser = onoff_parse)]
    pub osd_watermark: Option<bool>,
    /// Whether the name overlay is drawn at all
    #[arg(long, value_parser = onoff_parse)]
    pub osd_show_name: Option<bool>,
    /// Message id to write the overlay with; see the osd subcommand
    #[arg(long, default_value = "45")]
    pub osd_set_cmd_id: u32,
    /// Seconds between scheduled FTP photo uploads
    #[arg(long)]
    pub ftp_interval: Option<u32>,
    /// FTP host the camera uploads to
    #[arg(long)]
    pub ftp_server: Option<String>,
    /// FTP port
    #[arg(long)]
    pub ftp_port: Option<u16>,
    /// Directory uploaded into
    #[arg(long)]
    pub ftp_remote_dir: Option<String>,
    /// FTP user
    #[arg(long)]
    pub ftp_user: Option<String>,
    /// FTP password. Omit to leave the camera's own alone -- the camera never reveals it, so there
    /// is nothing to round-trip and this flag is the only way to change it
    #[arg(long)]
    pub ftp_password: Option<String>,
    /// Log in anonymously instead of with a user and password
    #[arg(long, value_parser = onoff_parse)]
    pub ftp_anonymous: Option<bool>,
    /// FTP protocol variant, as the camera's own integer
    #[arg(long)]
    pub ftp_version: Option<u8>,
    /// Whether the camera files uploads into dated subdirectories
    #[arg(long, value_parser = onoff_parse)]
    pub ftp_auto_dir: Option<bool>,
    /// Which stream video uploads are taken from, as the camera's own integer
    #[arg(long)]
    pub ftp_stream_type: Option<u8>,
    /// Width in pixels of uploaded photos; set with the height, the pair is the photo quality
    #[arg(long, requires = "ftp_pic_height")]
    pub ftp_pic_width: Option<u32>,
    /// Height in pixels of uploaded photos
    #[arg(long, requires = "ftp_pic_width")]
    pub ftp_pic_height: Option<u32>,
    /// Turn one FTP trigger on or off, written `people=on`. Repeatable
    #[arg(long = "ftp-trigger", value_parser = trigger_parse)]
    pub ftp_triggers: Vec<TriggerSetting>,
    /// Message id to write the FTP configuration with
    #[arg(long, default_value = "69")]
    pub ftp_set_cmd_id: u32,
    /// Anti-flicker: 50hz, 60hz, outdoor, or off
    #[arg(long)]
    pub anti_flicker: Option<String>,
}

/// One FTP trigger type and whether it should upload
#[derive(Clone, Debug)]
pub struct TriggerSetting {
    pub trigger_type: String,
    pub enabled: bool,
}

fn trigger_parse(src: &str) -> Result<TriggerSetting> {
    let (trigger_type, value) = src
        .split_once('=')
        .ok_or_else(|| anyhow!("Expected <type>=<on|off>, got {}", src))?;
    Ok(TriggerSetting {
        trigger_type: trigger_type.to_string(),
        enabled: onoff_parse(value)?,
    })
}
