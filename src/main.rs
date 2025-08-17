#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::sync::mpsc;
use std::thread;
use tray_item::{IconSource, TrayItem};

enum Message {
    Quit,
    Green,
    Red,
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    thread::spawn(|| {
        let mut tray = TrayItem::new(
            "Tray Example",
            IconSource::Resource("test"),
        )
            .unwrap();

        tray.add_label("Tray Label").unwrap();

        tray.add_menu_item("Hello", || {
            println!("Hello!");
        })
            .unwrap();

        tray.inner_mut().add_separator().unwrap();

        let (tx, rx) = mpsc::sync_channel(1);

        let red_tx = tx.clone();
        tray.add_menu_item("Red", move || {
            red_tx.send(Message::Red).unwrap();
        })
            .unwrap();

        let green_tx = tx.clone();
        tray.add_menu_item("Green", move || {
            green_tx.send(Message::Green).unwrap();
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
                Ok(Message::Red) => {
                    println!("Red");
                    tray.set_icon(IconSource::Resource("test"))
                        .unwrap();
                }
                Ok(Message::Green) => {
                    println!("Green");
                    tray.set_icon(IconSource::Resource("test"))
                        .unwrap()
                }
                _ => {}
            }
        }
    });

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([350.0, 450.0])
            .with_max_inner_size([350.0, 450.0])
            .with_min_inner_size([350.0, 450.0])
            .with_icon(
                // NOTE: Adding an icon is optional
                eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon-256.png")[..])
                    .expect("Failed to load icon"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "expurgate",
        native_options,
        Box::new(|cc| Ok(Box::new(expurgate::Expurgate::new(cc)))),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(eframe_template::TemplateApp::new(cc)))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}
