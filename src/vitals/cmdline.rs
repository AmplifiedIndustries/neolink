use clap::Parser;

/// The vitals command reads everything the bridge needs in a single camera session
#[derive(Parser, Debug)]
pub struct Opt {
    /// The name of the camera. Must be a name in the config
    pub camera: String,
    /// Comma separated AI object types to read, in the camera's own vocabulary
    #[arg(long, default_value = "people,vehicle,dog_cat")]
    pub ai_types: String,
}
