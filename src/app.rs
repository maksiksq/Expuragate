use std::{collections::HashSet};
use std::collections::HashMap;
use egui::Button;
use egui::UiKind::ScrollArea;
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System};

// cfg to enable cpu render if ram gets pushy later

use windows::core::{Array, Result, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetParent, GetWindow, GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GWL_STYLE, GW_OWNER, WM_CLOSE, WS_CHILD};

#[allow(unsafe_code)]
pub fn is_top_level(hwnd: HWND) -> bool {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;

        let has_no_parent = match GetParent(hwnd) {
            Ok(h) => h.0.is_null(),
            Err(_) => true,
        };

        let has_no_owner = match GetWindow(hwnd, GW_OWNER) {
            Ok(h) => h.0.is_null(),
            Err(_) => true,
        };

        let is_not_child = (style & WS_CHILD.0) == 0;

        has_no_parent && has_no_owner && is_not_child
    }
}

// closing an app by its process id
#[allow(unsafe_code)]
pub fn close_by_pid(target_pid: u32) -> Result<()> {
    extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));

            let target_pid = lparam.0 as usize as u32;
            if pid == target_pid {
                if is_top_level(hwnd) {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
        BOOL(1) // continuing enumeration
    }

    unsafe {
        EnumWindows(Some(enum_windows_proc), LPARAM(target_pid as isize));
    }
    Ok(())
}

#[allow(unsafe_code)]
pub fn get_hwnd_by_pid(target_pid: u32) -> Option<HWND> {
    extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));

            let data = lparam.0 as *mut (u32, Option<HWND>);
            let (target_pid, found_hwnd) = unsafe { &mut *data };
            if pid == *target_pid {
                *found_hwnd = Some(hwnd);
                return BOOL(0);
            };
        }
        BOOL(1)
    }

    let mut data = (target_pid, None);
    unsafe {
        EnumWindows(Some(enum_windows_proc), LPARAM(&mut data as *mut _ as isize));
    }

    data.1
}

//
// Tomorrow me:
// Take the current processes, sort the duplicates into one entry and then find top level
// (instead of the registry and parts (cause paths can change))
//
// pub fn get_list_available_apps

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,

    #[serde(skip)]
    sys: System,

    // maybe life time parameter here is a bad idea ??? idk
    #[serde(skip)]
    processlist: HashMap<String, u32>,

    #[serde(skip)]
    selected_process_pid: Option<u32>,

    #[serde(skip)]
    whitelist: HashSet<String>,

    #[serde(skip)]
    whitelist_input: String,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            sys: System::new_all(),
            processlist: HashMap::new(),
            selected_process_pid: None,
            whitelist: HashSet::new(),
            whitelist_input: String::new(),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("Whitelist");

            ui.horizontal(|ui| {
                ui.label("Add to whitelist");
                ui.text_edit_singleline(&mut self.whitelist_input);
            });

            if ui.button("Add").clicked() {
                let item = self.whitelist_input.trim();
                if !item.is_empty() {
                    self.whitelist.insert(item.to_string());
                    self.whitelist_input.clear();
                }
            }

            ui.separator();


            ui.label("Whitelist in question:");
            for item in &self.whitelist {
                ui.label(format!("- {}", item));
            }


            ui.separator();
            ui.label("Running processes");

            if ui.button("Close Notepad politely").clicked() {
                close_by_pid(24588).unwrap();
            }

            self.sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::everything().without_tasks(),
            );

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    ui.add_sized([50.0, 20.0], egui::Label::new("PID"));
                    ui.add_sized([50.0, 20.0], egui::Label::new("Process Name"));
                });

                self.processlist.clear();
                for (pid, process) in self.sys.processes() {
                    let maybe_hwnd: Option<HWND> = get_hwnd_by_pid(pid.as_u32());
                    if maybe_hwnd.is_none() {
                        continue;
                    }
                    if is_top_level(maybe_hwnd.unwrap()) {
                        self.processlist.insert(process.name().to_string_lossy().parse().unwrap(), pid.as_u32());
                    };
                }

                for (name, pid) in &self.processlist {
                    ui.horizontal(|ui| {
                        if ui.button("+").clicked() {
                            self.whitelist.insert(name.to_string());
                        }
                        ui.add_sized([50.0, 20.0], egui::Label::new(pid.to_string()));
                        ui.add_sized([50.0, 20.0], egui::Label::new(name));

                    })
                        .response
                        .rect
                    ;
                }
            });

            ui.spacing_mut().slider_width = 300.0;


            ui.separator();


            if ui.button("Say hi").clicked() {
                println!("Say hi");
            }

            ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label);
            });


            ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }

            ui.separator();

            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/main/",
                "Source code."
            ));

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
