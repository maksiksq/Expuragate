use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tray_item::{IconSource, TrayItem};
// cfg to enable cpu render if ram gets pushy later

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey,
};

use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GWL_EXSTYLE, GetAncestor, GetLastActivePopup,
    GetMessageW, GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible,
    MSG, PostMessageW, WM_CLOSE, WM_HOTKEY, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, Result};

enum Message {
    Quit,
    Hide,
    Unhide,
}


#[allow(unsafe_code)]
pub fn is_pseudo_open_in_taskbar(mut hwnd: HWND, show_all_processes: bool) -> bool {
    if show_all_processes {
        return true;
    }
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
            size_of::<u32>() as u32,
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
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
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

// future use
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
        // TODO: handle these two manually later
        || lower.contains("explorer")
        || lower.contains("taskmgr")
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
pub struct Expurgate {
    #[serde(skip)]
    sys: System,

    #[serde(skip)]
    processlist: BTreeMap<String, u32>,

    #[serde(skip)]
    unf_processlist: BTreeMap<String, u32>,

    #[serde(skip)]
    filter_to_remove: HashSet<String>,

    #[serde(skip)]
    selected_process_pid: Option<u32>,

    allowlist: BTreeSet<String>,

    #[serde(skip)]
    kill_hotkey_registered: bool,

    #[serde(skip)]
    hotkey_rx: Receiver<HotkeyEvent>,

    show_all_processes: bool,

    killlist: BTreeSet<String>,
}

impl Default for Expurgate {
    fn default() -> Self {
        // dummy sender
        let (_tx, rx) = mpsc::channel();

        Self {
            sys: System::new_all(),
            processlist: BTreeMap::new(),
            unf_processlist: BTreeMap::new(),
            filter_to_remove: HashSet::new(),
            selected_process_pid: None,
            allowlist: BTreeSet::new(),
            kill_hotkey_registered: false,
            hotkey_rx: rx,
            show_all_processes: false,
            killlist: BTreeSet::new(),
        }
    }
}

impl Expurgate {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();

        thread::spawn({
            let ctx = ctx.clone();
            move || {
                let mut tray = TrayItem::new(
                    "Tray",
                    IconSource::Resource("icon"),
                )
                    .unwrap();

                tray.add_label("Tray Label").unwrap();

                tray.add_menu_item("Hello", || {
                    println!("Hello!");
                })
                    .unwrap();

                tray.inner_mut().add_separator().unwrap();

                let (tx, rx) = mpsc::sync_channel(1);

                let hide_tx = tx.clone();
                tray.add_menu_item("Hide", move || {
                    hide_tx.send(Message::Hide).unwrap();
                })
                    .unwrap();

                let unhide_tx = tx.clone();
                tray.add_menu_item("Unhide", move || {
                    unhide_tx.send(Message::Unhide).unwrap();
                })
                    .unwrap();


                tray.inner_mut().add_separator().unwrap();

                let quit_tx = tx.clone();
                tray.add_menu_item("Quit", move || {
                    quit_tx.send(Message::Quit).unwrap();
                })
                    .unwrap();

                loop {
                    match rx.recv() {
                        Ok(Message::Quit) => {
                            println!("Quit");
                            break;
                        }
                        Ok(Message::Hide) => {
                            println!("Hide");
                            // We hackily do the hiding by just making the app 0 pixels in size
                            // The visibility of the window never toggles back because of an eFrame bug (egui #5229)
                            // so oh well
                            let viewport = egui::ViewportId::ROOT;
                            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Decorations(false));
                            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::InnerSize([0.0, 0.0].into()));
                            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::OuterPosition(egui::pos2(-10000.0, -10000.0)));
                            ctx.request_repaint();
                        },
                        Ok(Message::Unhide) => {
                            println!("Unhide");
                            let viewport = egui::ViewportId::ROOT;
                            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Decorations(true));
                            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::InnerSize([350.0, 480.0].into()));
                            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::OuterPosition(egui::pos2(200.0, 200.0)));
                            ctx.request_repaint();
                        }
                        _ => {}
                    }
                }
            }
        });

        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let mut app: Expurgate = if let Some(storage) = cc.storage {
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

impl eframe::App for Expurgate {
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
                if is_pseudo_open_in_taskbar(maybe_hwnd.unwrap(), false) {
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
                self.processlist.remove(name.as_str());
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

            ui.separator();

            ui.label("allowlist in question:");
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .id_salt("scrollin_allowlist_30x9403mcd2")
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
                        self.allowlist.remove(&name);
                    }
                });

            ui.separator();
            ui.label("Tax Evaders:");
            ui.label("These are closed whenever you please. Ctrl+Alt+J");

            // if ui.button("Close Notepad politely").clicked() {
            //     close_by_pid(&24588).unwrap();
            // }

            if ui.button("Kill them all.").clicked() {
                for (_, pid) in &self.processlist {
                    close_by_pid(pid).unwrap();
                }
                for (name, pid) in &self.unf_processlist {
                    if self.killlist.contains(name) {
                        println!("hai: {}", name);
                        close_by_pid(pid).unwrap();
                    }
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

            // ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            // });

            ui.separator();

            ui.checkbox(&mut self.show_all_processes, "Advanced");
            if !self.show_all_processes {
                return;
            }
            ui.heading("advanced");
            ui.separator();

            ui.label("Explicit killlist");
            ui.label("Here you can pick processes to kill if they do not appear up there, some (e.g. Figma) bypass my filters (for now).");

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .id_salt("scrollin_killist_30x9403mcd2")
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    let mut to_remove = None;

                    for name in &self.killlist {
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
                        self.killlist.remove(&name);
                    }
                });


            ui.label("Note: System processes and most others will not be closed since the app does not kill them but asks to close the window instead.");

            // populating unfiltered processlist
            self.unf_processlist.clear();
            for (pid, process) in self.sys.processes() {
                let maybe_hwnd: Option<HWND> = get_hwnd_by_pid(pid.as_u32());
                if maybe_hwnd.is_none() {
                    continue;
                }
                self.unf_processlist.insert(
                    process.name().to_string_lossy().parse().unwrap(),
                    pid.as_u32(),
                );
                if is_pseudo_open_in_taskbar(maybe_hwnd.unwrap(), true) {
                    // we don't strip file extension at the source because we will use in the actual allowlist,
                    // so it's removed only in display
                    self.unf_processlist.insert(
                        process.name().to_string_lossy().parse().unwrap(),
                        pid.as_u32(),
                    );
                };
            }

            egui::ScrollArea::vertical()
                .id_salt("cool-scrollarea-wahoo235235")
                .max_height(301.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.horizontal(|ui| {
                        ui.add_sized([50.0, 20.0], egui::Label::new("PID"));
                        ui.add_sized([50.0, 20.0], egui::Label::new("Process Name"));
                    });

                    for (name, pid) in &self.unf_processlist {
                        ui.push_id(*pid, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("+").clicked() {
                                    self.killlist.insert(name.to_string());
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
        });
    }
}
