use egui::Button;
use egui::UiKind::ScrollArea;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System};

// cfg to enable cpu render if ram gets pushy later

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_STYLE, GetParent, GetWindow, GetWindowLongW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE, WS_CHILD,
};
use windows::core::{Array, BOOL, Result};

#[allow(unsafe_code)]
pub fn is_top_level(hwnd: HWND) -> bool {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;

        let is_not_child = (style & WS_CHILD.0) == 0;

        is_not_child
    }
}

// closing an app by its process id
#[allow(unsafe_code)]
pub fn close_by_pid(target_pid: &u32) -> Result<()> {
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
        EnumWindows(Some(enum_windows_proc), LPARAM(*target_pid as isize));
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
        EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut data as *mut _ as isize),
        );
    }

    data.1
}

pub fn strip_file_extension(s: &String) -> String {
    Path::new(s)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

// making sure we don't try to kill some system process or helper
// i'm going to anyway tho, i'm certain lol
// TODO: make this togglable later
// maybe check if owner is SYSTEM?
pub fn loosely_check_if_real_app(pid: &u32, name: &String) -> bool {
    // system processes
    let lower = name.to_ascii_lowercase();
    if lower.contains("service")
        || lower.contains("helper")
        || lower.contains("overlay")
        || lower.contains("tray")
        || lower.contains("host")
        || lower.contains("broker")
        || lower.contains("container")
        || lower.contains("runtime")
        || lower.contains("svchost")
        || lower.contains("dwm")
        || lower.contains("explorer")
        || *pid == 0 || *pid == 4 {
        return false;
    }
    true
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

    #[serde(skip)]
    processlist: BTreeMap<String, u32>,

    #[serde(skip)]
    filter_to_remove: HashSet<String>,

    #[serde(skip)]
    selected_process_pid: Option<u32>,

    whitelist: BTreeSet<String>,

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
            processlist: BTreeMap::new(),
            filter_to_remove: HashSet::new(),
            selected_process_pid: None,
            whitelist: BTreeSet::new(),
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
            self.sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::everything().without_tasks(),
            );

            // populating processlist
            self.processlist.clear();
            for (pid, process) in self.sys.processes() {
                let maybe_hwnd: Option<HWND> = get_hwnd_by_pid(pid.as_u32());
                if maybe_hwnd.is_none() {
                    continue;
                }
                if is_top_level(maybe_hwnd.unwrap()) {
                    // we don't strip file extension at the source because we will use in the actual whitelist,
                    // so it's removed only in display
                    self.processlist.insert(
                        process.name().to_string_lossy().parse().unwrap(),
                        pid.as_u32(),
                    );
                };
            }

            for key in &self.whitelist {
                self.processlist.remove(key.as_str());
            }

            // and filtering it
            for (name, pid) in &self.processlist {
                if !loosely_check_if_real_app(&pid, &name) || name.as_str() == "expurgate.exe" {
                    println!("{:?}", name);
                    self.filter_to_remove.insert(name.clone());
                };
            }

            for name in &self.filter_to_remove {
                &self.processlist.remove(name.as_str());
            }


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
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .id_salt("scrollin_30x9403mcd2")
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    let mut to_remove = None;

                    for name in &self.whitelist {
                        ui.horizontal(|ui| {
                            if ui.button("-").clicked() {
                                to_remove = Some(name.clone());
                            }

                            ui.add_sized([50.0, 20.0], egui::Label::new(strip_file_extension(name)));
                        });
                    }

                    if let Some(name) = to_remove {
                        &self.whitelist.remove(&name);
                    }
                });

            ui.separator();
            ui.label("Death note");

            if ui.button("Close Notepad politely").clicked() {
                close_by_pid(&24588).unwrap();
            }
            

            if ui.button("Kill them all.").clicked() {
                println!("Killing {:#?}", &self.processlist);
                for (_, pid) in &self.processlist {
                    close_by_pid(pid).unwrap();
                }
            }

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.horizontal(|ui| {
                        ui.add_sized([50.0, 20.0], egui::Label::new("PID"));
                        ui.add_sized([50.0, 20.0], egui::Label::new("Process Name"));
                    });

                    for (name, pid) in &self.processlist {
                        ui.push_id(*pid, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("+").clicked() {
                                    self.whitelist.insert(name.to_string());
                                }
                                ui.add_sized([50.0, 20.0], egui::Label::new(pid.to_string()));
                                ui.add_sized(
                                    [0.0, 20.0],
                                    egui::Label::new(strip_file_extension(name)),
                                );
                            });
                        });
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
