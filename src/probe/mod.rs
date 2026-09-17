///
/// # Neolink Probe
///
/// Sends one Baichuan message by id and reports the camera's response code.
///
/// This is a **discovery tool**, not a feature. Neolink only models the messages someone has
/// already implemented, and `BcXml` silently ignores XML elements it does not know -- so a reply
/// containing a field nobody has modelled parses to an empty struct and looks like nothing. That
/// makes it impossible to answer "does this camera support X" by reading code.
///
/// Run it with `debug = true` on the camera in the config: Neolink then prints every decrypted
/// payload as `Payload Txt: ...` **before** parsing, so the reply's real XML appears in full
/// whether or not anything models it. That pairing -- this command plus that flag -- is how a new
/// message's schema gets established, and it replaces a packet capture of the Reolink app.
///
/// Only bodyless requests are sent, which is what a plain "get these settings" message is.
///
/// Several ids go in one session on purpose. Each session wakes a battery camera for tens of
/// seconds, so probing six messages one command at a time is six wakes for information that fits
/// in one. A marker line is printed before each send, which is what makes a reply attributable:
/// the payload log is a stream, and a session also carries login and keepalive traffic of its own.
///
/// # Usage
///
/// ```bash
/// # OSD settings (camera name, timestamp, watermark), with debug = true in the config
/// neolink probe --config=config.toml CameraName --cmd-ids 44
/// # Several at once, in a single wake
/// neolink probe --config=config.toml CameraName --cmd-ids 44,70,26,56
/// ```
use anyhow::{anyhow, Result};

mod cmdline;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Entry point for the probe subcommand
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    let camera = reactor.get(&opt.camera).await?;
    let cmd_ids = opt.cmd_ids.clone();
    if cmd_ids.is_empty() {
        return Err(anyhow!("Give at least one id with --cmd-ids"));
    }

    let results = camera
        .run_task(move |cam| {
            let cmd_ids = cmd_ids.clone();
            Box::pin(async move {
                let mut results: Vec<(u32, String)> = Vec::new();
                for cmd_id in cmd_ids.iter().copied() {
                    // Printed before the send so the payload that follows in the debug log can be
                    // attributed to it; a session carries traffic of its own too.
                    println!("=== probe cmd_id {} ===", cmd_id);
                    let outcome = match cam.probe_message(cmd_id).await {
                        Ok(code) => format!("code {}", code),
                        Err(err) => format!("failed: {:?}", err),
                    };
                    println!("=== probe cmd_id {} -> {} ===", cmd_id, outcome);
                    results.push((cmd_id, outcome));
                }
                Ok(results)
            })
        })
        .await?;

    println!();
    for (cmd_id, outcome) in results {
        println!("cmd_id {:<5} {}", cmd_id, outcome);
    }
    Ok(())
}
