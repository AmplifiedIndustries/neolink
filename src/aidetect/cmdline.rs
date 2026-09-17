use clap::Parser;

/// The ai-detect command reads or changes the per-object-type AI detection settings
#[derive(Parser, Debug)]
pub struct Opt {
    /// The name of the camera. Must be a name in the config
    pub camera: String,
    /// The object type, e.g. person, vehicle, dog_cat
    #[arg(long, default_value = "person")]
    pub ai_type: String,
    /// Detection sensitivity to set. Omit to read the current configuration
    #[arg(long)]
    pub sensitivity: Option<u8>,
    /// How long an object must remain before the camera reports it, in seconds
    #[arg(long)]
    pub delay: Option<u32>,
}
