//! The egui-based desktop app: one window that can either register this
//! machine as a host (share this screen) or connect to a remote host as a
//! controller (view + control its screen). Networking runs on a background
//! tokio runtime; the UI thread only ever touches shared state through a
//! `Mutex`, since eframe's render loop is synchronous.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::RgbaImage;
use webrtc::data_channel::RTCDataChannel;

use crate::controller;
use crate::host::register_and_serve;
use visuara_common::control::{InputEvent, KeyCode, MouseButton};
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
                    match register_and_serve(&server_url, &email, &password, &device_name, sink).await {
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
                ui.horizontal(|ui| {
                    ui.label("One-time password:");
                    ui.text_edit_singleline(&mut self.otp);
                });

                if ui.button("Connect").clicked() {
                    let server_url = self.server_url.clone();
                    let email = self.email.clone();
                    let password = self.password.clone();
                    let target = self.target_device_id.clone();
                    let otp = self.otp.clone();
                    let shared = self.shared.clone();
                    self.rt.spawn(async move {
                        let result = controller::connect(
                            &server_url,
                            &email,
                            &password,
                            &target,
                            ConnectCredential::OneTimePassword(otp),
                        )
                        .await;
                        match result {
                            Ok(mut session) => {
                                shared.lock().unwrap().data_channel = Some(session.data_channel.clone());
                                while let Some(frame) = session.frames.recv().await {
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

        let Some(texture) = &self.texture else {
            ui.centered_and_justified(|ui| ui.label("Waiting for video..."));
            return;
        };
        let Some((remote_w, remote_h)) = remote_size else { return };
        let Some(dc) = data_channel else { return };

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
