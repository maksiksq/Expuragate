use egui::Button;
use egui::UiKind::ScrollArea;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System};

// cfg to enable cpu render if ram gets pushy later

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetLastActivePopup,
    GetMessageW, GetParent, GetWindow, GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible,
    MSG, PostMessageW, WM_CLOSE, WM_HOTKEY, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    WS_VISIBLE,
};
use windows::core::{Array, BOOL, Result};

#[allow(unsafe_code)]
pub fn is_pseudo_open_in_taskbar(mut hwnd: HWND) -> bool {
    unsafe {
        // Finding a visible popup
        let root = GetAncestor(hwnd, GA_ROOTOWNER);
        let mut last = root;
        loop {
            let popup = GetLastActivePopup(last);
            if popup == last {
                break;
            }
            if IsWindowVisible(popup).as_bool() {
                last = popup;
                break;
            }
            last = popup;
        }

        // if the root is invisible but the popup is visible, we use the popup
        if !IsWindowVisible(last).as_bool() {
            let popup = GetLastActivePopup(root);
            if IsWindowVisible(popup).as_bool() {
                last = popup;
            }
        }

        hwnd = last;

        // Is cloaked?
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
        {
            if cloaked != 0 {
                return false;
            }
        }

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // Is tool window
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }

        // Is visible?
        if IsWindowVisible(hwnd).as_bool() == false {
            return false;
        }

        true
    }
}

// #[allow(unsafe_code)]
// pub fn is_top_level(hwnd: HWND) -> bool {
//     unsafe {
//         let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
//
//         let is_not_child = (style & WS_CHILD.0) == 0;
//
//         is_not_child
//     }
// }

// closing an app by its process id
#[allow(unsafe_code)]
pub fn close_by_pid(target_pid: &u32) -> Result<()> {
    extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));

            let target_pid = lparam.0 as usize as u32;
            if pid == target_pid {
                if is_pseudo_open_in_taskbar(hwnd) {
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

#[allow(unsafe_code)]
pub fn register_kill_hotkey() {
    unsafe {
        RegisterHotKey(None, 1, MOD_CONTROL | MOD_ALT, 'J' as u32)
            .expect("Failed to register hotkey");
    }
}

#[allow(unsafe_code)]
pub fn unregister_hotkey(id: i32) {
    unsafe {
        UnregisterHotKey(Option::from(HWND(std::ptr::null_mut())), id)
            .expect("Failed to unregister hotkey");
    }
}

// handling closing with a hotkey
#[allow(unsafe_code)]
pub fn start_kill_hotkey_listener(tx: Sender<HotkeyEvent>) {
    thread::spawn(move || unsafe {
        let mut msg = MSG::default();
        register_kill_hotkey();

        while GetMessageW(&mut msg, None, 0, 0).into() {
            if msg.message == WM_HOTKEY && msg.wParam.0 == 1 {
                tx.send(HotkeyEvent::Kill).ok();
            }
        }
    });
}

// making sure we don't try to kill some system process or helper
// i'm going to anyway tho, i'm certain lol
// TODO: make this togglable later
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
        || *pid == 0
        || *pid == 4
    {
        return false;
    }
    true
}

#[derive(Debug)]
pub enum HotkeyEvent {
    Kill,
}

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

    allowlist: BTreeSet<String>,

    #[serde(skip)]
    allowlist_input: String,

    #[serde(skip)]
    kill_hotkey_registered: bool,

    #[serde(skip)]
    hotkey_rx: Receiver<HotkeyEvent>,

}

impl Default for TemplateApp {
    fn default() -> Self {
        // dummy sender
        let (_tx, rx) = mpsc::channel();

        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            sys: System::new_all(),
            processlist: BTreeMap::new(),
            filter_to_remove: HashSet::new(),
            selected_process_pid: None,
            allowlist: BTreeSet::new(),
            allowlist_input: String::new(),
            kill_hotkey_registered: false,
            hotkey_rx: rx,
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
        let mut app: TemplateApp = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };

        // handling the politely killing listener
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        start_kill_hotkey_listener(tx);
        app.hotkey_rx = rx;

        app

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
                if is_pseudo_open_in_taskbar(maybe_hwnd.unwrap()) {
                    // we don't strip file extension at the source because we will use in the actual allowlist,
                    // so it's removed only in display
                    self.processlist.insert(
                        process.name().to_string_lossy().parse().unwrap(),
                        pid.as_u32(),
                    );
                };
            }

            for key in &self.allowlist {
                self.processlist.remove(key.as_str());
            }

            // and filtering it
            for (name, pid) in &self.processlist {
                if !loosely_check_if_real_app(&pid, &name) || name.as_str() == "expurgate.exe" {
                    self.filter_to_remove.insert(name.clone());
                };
            }

            for name in &self.filter_to_remove {
                &self.processlist.remove(name.as_str());
            }

            // handling kill hotkey
            while let Ok(e) = self.hotkey_rx.try_recv() {
                match e {
                    HotkeyEvent::Kill => {
                        println!("Polite murder initiated.");
                        for (_, pid) in &self.processlist {
                            close_by_pid(pid).unwrap();
                        }
                    }
                }
            }

            // ui:

            ui.heading("allowlist");

            ui.horizontal(|ui| {
                ui.label("Add to allowlist");
                ui.text_edit_singleline(&mut self.allowlist_input);
            });

            if ui.button("Add").clicked() {
                let item = self.allowlist_input.trim();
                if !item.is_empty() {
                    self.allowlist.insert(item.to_string());
                    self.allowlist_input.clear();
                }
            }

            ui.separator();

            ui.label("allowlist in question:");
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .id_salt("scrollin_30x9403mcd2")
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    let mut to_remove = None;

                    for name in &self.allowlist {
                        ui.horizontal(|ui| {
                            if ui.button("-").clicked() {
                                to_remove = Some(name.clone());
                            }

                            ui.add_sized(
                                [50.0, 20.0],
                                egui::Label::new(strip_file_extension(name)),
                            );
                        });
                    }

                    if let Some(name) = to_remove {
                        &self.allowlist.remove(&name);
                    }
                });

            ui.separator();
            ui.label("Death note");

            if ui.button("Close Notepad politely").clicked() {
                close_by_pid(&24588).unwrap();
            }

            if ui.button("Kill them all.").clicked() {
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
                                    self.allowlist.insert(name.to_string());
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
