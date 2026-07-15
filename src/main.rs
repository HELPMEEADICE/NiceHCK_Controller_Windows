#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::collections::VecDeque;
use std::error::Error;
use std::time::Duration;

use nicehck_controller::controller::{
    ControllerCommand, ControllerEvent, ControllerHandle, spawn_controller,
};
use nicehck_controller::models::{AncMode, CodecMode, ConnectionState, DeviceState, EqMode};
use slint::{ComponentHandle, SharedString};
use tokio::runtime::Builder;
#[cfg(debug_assertions)]
use tracing::info;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(debug_assertions)]
    let (_log_guard, log_dir) = nicehck_controller::logging::initialize()?;
    #[cfg(debug_assertions)]
    info!(path = %log_dir.display(), "NiceHCK Controller starting");

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("nicehck-async")
        .build()?;
    let (controller, events) = {
        let _runtime_context = runtime.enter();
        spawn_controller()
    };

    let ui = MainWindow::new()?;
    let (clear_log_tx, clear_log_rx) = tokio::sync::mpsc::unbounded_channel();
    bind_callbacks(&ui, &controller, clear_log_tx);
    let event_task = runtime.spawn(forward_events(ui.as_weak(), events, clear_log_rx));

    let ui_result = ui.run();
    runtime.block_on(controller.shutdown());
    runtime.block_on(async {
        let _ = tokio::time::timeout(Duration::from_secs(2), event_task).await;
    });
    drop(ui);
    runtime.shutdown_timeout(Duration::from_secs(2));
    ui_result?;
    Ok(())
}

fn bind_callbacks(
    ui: &MainWindow,
    controller: &ControllerHandle,
    clear_log: tokio::sync::mpsc::UnboundedSender<()>,
) {
    bind_command(ui, controller, |ui, controller| {
        ui.on_connect_requested(move || send_command(&controller, ControllerCommand::Connect));
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_disconnect_requested(move || {
            send_command(&controller, ControllerCommand::Disconnect)
        });
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_refresh_requested(move || send_command(&controller, ControllerCommand::Refresh));
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_apply_anc(move |index| {
            if let Some(mode) = AncMode::ALL.get(index as usize).copied() {
                send_command(&controller, ControllerCommand::SetAnc(mode));
            }
        });
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_apply_eq(move |index| {
            if let Some(mode) = EqMode::ALL.get(index as usize).copied() {
                send_command(&controller, ControllerCommand::SetEq(mode));
            }
        });
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_apply_codec(move |index| {
            if let Some(mode) = CodecMode::ALL.get(index as usize).copied() {
                send_command(&controller, ControllerCommand::SetCodec(mode));
            }
        });
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_set_game_mode(move |enabled| {
            send_command(&controller, ControllerCommand::SetGameMode(enabled));
        });
    });
    bind_command(ui, controller, |ui, controller| {
        ui.on_set_low_latency(move |enabled| {
            send_command(&controller, ControllerCommand::SetLowLatency(enabled));
        });
    });
    ui.on_clear_log(move || {
        let _ = clear_log.send(());
    });
    ui.on_dismiss_error({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_error_message(SharedString::default());
            }
        }
    });
}

fn bind_command(
    ui: &MainWindow,
    controller: &ControllerHandle,
    bind: impl FnOnce(&MainWindow, ControllerHandle),
) {
    bind(ui, controller.clone());
}

fn send_command(controller: &ControllerHandle, command: ControllerCommand) {
    if let Err(error) = controller.send(command) {
        tracing::error!("failed to send controller command: {error}");
    }
}

async fn forward_events(
    weak: slint::Weak<MainWindow>,
    mut events: tokio::sync::mpsc::UnboundedReceiver<ControllerEvent>,
    mut clear_log: tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    let mut log_lines = VecDeque::with_capacity(200);
    loop {
        let event = tokio::select! {
            event = events.recv() => event,
            clear = clear_log.recv() => {
                if clear.is_none() {
                    continue;
                }
                log_lines.clear();
                update_log(&weak, &log_lines, None);
                continue;
            }
        };
        let Some(event) = event else {
            break;
        };
        match event {
            ControllerEvent::State(state) => {
                let _ = weak.upgrade_in_event_loop(move |ui| apply_state(&ui, &state));
            }
            ControllerEvent::Log(message) => {
                push_log(&mut log_lines, message);
                update_log(&weak, &log_lines, None);
            }
            ControllerEvent::Error(message) => {
                push_log(&mut log_lines, format!("错误：{message}"));
                update_log(&weak, &log_lines, Some(message));
            }
        }
    }
}

fn push_log(lines: &mut VecDeque<String>, message: String) {
    if lines.len() == 200 {
        lines.pop_front();
    }
    lines.push_back(message);
}

fn update_log(weak: &slint::Weak<MainWindow>, lines: &VecDeque<String>, error: Option<String>) {
    let text = lines.iter().cloned().collect::<Vec<_>>().join("\n");
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_log_text(text.into());
        if let Some(error) = error {
            ui.set_error_message(error.into());
        }
    });
}

fn apply_state(ui: &MainWindow, state: &DeviceState) {
    let connected = state.connection_state == ConnectionState::Connected;
    ui.set_connected(connected);
    ui.set_connecting(state.connection_state == ConnectionState::Connecting);
    ui.set_connection_status(
        match state.connection_state {
            ConnectionState::Disconnected => "未连接",
            ConnectionState::Connecting => "连接中",
            ConnectionState::Connected => "已连接",
            ConnectionState::Error => "连接错误",
        }
        .into(),
    );
    ui.set_device_name(state.device_name.clone().into());
    ui.set_endpoint(state.port_name.as_deref().unwrap_or("-").into());
    ui.set_firmware(state.firmware.display().into());
    ui.set_codec_status(state.selected_codec.map_or("-", CodecMode::label).into());
    set_battery(
        state.left_battery,
        |value| ui.set_left_battery(value),
        |value| ui.set_left_progress(value),
    );
    set_battery(
        state.right_battery,
        |value| ui.set_right_battery(value),
        |value| ui.set_right_progress(value),
    );
    set_battery(
        state.case_battery,
        |value| ui.set_case_battery(value),
        |value| ui.set_case_progress(value),
    );

    if let Some(index) = AncMode::ALL.iter().position(|mode| *mode == state.anc_mode) {
        ui.set_anc_index(index as i32);
    }
    if let Some(index) = EqMode::ALL.iter().position(|mode| *mode == state.eq_mode) {
        ui.set_eq_index(index as i32);
    }
    if let Some(codec) = state.selected_codec
        && let Some(index) = CodecMode::ALL.iter().position(|mode| *mode == codec)
    {
        ui.set_codec_index(index as i32);
    } else {
        ui.set_codec_index(0);
    }
    ui.set_game_mode(state.game_mode_enabled.unwrap_or(false));
    ui.set_low_latency(state.low_latency_enabled.unwrap_or(false));
    ui.set_codec_enabled(connected && state.firmware.known());
    ui.set_modern_codec(state.firmware.supports_modern_codec_switch());
    ui.set_extended_eq(state.firmware.supports_extended_eq());
}

fn set_battery(
    value: Option<u8>,
    set_text: impl FnOnce(SharedString),
    set_progress: impl FnOnce(f32),
) {
    match value {
        Some(value) => {
            set_text(format!("{value}%").into());
            set_progress(f32::from(value.min(100)) / 100.0);
        }
        None => {
            set_text("-".into());
            set_progress(0.0);
        }
    }
}
