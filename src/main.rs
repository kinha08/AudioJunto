#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod instance;
mod wake;
use eframe::egui::{self, Color32, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::mpsc, time::Duration};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

const ACCENT: Color32 = Color32::from_rgb(109, 224, 186);

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    source: String,
    targets: BTreeSet<String>,
}
enum Action {
    Open,
    Toggle,
    Quit,
}
struct App {
    _instance: instance::Instance,
    settings: Settings,
    devices: Vec<audio::Endpoint>,
    engine: Option<audio::Engine>,
    ready: bool,
    message: String,
    error: bool,
    tray: Option<TrayIcon>,
    toggle_item: Option<MenuItem>,
    tray_status: Option<(bool, bool)>,
    actions: mpsc::Receiver<Action>,
    quitting: bool,
    scan: Option<mpsc::Receiver<Result<audio::Devices, String>>>,
}

fn icon() -> Icon {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32usize {
        for x in 0..32usize {
            let rounded = (x as f32 - 15.5).powi(2) + (y as f32 - 15.5).powi(2) < 240.0;
            if rounded {
                let bar = [(8, 12, 20), (13, 7, 25), (18, 10, 22), (23, 13, 19)]
                    .iter()
                    .any(|&(bx, top, bottom)| x >= bx && x < bx + 3 && y >= top && y < bottom);
                rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(if bar {
                    &[109, 224, 186, 255]
                } else {
                    &[22, 30, 42, 255]
                });
            }
        }
    }
    Icon::from_rgba(rgba, 32, 32).expect("Ícone interno válido")
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, mut instance: instance::Instance) -> Self {
        let wake = wake::NativeWake::new(cc).expect("Janela nativa do aplicativo");
        instance.watch(cc.egui_ctx.clone(), wake);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(15, 20, 29);
        style.visuals.window_fill = Color32::from_rgb(22, 30, 42);
        style.visuals.selection.bg_fill = Color32::from_rgb(35, 95, 79);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        cc.egui_ctx.set_style(style);
        let settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, "devices"))
            .unwrap_or_default();
        let (tx, actions) = mpsc::channel();
        let mut app = Self {
            _instance: instance,
            settings,
            devices: Vec::new(),
            engine: None,
            ready: false,
            message: "Escolha onde compartilhar seu áudio.".into(),
            error: false,
            tray: None,
            toggle_item: None,
            tray_status: None,
            actions,
            quitting: false,
            scan: None,
        };
        match app.make_tray(tx, cc.egui_ctx.clone(), wake) {
            Ok((tray, toggle)) => {
                app.tray = Some(tray);
                app.toggle_item = Some(toggle);
            }
            Err(e) => {
                app.error = true;
                app.message = format!("Bandeja indisponível: {e}");
            }
        }
        app.refresh();
        app
    }
    fn make_tray(
        &self,
        tx: mpsc::Sender<Action>,
        ctx: egui::Context,
        wake: wake::NativeWake,
    ) -> anyhow::Result<(TrayIcon, MenuItem)> {
        let menu = Menu::new();
        let open = MenuItem::new("Abrir AudioJunto", true, None);
        let toggle = MenuItem::new("Ativar duplicação", true, None);
        let quit = MenuItem::new("Sair", true, None);
        menu.append_items(&[&open, &toggle, &quit])?;
        let open_id = open.id().clone();
        let toggle_id = toggle.id().clone();
        let quit_id = quit.id().clone();
        let menu_tx = tx.clone();
        let menu_ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == open_id {
                Action::Open
            } else if event.id == toggle_id {
                Action::Toggle
            } else if event.id == quit_id {
                Action::Quit
            } else {
                return;
            };
            let _ = menu_tx.send(action);
            wake.wake(&menu_ctx);
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                let _ = tx.send(Action::Open);
                wake.wake(&ctx);
            }
        }));
        let tray = TrayIconBuilder::new()
            .with_tooltip("AudioJunto • desativado")
            .with_icon(icon())
            .with_menu(Box::new(menu))
            .build()?;
        Ok((tray, toggle))
    }
    fn refresh(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(audio::devices().map_err(|e| format!("{e:#}")));
        });
    }
    fn finish_refresh(&mut self, result: Result<audio::Devices, String>) {
        match result {
            Ok(found) => {
                self.devices = found.endpoints;
                if !self.devices.iter().any(|d| d.id == self.settings.source) {
                    self.settings.source = found.default;
                }
                self.settings
                    .targets
                    .retain(|id| self.devices.iter().any(|d| &d.id == id));
                self.settings.targets.remove(&self.settings.source);
            }
            Err(e) => {
                self.error = true;
                self.message = format!("Não foi possível listar as saídas: {e:#}");
            }
        }
    }
    fn toggle(&mut self, ctx: &egui::Context) {
        if self.engine.take().is_some() {
            self.ready = false;
            self.error = false;
            self.message = "Duplicação desligada. O áudio original continua normalmente.".into();
        } else if self.settings.targets.is_empty() || self.settings.source.is_empty() {
            self.error = true;
            self.message = "Selecione a origem e pelo menos uma saída adicional.".into();
        } else {
            self.error = false;
            self.ready = false;
            self.message = "Conectando os dispositivos…".into();
            self.engine = Some(audio::Engine::start(
                self.settings.source.clone(),
                self.settings.targets.iter().cloned().collect(),
                ctx.clone(),
            ));
        }
        self.sync_tray();
    }
    fn sync_tray(&mut self) {
        let status = (self.engine.is_some(), self.ready);
        if self.tray_status == Some(status) {
            return;
        }
        self.tray_status = Some(status);
        if let Some(item) = &self.toggle_item {
            item.set_text(if self.engine.is_some() {
                "Desativar duplicação"
            } else {
                "Ativar duplicação"
            });
        }
        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some(if self.ready {
                "AudioJunto • compartilhando áudio"
            } else {
                "AudioJunto • desativado"
            }));
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "devices", &self.settings);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(result) = self.scan.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.scan = None;
            self.finish_refresh(result);
        }
        while let Ok(action) = self.actions.try_recv() {
            match action {
                Action::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Action::Toggle => self.toggle(ctx),
                Action::Quit => {
                    self.engine = None;
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        let mut failure = None;
        if let Some(engine) = &self.engine {
            while let Ok(event) = engine.events.try_recv() {
                match event {
                    audio::Event::Started => {
                        self.ready = true;
                        self.message = "Seu áudio está tocando nas saídas selecionadas.".into();
                    }
                    audio::Event::Failed(e) => failure = Some(e),
                }
            }
        }
        if let Some(e) = failure {
            self.engine = None;
            self.ready = false;
            self.error = true;
            self.message = e;
        }
        self.sync_tray();
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::new().inner_margin(24.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("≋").size(38.0).color(ACCENT));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("AudioJunto").size(28.0).strong());
                        ui.label(
                            RichText::new("Um som. Várias conexões.")
                                .color(Color32::from_gray(160)),
                        );
                    });
                });
                ui.add_space(16.0);
                egui::Frame::group(ui.style())
                    .inner_margin(18.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(if self.ready {
                                        "COMPARTILHANDO"
                                    } else {
                                        "DUPLICAÇÃO DE ÁUDIO"
                                    })
                                    .color(ACCENT)
                                    .strong(),
                                );
                                ui.label(if self.engine.is_some() {
                                    "Origem + saídas selecionadas"
                                } else {
                                    "Somente o áudio original"
                                });
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let label = if self.engine.is_some() {
                                        "●  Desativar"
                                    } else {
                                        "○  Ativar"
                                    };
                                    if ui
                                        .add(
                                            egui::Button::new(RichText::new(label).strong())
                                                .min_size(Vec2::new(125.0, 42.0)),
                                        )
                                        .clicked()
                                    {
                                        self.toggle(ctx);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(10.0);
                let running = self.engine.is_some();
                ui.add_enabled_ui(!running, |ui| {
                    ui.label(RichText::new("01  Origem do áudio").strong().size(16.0));
                    let name = self
                        .devices
                        .iter()
                        .find(|d| d.id == self.settings.source)
                        .map(|d| d.name.as_str())
                        .unwrap_or("Nenhum dispositivo");
                    egui::ComboBox::from_id_salt("source")
                        .width(ui.available_width() - 10.0)
                        .selected_text(name)
                        .show_ui(ui, |ui| {
                            for device in &self.devices {
                                ui.selectable_value(
                                    &mut self.settings.source,
                                    device.id.clone(),
                                    &device.name,
                                );
                            }
                        });
                    self.settings.targets.remove(&self.settings.source);
                    ui.label(
                        RichText::new("Escolha a saída em que o Windows já está tocando.")
                            .small()
                            .color(Color32::from_gray(150)),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("02  Tocar também em").strong().size(16.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Atualizar").clicked() {
                                self.refresh();
                            }
                        });
                    });
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            let mut count = 0;
                            for device in &self.devices {
                                if device.id == self.settings.source {
                                    continue;
                                }
                                count += 1;
                                egui::Frame::group(ui.style())
                                    .inner_margin(10.0)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        let mut selected =
                                            self.settings.targets.contains(&device.id);
                                        if ui.checkbox(&mut selected, &device.name).changed() {
                                            if selected {
                                                self.settings.targets.insert(device.id.clone());
                                            } else {
                                                self.settings.targets.remove(&device.id);
                                            }
                                        }
                                    });
                            }
                            if count == 0 {
                                ui.label("Conecte outro fone e clique em Atualizar.");
                            }
                        });
                });
                ui.add_space(12.0);
                ui.label(RichText::new(&self.message).color(if self.error {
                    Color32::from_rgb(255, 150, 150)
                } else {
                    ACCENT
                }));
                ui.add_space(8.0);
                ui.separator();
                ui.label(
                    RichText::new(
                        "Bluetooth pode ter atraso. Fechar a janela mantém o app na bandeja.",
                    )
                    .small()
                    .color(Color32::from_gray(150)),
                );
                if self.tray.is_some() && ui.link("Recolher para a bandeja").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
            });
        });
        // Timer also services hidden windows; tray callbacks explicitly wake egui.
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn main() -> eframe::Result {
    let Some(instance) =
        instance::Instance::acquire().map_err(|e| eframe::Error::AppCreation(Box::new(e)))?
    else {
        return Ok(());
    };
    let options = eframe::NativeOptions {
        persistence_path: std::env::current_exe()
            .ok()
            .map(|p| p.with_file_name("AudioJunto.ron")),
        viewport: egui::ViewportBuilder::default()
            .with_title("AudioJunto")
            .with_inner_size([580.0, 680.0])
            .with_min_inner_size([520.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AudioJunto",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, instance)))),
    )
}
