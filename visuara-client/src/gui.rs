//! The egui-based desktop app: one window that can either register this
//! machine as a host (share this screen) or connect to a remote host as a
//! controller (view + control its screen). Networking runs on a background
//! tokio runtime; the UI thread only ever touches shared state through a
//! `Mutex`, since eframe's render loop is synchronous.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::RgbaImage;
use webrtc::data_channel::RTCDataChannel;

use crate::clipboard_sync::ClipboardSync;
use crate::controller;
use crate::host::register_and_serve;
use visuara_common::control::{InputEvent, KeyCode, MonitorInfo, MouseButton};
use visuara_common::signaling::ConnectCredential;

#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum Panel {
    #[default]
    Host,
    Connect,
}

#[derive(Default)]
struct SharedState {
    status: String,
    host_info: Option<(String, String)>,
    data_channel: Option<Arc<RTCDataChannel>>,
    latest_frame: Option<RgbaImage>,
    remote_size: Option<(u32, u32)>,
    monitors: Vec<MonitorInfo>,
    // Kept alive for as long as the session lasts; dropping it stops sync.
    _clipboard_sync: Option<Arc<ClipboardSync>>,
}

pub struct VisuaraApp {
    rt: tokio::runtime::Handle,
    shared: Arc<Mutex<SharedState>>,
    texture: Option<egui::TextureHandle>,
    panel: Panel,
    server_url: String,
    email: String,
    password: String,
    device_name: String,
    target_device_id: String,
    otp: String,
    selected_monitor_id: Option<u32>,
    unattended_password: String,
    use_unattended_password: bool,
}

impl VisuaraApp {
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        // Keeps the embedded-config marker static from being optimized away
        // in a release build, even though nothing else references it by
        // symbol — the server locates and patches it by raw byte search.
        std::hint::black_box(&visuara_common::embedded_config::VISUARA_EMBEDDED_CONFIG_SLOT);

        let embedded = visuara_common::embedded_config::EmbeddedConfig::read_from_current_exe()
            .ok()
            .flatten()
            .unwrap_or_default();

        Self {
            rt,
            shared: Arc::new(Mutex::new(SharedState::default())),
            texture: None,
            panel: Panel::default(),
            server_url: embedded.server_url.unwrap_or_else(|| "ws://127.0.0.1:8080/ws".to_string()),
            email: String::new(),
            password: String::new(),
            device_name: embedded.device_name.unwrap_or_else(|| "this-machine".to_string()),
            target_device_id: String::new(),
            otp: String::new(),
            selected_monitor_id: None,
            unattended_password: String::new(),
            use_unattended_password: false,
        }
    }

    fn send_input(&self, dc: &Arc<RTCDataChannel>, event: InputEvent) {
        let dc = dc.clone();
        self.rt.spawn(async move {
            let _ = controller::send_input(&dc, event).await;
        });
    }
}

impl eframe::App for VisuaraApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(33));

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.panel, Panel::Host, "Host");
                ui.selectable_value(&mut self.panel, Panel::Connect, "Connect");
            });
        });

        match self.panel {
            Panel::Host => self.show_host_panel(ui),
            Panel::Connect => self.show_connect_panel(ui),
        }
    }
}

impl VisuaraApp {
    fn show_host_panel(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Share this machine");
            ui.horizontal(|ui| {
                ui.label("Server URL:");
                ui.text_edit_singleline(&mut self.server_url);
            });
            ui.horizontal(|ui| {
                ui.label("Email:");
                ui.text_edit_singleline(&mut self.email);
            });
            ui.horizontal(|ui| {
                ui.label("Password:");
                ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
            });
            ui.horizontal(|ui| {
                ui.label("Device name:");
                ui.text_edit_singleline(&mut self.device_name);
            });

            let host_info = self.shared.lock().unwrap().host_info.clone();
            if let Some((device_id, otp)) = host_info {
                ui.separator();
                ui.label(format!("Device ID: {device_id}"));
                ui.label(format!("One-time password: {otp}"));
                ui.label("Waiting for incoming connections...");

                ui.separator();
                ui.heading("Unattended access");
                ui.horizontal(|ui| {
                    ui.label("Fixed password:");
                    ui.add(egui::TextEdit::singleline(&mut self.unattended_password).password(true));
                    if ui.button("Set").clicked() && !self.unattended_password.is_empty() {
                        let server_url = self.server_url.clone();
                        let email = self.email.clone();
                        let password = self.password.clone();
                        let unattended_password = self.unattended_password.clone();
                        let device_id = device_id.clone();
                        let shared = self.shared.clone();
                        self.rt.spawn(async move {
                            let result = crate::host::set_unattended_password(
                                &server_url,
                                &email,
                                &password,
                                &device_id,
                                &unattended_password,
                            )
                            .await;
                            if let Err(e) = result {
                                shared.lock().unwrap().status = format!("failed to set unattended password: {e:#}");
                            }
                        });
                    }
                });
                ui.label("Lets a controller connect to this device without a fresh one-time password.");

                ui.separator();
                let autostart_enabled = crate::autostart::is_enabled();
                let toggle_label = if autostart_enabled { "Disable auto-start on login" } else { "Enable auto-start on login" };
                if ui.button(toggle_label).clicked() {
                    if autostart_enabled {
                        if let Err(e) = crate::autostart::disable().and_then(|_| crate::saved_credentials::SavedCredentials::clear()) {
                            self.shared.lock().unwrap().status = format!("failed to disable auto-start: {e:#}");
                        }
                    } else {
                        let creds = crate::saved_credentials::SavedCredentials {
                            server_url: self.server_url.clone(),
                            email: self.email.clone(),
                            password: self.password.clone(),
                            device_name: self.device_name.clone(),
                        };
                        let result = creds.save().and_then(|_| crate::autostart::enable());
                        if let Err(e) = result {
                            self.shared.lock().unwrap().status = format!("failed to enable auto-start: {e:#}");
                        }
                    }
                }
                ui.label("When enabled, this machine automatically re-shares itself after you log in, using the account above.");
            } else if ui.button("Start sharing").clicked() {
                let server_url = self.server_url.clone();
                let email = self.email.clone();
                let password = self.password.clone();
                let device_name = self.device_name.clone();
                let shared = self.shared.clone();
                self.rt.spawn(async move {
                    let sink = match visuara_agent::input::InputInjector::new() {
                        Ok(s) => Box::new(s),
                        Err(e) => {
                            shared.lock().unwrap().status = format!("input injector failed: {e:#}");
                            return;
                        }
                    };
                    let dest = crate::file_transfer::FileReceiver::default_destination();
                    match register_and_serve(&server_url, &email, &password, &device_name, sink, dest).await {
                        Ok(handle) => {
                            shared.lock().unwrap().host_info = Some((handle.device_id, handle.one_time_password));
                        }
                        Err(e) => {
                            shared.lock().unwrap().status = format!("registration failed: {e:#}");
                        }
                    }
                });
            }

            let status = self.shared.lock().unwrap().status.clone();
            if !status.is_empty() {
                ui.colored_label(egui::Color32::RED, status);
            }
        });
    }

    fn show_connect_panel(&mut self, ui: &mut egui::Ui) {
        let connected = self.shared.lock().unwrap().data_channel.is_some();

        if !connected {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Connect to a device");
                ui.horizontal(|ui| {
                    ui.label("Server URL:");
                    ui.text_edit_singleline(&mut self.server_url);
                });
                ui.horizontal(|ui| {
                    ui.label("Email:");
                    ui.text_edit_singleline(&mut self.email);
                });
                ui.horizontal(|ui| {
                    ui.label("Password:");
                    ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                });
                ui.horizontal(|ui| {
                    ui.label("Target device ID:");
                    ui.text_edit_singleline(&mut self.target_device_id);
                });
                ui.checkbox(&mut self.use_unattended_password, "Use fixed unattended password instead of a one-time password");
                ui.horizontal(|ui| {
                    ui.label(if self.use_unattended_password { "Fixed password:" } else { "One-time password:" });
                    ui.add(egui::TextEdit::singleline(&mut self.otp).password(self.use_unattended_password));
                });

                if ui.button("Connect").clicked() {
                    let server_url = self.server_url.clone();
                    let email = self.email.clone();
                    let password = self.password.clone();
                    let target = self.target_device_id.clone();
                    let otp = self.otp.clone();
                    let credential = if self.use_unattended_password {
                        ConnectCredential::UnattendedPassword(otp)
                    } else {
                        ConnectCredential::OneTimePassword(otp)
                    };
                    let shared = self.shared.clone();
                    self.rt.spawn(async move {
                        let result = controller::connect(&server_url, &email, &password, &target, credential).await;
                        match result {
                            Ok(session) => {
                                let controller::ControllerSession {
                                    data_channel,
                                    mut frames,
                                    mut monitor_updates,
                                    clipboard_sync,
                                } = session;
                                {
                                    let mut s = shared.lock().unwrap();
                                    s.data_channel = Some(data_channel);
                                    s._clipboard_sync = Some(clipboard_sync);
                                }
                                let shared_for_monitors = shared.clone();
                                tokio::spawn(async move {
                                    while let Some(monitors) = monitor_updates.recv().await {
                                        shared_for_monitors.lock().unwrap().monitors = monitors;
                                    }
                                });
                                while let Some(frame) = frames.recv().await {
                                    let mut s = shared.lock().unwrap();
                                    s.remote_size = Some((frame.width(), frame.height()));
                                    s.latest_frame = Some(frame);
                                }
                            }
                            Err(e) => {
                                shared.lock().unwrap().status = format!("connect failed: {e:#}");
                            }
                        }
                    });
                }

                let status = self.shared.lock().unwrap().status.clone();
                if !status.is_empty() {
                    ui.colored_label(egui::Color32::RED, status);
                }
            });
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    self.show_video_and_capture_input(ui);
                });
        }
    }

    fn show_video_and_capture_input(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (frame, remote_size, data_channel) = {
            let mut s = self.shared.lock().unwrap();
            (s.latest_frame.take(), s.remote_size, s.data_channel.clone())
        };

        if let Some(frame) = frame {
            let size = [frame.width() as usize, frame.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, frame.as_raw());
            match &mut self.texture {
                Some(tex) => tex.set(color_image, egui::TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("remote-video", color_image, egui::TextureOptions::LINEAR)),
            }
        }

        let monitors = self.shared.lock().unwrap().monitors.clone();
        if !monitors.is_empty() {
            if let Some(dc) = &data_channel {
                ui.horizontal(|ui| {
                    ui.label("Monitor:");
                    let current_label = self
                        .selected_monitor_id
                        .and_then(|id| monitors.iter().find(|m| m.id == id))
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| "Primary".to_string());
                    egui::ComboBox::from_id_salt("monitor-select").selected_text(current_label).show_ui(ui, |ui| {
                        for m in &monitors {
                            if ui.selectable_label(Some(m.id) == self.selected_monitor_id, &m.name).clicked() {
                                self.selected_monitor_id = Some(m.id);
                                let dc = dc.clone();
                                let monitor_id = m.id;
                                self.rt.spawn(async move {
                                    let _ = controller::switch_monitor(&dc, monitor_id).await;
                                });
                            }
                        }
                    });
                });
            }
        }

        let Some(texture) = &self.texture else {
            ui.centered_and_justified(|ui| ui.label("Waiting for video..."));
            return;
        };
        let Some((remote_w, remote_h)) = remote_size else { return };
        let Some(dc) = data_channel else { return };

        // Drag-and-drop: send any dropped local files to the host.
        let dropped_paths: Vec<_> =
            ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for path in dropped_paths {
            let dc = dc.clone();
            self.rt.spawn(async move {
                if let Err(e) = crate::file_transfer::send_file(&dc, &path).await {
                    eprintln!("[controller] file send failed: {e:#}");
                }
            });
        }

        let available = ui.available_size();
        let response = ui.add(
            egui::Image::new((texture.id(), available)).sense(egui::Sense::click_and_drag()),
        );
        let rect = response.rect;

        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::PointerMoved(pos) if rect.contains(pos) => {
                    let (x, y) = remote_coords(pos, rect, remote_w, remote_h);
                    self.send_input(&dc, InputEvent::MouseMove { x, y });
                }
                egui::Event::PointerButton { pos, button, pressed, .. } if rect.contains(pos) => {
                    if let Some(button) = map_pointer_button(button) {
                        self.send_input(&dc, InputEvent::MouseButton { button, pressed });
                    }
                }
                egui::Event::MouseWheel { delta, .. } if response.hovered() => {
                    self.send_input(
                        &dc,
                        InputEvent::MouseScroll { delta_x: -delta.x as i32, delta_y: -delta.y as i32 },
                    );
                }
                egui::Event::Text(text) if response.hovered() || response.has_focus() => {
                    self.send_input(&dc, InputEvent::TypeText { text });
                }
                egui::Event::Key { key, pressed, repeat: false, .. }
                    if response.hovered() || response.has_focus() =>
                {
                    if let Some(key) = map_special_key(key) {
                        self.send_input(&dc, InputEvent::KeyEvent { key, pressed });
                    }
                }
                _ => {}
            }
        }

        response.request_focus();
    }
}

fn remote_coords(pos: egui::Pos2, rect: egui::Rect, remote_w: u32, remote_h: u32) -> (i32, i32) {
    let rel_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let rel_y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    ((rel_x * remote_w as f32) as i32, (rel_y * remote_h as f32) as i32)
}

fn map_pointer_button(button: egui::PointerButton) -> Option<MouseButton> {
    match button {
        egui::PointerButton::Primary => Some(MouseButton::Left),
        egui::PointerButton::Secondary => Some(MouseButton::Right),
        egui::PointerButton::Middle => Some(MouseButton::Middle),
        _ => None,
    }
}

fn map_special_key(key: egui::Key) -> Option<KeyCode> {
    use egui::Key as EK;
    Some(match key {
        EK::Backspace => KeyCode::Backspace,
        EK::Enter => KeyCode::Enter,
        EK::Tab => KeyCode::Tab,
        EK::Escape => KeyCode::Escape,
        EK::Space => KeyCode::Space,
        EK::Delete => KeyCode::Delete,
        EK::ArrowUp => KeyCode::ArrowUp,
        EK::ArrowDown => KeyCode::ArrowDown,
        EK::ArrowLeft => KeyCode::ArrowLeft,
        EK::ArrowRight => KeyCode::ArrowRight,
        _ => return None,
    })
}

pub fn run(rt: tokio::runtime::Handle) -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Visuara",
        options,
        Box::new(|_cc| Ok(Box::new(VisuaraApp::new(rt)))),
    )
}
