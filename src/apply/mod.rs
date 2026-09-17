///
/// # Neolink Apply
///
/// Writes several camera settings inside **one** session: the PIR switch, the PIR sensitivity, any
/// number of per-object-type AI sensitivities, and the video overlay (camera name and watermark).
///
/// The reason is the same one behind the `vitals` command. Login is what costs time and battery on
/// a 4G camera, not the messages, so a screen that saves three settings must not pay for three
/// wakes. It also means a save is atomic from the operator's point of view: either the session
/// happened or it did not.
///
/// Each setting is still a read-modify-write of its own message, because that is how the camera
/// works -- sending only the field that changed would reset the ones beside it.
///
/// # Usage
///
/// ```bash
/// neolink apply --config=config.toml CameraName --pir on --pir-sensitivity 26 --ai people=30 --ai vehicle=45
/// ```
use anyhow::{Context, Result};

/// Every change requested for one AI object type, so the type is read and written exactly once.
#[derive(Clone, Debug, Default)]
struct AiChange {
    sensitivity: Option<u8>,
    zone: Option<String>,
    min_width: Option<String>,
    min_height: Option<String>,
    max_width: Option<String>,
    max_height: Option<String>,
}

mod cmdline;

use crate::common::NeoReactor;
pub(crate) use cmdline::Opt;

/// Entry point for the apply subcommand
pub(crate) async fn main(opt: Opt, reactor: NeoReactor) -> Result<()> {
    let camera = reactor.get(&opt.camera).await?;
    let (pir, pir_sensitivity, ai) = (opt.pir, opt.pir_sensitivity, opt.ai.clone());
    let (osd_name, osd_watermark, osd_show_name, osd_set_cmd_id) = (
        opt.osd_name.clone(),
        opt.osd_watermark,
        opt.osd_show_name,
        opt.osd_set_cmd_id,
    );
    let touches_osd = osd_name.is_some() || osd_watermark.is_some() || osd_show_name.is_some();
    let (ftp_interval, ftp_triggers, ftp_set_cmd_id, anti_flicker) = (
        opt.ftp_interval,
        opt.ftp_triggers.clone(),
        opt.ftp_set_cmd_id,
        opt.anti_flicker.clone(),
    );
    // Width and height are one setting, the photo size, so clap requires them together: half a
    // resolution written to a camera is a shape it would accept and nobody intended.
    let (ftp_pic_width, ftp_pic_height) = (opt.ftp_pic_width, opt.ftp_pic_height);
    let (ftp_server, ftp_port, ftp_remote_dir, ftp_user, ftp_password) = (
        opt.ftp_server.clone(),
        opt.ftp_port,
        opt.ftp_remote_dir.clone(),
        opt.ftp_user.clone(),
        opt.ftp_password.clone(),
    );
    let (ftp_anonymous, ftp_version, ftp_auto_dir, ftp_stream_type) = (
        opt.ftp_anonymous,
        opt.ftp_version,
        opt.ftp_auto_dir,
        opt.ftp_stream_type,
    );
    let touches_ftp = ftp_interval.is_some()
        || ftp_pic_width.is_some()
        || ftp_server.is_some()
        || ftp_port.is_some()
        || ftp_remote_dir.is_some()
        || ftp_user.is_some()
        || ftp_password.is_some()
        || ftp_anonymous.is_some()
        || ftp_version.is_some()
        || ftp_auto_dir.is_some()
        || ftp_stream_type.is_some();
    let touches_triggers = !ftp_triggers.is_empty();

    // Every per-type change, gathered by object type. Collapsing them here rather than looping per
    // flag is what keeps a type to a single read-modify-write: the camera merges, but each extra
    // round trip is time a battery camera spends awake.
    let mut ai_changes: std::collections::BTreeMap<String, AiChange> = Default::default();
    for setting in &ai {
        ai_changes.entry(setting.ai_type.clone()).or_default().sensitivity = Some(setting.sensitivity);
    }
    for entry in &opt.ai_zone {
        ai_changes.entry(entry.ai_type.clone()).or_default().zone = Some(entry.value.clone());
    }
    for entry in &opt.ai_min_width {
        ai_changes.entry(entry.ai_type.clone()).or_default().min_width = Some(entry.value.clone());
    }
    for entry in &opt.ai_min_height {
        ai_changes.entry(entry.ai_type.clone()).or_default().min_height = Some(entry.value.clone());
    }
    for entry in &opt.ai_max_width {
        ai_changes.entry(entry.ai_type.clone()).or_default().max_width = Some(entry.value.clone());
    }
    for entry in &opt.ai_max_height {
        ai_changes.entry(entry.ai_type.clone()).or_default().max_height = Some(entry.value.clone());
    }

    if pir.is_none()
        && pir_sensitivity.is_none()
        && ai_changes.is_empty()
        && !touches_osd
        && !touches_ftp
        && !touches_triggers
        && anti_flicker.is_none()
    {
        // Writing nothing would still wake the camera, so refuse rather than spend the battery.
        return Err(anyhow::anyhow!(
            "Nothing to apply: give at least one setting to write"
        ));
    }

    camera
        .run_task(move |cam| {
            let ai_changes = ai_changes.clone();
            let osd_name = osd_name.clone();
            let ftp_triggers = ftp_triggers.clone();
            let anti_flicker = anti_flicker.clone();
            let ftp_server = ftp_server.clone();
            let ftp_remote_dir = ftp_remote_dir.clone();
            let ftp_user = ftp_user.clone();
            let ftp_password = ftp_password.clone();
            Box::pin(async move {
                if pir.is_some() || pir_sensitivity.is_some() {
                    let mut pir_state = cam
                        .get_pirstate()
                        .await
                        .context("Unable to read the PIR configuration before changing it")?;
                    if let Some(on) = pir {
                        pir_state.enable = u8::from(on);
                    }
                    if let Some(sensitivity) = pir_sensitivity {
                        pir_state.sensiValue = sensitivity;
                    }
                    cam.set_pirstate(pir_state)
                        .await
                        .context("Unable to set the PIR configuration")?;
                }

                for (ai_type, change) in ai_changes.iter() {
                    let mut cfg = cam
                        .get_ai_detect_cfg(ai_type)
                        .await
                        .with_context(|| {
                            format!(
                                "Unable to read the {} AI configuration before changing it",
                                ai_type
                            )
                        })?;
                    if let Some(sensitivity) = change.sensitivity {
                        cfg.sensitivity = Some(sensitivity);
                    }
                    if let Some(zone) = change.zone.clone() {
                        cfg.area = Some(zone);
                    }
                    if let Some(value) = change.min_width.clone() {
                        cfg.min_target_width = Some(value);
                    }
                    if let Some(value) = change.min_height.clone() {
                        cfg.min_target_height = Some(value);
                    }
                    if let Some(value) = change.max_width.clone() {
                        cfg.max_target_width = Some(value);
                    }
                    if let Some(value) = change.max_height.clone() {
                        cfg.max_target_height = Some(value);
                    }
                    cam.set_ai_detect_cfg(cfg).await.with_context(|| {
                        format!("Unable to set the {} AI configuration", ai_type)
                    })?;
                }

                if touches_osd {
                    let mut osd = cam
                        .get_osd()
                        .await
                        .context("Unable to read the OSD configuration before changing it")?;
                    let channel_name = osd.channel_name.as_mut().context(
                        "This camera reports no OsdChannelName, so it has no name or watermark to set",
                    )?;
                    if let Some(name) = osd_name {
                        channel_name.name = Some(name);
                    }
                    if let Some(watermark) = osd_watermark {
                        channel_name.en_watermark = Some(u8::from(watermark));
                    }
                    if let Some(show_name) = osd_show_name {
                        channel_name.enable = Some(u8::from(show_name));
                    }
                    cam.set_osd(osd, osd_set_cmd_id)
                        .await
                        .context("Unable to set the OSD configuration")?;
                }

                if touches_ftp {
                    let mut ftp = cam
                        .get_ftp()
                        .await
                        .context("Unable to read the FTP configuration before changing it")?;
                    if let Some(interval) = ftp_interval {
                        ftp.pic_intervals = Some(interval);
                    }
                    if let (Some(width), Some(height)) = (ftp_pic_width, ftp_pic_height) {
                        ftp.pic_width = Some(width);
                        ftp.pic_height = Some(height);
                    }
                    if let Some(server) = ftp_server {
                        ftp.server = Some(server);
                    }
                    if let Some(port) = ftp_port {
                        ftp.port = Some(port);
                    }
                    if let Some(dir) = ftp_remote_dir {
                        ftp.remote_dir = Some(dir);
                    }
                    if let Some(user) = ftp_user {
                        ftp.user_name = Some(user);
                    }
                    if let Some(anonymous) = ftp_anonymous {
                        ftp.nonymous = Some(u8::from(anonymous));
                    }
                    if let Some(version) = ftp_version {
                        ftp.ftp_version = Some(version);
                    }
                    if let Some(auto_dir) = ftp_auto_dir {
                        ftp.auto_dir = Some(u8::from(auto_dir));
                    }
                    if let Some(stream_type) = ftp_stream_type {
                        ftp.stream_type = Some(stream_type);
                    }
                    // The password the camera just handed back is a row of asterisks, not the real
                    // one. Writing it would set the password *to* asterisks and break every upload
                    // from this camera, so drop it unconditionally and put a value back only when
                    // the caller supplied a real one. Do not fold these two statements together.
                    ftp.password = None;
                    if let Some(password) = ftp_password {
                        ftp.password = Some(password);
                    }
                    cam.set_ftp(ftp, ftp_set_cmd_id)
                        .await
                        .context("Unable to set the FTP configuration")?;
                }

                if touches_triggers {
                    let mut task = cam
                        .get_ftp_task()
                        .await
                        .context("Unable to read the FTP trigger schedule before changing it")?;
                    let list = task.type_schedule_list.as_mut().context(
                        "This camera reports no FTP trigger schedule, so there is nothing to toggle",
                    )?;
                    for setting in ftp_triggers.iter() {
                        let item = list
                            .item
                            .iter_mut()
                            .find(|item| item.trigger_type == setting.trigger_type)
                            .with_context(|| {
                                format!("This camera has no FTP trigger {}", setting.trigger_type)
                            })?;
                        // The table is one digit per hour of the week. Turning a trigger on means
                        // every hour, off means none: the hours themselves are deliberately not
                        // exposed, so there is no partial schedule to preserve.
                        let digit = if setting.enabled { '1' } else { '0' };
                        item.value_table = std::iter::repeat(digit)
                            .take(item.value_table.chars().count())
                            .collect();
                    }
                    cam.set_ftp_task(task)
                        .await
                        .context("Unable to set the FTP trigger schedule")?;
                }

                if let Some(mode) = anti_flicker {
                    let mut isp = cam
                        .get_isp()
                        .await
                        .context("Unable to read the image settings before changing them")?;
                    let plf = isp.power_line_frequency.get_or_insert_with(Default::default);
                    // "off" is the camera's enable=0 rather than a mode of its own.
                    if mode == "off" {
                        plf.enable = Some(0);
                    } else {
                        plf.mode = Some(mode);
                        plf.enable = Some(1);
                    }
                    cam.set_isp(isp)
                        .await
                        .context("Unable to set the image settings")?;
                }

                Ok(())
            })
        })
        .await?;

    Ok(())
}
