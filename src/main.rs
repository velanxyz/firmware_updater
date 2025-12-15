#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use hidapi::HidApi;
use serde::Deserialize;
use slint::{Model, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;

slint::include_modules!();

#[derive(Deserialize, Clone, Debug)]
struct SoftwareOption {
    name: String,
    description: String,
    url: String,
    filename: String,
}

#[derive(Deserialize, Clone, Debug)]
struct SupportedDevice {
    name: String,
    vid: u16,
    pid: u16,
    options: Vec<SoftwareOption>,
}

struct AppState {
    found_devices: Vec<SupportedDevice>,
}

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();
    
    let state = Rc::new(RefCell::new(AppState {
        found_devices: Vec::new(),
    }));

    // --- КНОПКА СКАНИРОВАТЬ ---
    let state_scan = state.clone();
    let ui_scan = ui_handle.clone();
    ui.on_scan_clicked(move || {
        let ui = ui_scan.unwrap();
        let mut state = state_scan.borrow_mut();
        
        let database = load_database(); 
        let found = scan_usb(&database); // <-- Вся магия теперь внутри этой функции
        state.found_devices = found.clone();

        let names: Vec<SharedString> = found.iter().map(|d| d.name.clone().into()).collect();
        ui.set_device_list(Rc::new(VecModel::from(names)).into());

        if found.is_empty() {
            ui.set_status_text("Устройства не найдены".into());
            ui.set_current_device_index(-1);
        } else {
            ui.set_status_text(format!("Найдено: {}", found.len()).into());
            ui.set_current_device_index(0);
        }
    });

    // --- ВЫБОР УСТРОЙСТВА ---
    let ui_select = ui_handle.clone();
    ui.on_device_selected(move |index| {
        let ui = ui_select.unwrap();
        if index >= 0 {
            ui.set_status_text("Готов к загрузке".into());
        }
    });

    // --- СКАЧИВАНИЕ ---
    let state_dl = state.clone();
    let ui_dl = ui_handle.clone();
    
    ui.on_download_clicked(move |option_index| {
        let ui = ui_dl.unwrap();
        let state = state_dl.borrow();
        let device_idx = ui.get_current_device_index();
        
        if device_idx < 0 || device_idx as usize >= state.found_devices.len() { return; }
        
        let device = &state.found_devices[device_idx as usize];
        if option_index as usize >= device.options.len() { return; }
        
        let option = device.options[option_index as usize].clone();
        
        ui.set_is_downloading(true);
        ui.set_progress(0.0);
        ui.set_status_text(format!("Скачивание {}...", option.name).into());

        let ui_async = ui_dl.clone();
        tokio::spawn(async move {
            let result = download_file_async(option.url, option.filename, ui_async.clone()).await;
            
            let _ = slint::invoke_from_event_loop(move || {
                let ui = ui_async.unwrap();
                ui.set_is_downloading(false);
                
                match result {
                    Ok(path) => {
                        ui.set_status_text("Запуск установщика...".into());
                        let _ = Command::new(path).spawn();
                    }
                    Err(e) => {
                        ui.set_status_text(format!("Ошибка: {}", e).into());
                    }
                }
            });
        });
    });

    let ui_auto = ui_handle.clone();
    slint::invoke_from_event_loop(move || {
         ui_auto.unwrap().invoke_scan_clicked();
    }).unwrap();

    ui.run()
}

// --- ФУНКЦИИ ---

fn load_database() -> Vec<SupportedDevice> {
    if let Ok(file) = File::open("devices.json") {
        let reader = BufReader::new(file);
        if let Ok(json_db) = serde_json::from_reader(reader) {
            return json_db;
        }
    }
    let embedded_json = include_str!("../devices.json");
    serde_json::from_str(embedded_json).unwrap_or_else(|_| Vec::new())
}

// ГЛАВНАЯ ЛОГИКА ПОИСКА И БРЕНДОВ
fn scan_usb(database: &[SupportedDevice]) -> Vec<SupportedDevice> {
    let mut found = Vec::new();
    
    // Множество уже добавленных VID (чтобы не дублировать)
    // Если мы нашли конкретную мышь Razer, мы не должны добавлять "Generic Razer"
    let mut processed_vids = Vec::new(); 

    if let Ok(api) = HidApi::new() {
        for device in api.device_list() {
            let vid = device.vendor_id();
            let pid = device.product_id();

            // 1. Сначала ищем ТОЧНОЕ совпадение в базе (devices.json)
            let mut exact_match = false;
            for supported in database {
                if vid == supported.vid && pid == supported.pid {
                    if !found.iter().any(|d: &SupportedDevice| d.name == supported.name) {
                        found.push(supported.clone());
                        processed_vids.push(vid); // Запоминаем, что этот бренд уже обработан точно
                    }
                    exact_match = true;
                    break;
                }
            }

            // 2. Если точного совпадения НЕТ, проверяем БРЕНД (VID)
            if !exact_match {
                // Если мы еще не находили устройств этого бренда
                 // (или можно убрать эту проверку, если хочешь видеть все девайсы)
                if let Some(generic_device) = get_vendor_fallback(vid, pid, device.product_string()) {
                    // Проверяем, чтобы не добавить одну и ту же "Generic Mouse" 10 раз
                    if !found.iter().any(|d: &SupportedDevice| d.name == generic_device.name) {
                        found.push(generic_device);
                    }
                }
            }
        }
    }
    found
}

// База данных брендов (Метод 4 - Hardcoded VIDs)
fn get_vendor_fallback(vid: u16, pid: u16, product_name: Option<&str>) -> Option<SupportedDevice> {
    // Получаем имя устройства из USB или ставим заглушку
    let dev_name = product_name.unwrap_or("Unknown Device").to_string();
    
    match vid {
        // RAZER (0x1532)
        0x1532 => Some(SupportedDevice {
            name: format!("Razer Device ({})", dev_name),
            vid, pid,
            options: vec![
                SoftwareOption {
                    name: "Razer Synapse 3".to_string(),
                    description: "Универсальное ПО для Razer".to_string(),
                    url: "https://rzr.to/synapse-3-pc".to_string(),
                    filename: "RazerSynapseInstaller.exe".to_string(),
                }
            ]
        }),
        // STEELSERIES (0x1038)
        0x1038 => Some(SupportedDevice {
            name: format!("SteelSeries ({})", dev_name),
            vid, pid,
            options: vec![
                SoftwareOption {
                    name: "SteelSeries GG".to_string(),
                    description: "Engine + Sonar".to_string(),
                    url: "https://steelseries.com/gg/downloads/gg/windows/json".to_string(), // Внимание: тут часто переадресация, лучше проверить ссылку
                    // Лучше использовать прямую, но она часто меняется. Пока оставим для примера.
                    filename: "SteelSeriesGG.exe".to_string(),
                }
            ]
        }),
        // HYPERX (0x0951)
        0x0951 => Some(SupportedDevice {
            name: format!("HyperX ({})", dev_name),
            vid, pid,
            options: vec![
                SoftwareOption {
                    name: "NGENUITY (MS Store)".to_string(),
                    description: "Перенаправит в магазин приложений".to_string(),
                    // HyperX сложный, они в Microsoft Store. Можно кинуть на сайт.
                    url: "https://hyperx.com".to_string(), 
                    filename: "hyperx.html".to_string(), // Откроет браузер
                }
            ]
        }),
         // CORSAIR (0x1b1c)
         0x1b1c => Some(SupportedDevice {
            name: format!("Corsair ({})", dev_name),
            vid, pid,
            options: vec![
                SoftwareOption {
                    name: "iCUE".to_string(),
                    description: "Полный пакет драйверов".to_string(),
                    url: "https://downloads.corsair.com/Files/CUE/iCUESetup_5.14.93_release.msi".to_string(),
                    filename: "iCUE_Setup.msi".to_string(),
                }
            ]
        }),
        // LOGITECH (0x046d) - Фолбэк, если не G Pro X
        0x046d => Some(SupportedDevice {
            name: format!("Logitech Device ({})", dev_name),
            vid, pid,
            options: vec![
                SoftwareOption {
                    name: "Logitech G HUB".to_string(),
                    description: "Универсальный драйвер".to_string(),
                    url: "https://download01.logi.com/web/ftp/pub/techsupport/gaming/lghub_installer.exe".to_string(),
                    filename: "lghub_installer.exe".to_string(),
                }
            ]
        }),
        _ => None // Бренд не известен
    }
}

async fn download_file_async(url: String, filename: String, ui_handle: Weak<AppWindow>) -> Result<PathBuf, String> {
    let client = reqwest::Client::new();
    let response = client.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Код ответа: {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut dest_path = std::env::temp_dir();
    dest_path.push(filename);
    
    let mut file = tokio::fs::File::create(&dest_path).await.map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        
        if total_size > 0 {
            let progress = downloaded as f32 / total_size as f32;
            let ui_weak = ui_handle.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_progress(progress);
                }
            });
        }
    }
    Ok(dest_path)
}