#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod network;
mod usb;

use models::{AppState, shorten_software_name};
use network::{fetch_database, download_file_async};
use network::AppWindow;
use usb::scan_usb;
use slint::{ComponentHandle, SharedString, VecModel};
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    // Ключи, "запеченные" при сборке через build.rs
    let supabase_url = env!("SUPABASE_URL").to_string();
    let supabase_key = env!("SUPABASE_KEY").to_string();

    let ui = AppWindow::new()?;
    
    let ui_handle = ui.as_weak();

    let state = Arc::new(Mutex::new(AppState {
        found_devices: Vec::new(),
    }));

    let supabase_url_clone = supabase_url.clone();
    let supabase_key_clone = supabase_key.clone();

    // --- КНОПКА СКАНИРОВАТЬ ---
    let state_scan = state.clone();
    let ui_scan_handle = ui_handle.clone();
    let ui_scan_async_handle = ui_handle.clone();

    ui.on_scan_clicked(move || {
        let ui = ui_scan_handle.unwrap();
        ui.set_status_text("Подключение к облаку...".into());

        let state_inner = state_scan.clone();
        let ui_weak = ui_scan_async_handle.clone();
        let url = supabase_url_clone.clone();
        let key = supabase_key_clone.clone();

        tokio::spawn(async move {
            let database = fetch_database(&url, &key).await;
            let found = scan_usb(&database);

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    let mut state = state_inner.lock().unwrap();
                    state.found_devices = found.clone();

                    let names: Vec<SharedString> =
                        found.iter().map(|d| d.name.clone().into()).collect();
                    ui.set_device_list(Rc::new(VecModel::from(names)).into());

                    if found.is_empty() {
                        ui.set_status_text("Устройства не найдены".into());
                        ui.set_current_device_index(-1);
                        ui.set_software_options(Rc::new(VecModel::default()).into());
                    } else {
                        ui.set_status_text(format!("Найдено: {}", found.len()).into());
                        ui.set_current_device_index(0);
                        // Обновляем опции для первого устройства
                        if !found.is_empty() {
                            let device = &found[0];
                            let option_names: Vec<SharedString> = 
                                device.options.iter()
                                    .map(|opt| format!("Скачать {}", shorten_software_name(&opt.name)).into())
                                    .collect();
                            ui.set_software_options(Rc::new(VecModel::from(option_names)).into());
                        }
                    }
                }
            });
        });
    });

    // --- ВЫБОР УСТРОЙСТВА ---
    let state_select = state.clone();
    let ui_select = ui_handle.clone();
    ui.on_device_selected(move |index| {
        if let Some(ui) = ui_select.upgrade() {
            if index >= 0 {
                let state = state_select.lock().unwrap();
                if (index as usize) < state.found_devices.len() {
                    let device = &state.found_devices[index as usize];
                    // Обновляем список опций ПО в UI
                    let option_names: Vec<SharedString> = 
                        device.options.iter()
                            .map(|opt| format!("Скачать {}", shorten_software_name(&opt.name)).into())
                            .collect();
                    ui.set_software_options(Rc::new(VecModel::from(option_names)).into());
                    ui.set_status_text("Готов к загрузке".into());
                }
            } else {
                // Очищаем список опций, если устройство не выбрано
                ui.set_software_options(Rc::new(VecModel::default()).into());
            }
        }
    });

    // --- КНОПКА СКАЧИВАНИЯ ---
    let state_dl = state.clone();
    let ui_dl = ui_handle.clone();
    let ui_dl_async = ui_handle.clone();

    ui.on_download_clicked(move |option_index| {
        let ui = ui_dl.unwrap();
        let state = state_dl.lock().unwrap();

        let device_idx = ui.get_current_device_index();
        if device_idx < 0 || device_idx as usize >= state.found_devices.len() {
            return;
        }

        let device = &state.found_devices[device_idx as usize];
        if option_index as usize >= device.options.len() {
            return;
        }

        let option = device.options[option_index as usize].clone();
        drop(state);

        ui.set_is_downloading(true);
        ui.set_progress(0.0);
        ui.set_status_text(format!("Скачивание {}...", option.name).into());

        let ui_async = ui_dl_async.clone();

        tokio::spawn(async move {
            let result =
                download_file_async(option.url, option.filename, ui_async.clone()).await;

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_async.upgrade() {
                    ui.set_is_downloading(false);
                    match result {
                        Ok(path) => {
                            ui.set_status_text("Запуск установщика...".into());
                            let _ = Command::new(path).spawn();
                        }
                        Err(e) => ui.set_status_text(format!("Ошибка: {}", e).into()),
                    }
                }
            });
        });
    });

    // --- ПОКАЗ ОПИСАНИЯ ---
    let state_desc = state.clone();
    let ui_desc = ui_handle.clone();
    ui.on_show_description(move |option_index| {
        if let Some(ui) = ui_desc.upgrade() {
            let state = state_desc.lock().unwrap();
            let device_idx = ui.get_current_device_index();
            if device_idx >= 0 && (device_idx as usize) < state.found_devices.len() {
                let device = &state.found_devices[device_idx as usize];
                if (option_index as usize) < device.options.len() {
                    let option = &device.options[option_index as usize];
                    ui.set_description_title(option.name.clone().into());
                    ui.set_description_text(option.description.clone().into());
                    ui.set_show_description_dialog(true);
                }
            }
        }
    });

    // --- ЗАКРЫТИЕ ОПИСАНИЯ ---
    let ui_close_desc = ui_handle.clone();
    ui.on_close_description(move || {
        if let Some(ui) = ui_close_desc.upgrade() {
            ui.set_show_description_dialog(false);
        }
    });

    // Авто-сканирование при старте
    let ui_auto = ui_handle.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_auto.upgrade() {
            ui.invoke_scan_clicked();
        }
    })
    .unwrap();

    ui.run()
}
