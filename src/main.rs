#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hidapi::HidApi;
use serde::Deserialize;
use serde_json::Value;
use slint::{SharedString, VecModel, Weak};
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

slint::include_modules!();

// Функция для сокращения длинных названий ПО
fn shorten_software_name(name: &str) -> &str {
    match name {
        "Onboard Memory Manager" => "OMM",
        _ => name,
    }
}

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

// --- ФУНКЦИИ РАБОТЫ С СЕРВЕРОМ ---

async fn fetch_database(
    supabase_url: &str,
    supabase_key: &str,
) -> Vec<SupportedDevice> {
    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert("apikey", HeaderValue::from_str(supabase_key).unwrap());
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", supabase_key)).unwrap(),
    );

    // supabase_url = https://...supabase.co
    let url = format!("{}/rest/v1/mouse?select=*", supabase_url);

    match client.get(&url).headers(headers).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(raw_devices) = resp.json::<Vec<Value>>().await {
                    let mut devices = Vec::new();
                    for raw in raw_devices {
                        // Пробуем сначала "options", потом "jsonb" для обратной совместимости
                        let options_json = raw.get("options")
                            .or_else(|| raw.get("jsonb"));
                        
                        if let (Some(name), Some(vid), Some(pid), Some(options_json)) = (
                            raw.get("name").and_then(|v| v.as_str()),
                            raw.get("vid").and_then(|v| v.as_u64()),
                            raw.get("pid").and_then(|v| v.as_u64()),
                            options_json,
                        ) {
                            if let Ok(options) = serde_json::from_value::<Vec<SoftwareOption>>(options_json.clone()) {
                                devices.push(SupportedDevice {
                                    name: name.to_string(),
                                    vid: vid as u16,
                                    pid: pid as u16,
                                    options,
                                });
                            }
                        }
                    }
                    return devices;
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

// --- ПОИСК УСТРОЙСТВ ---

fn scan_usb(database: &[SupportedDevice]) -> Vec<SupportedDevice> {
    let mut found = Vec::new();

    if let Ok(api) = HidApi::new() {
        for device in api.device_list() {
            let vid = device.vendor_id();
            let pid = device.product_id();
            let mut exact_match = false;

            // 1. Точное совпадение VID/PID в базе
            for supported in database {
                if vid == supported.vid && pid == supported.pid {
                    if !found.iter().any(|d: &SupportedDevice| d.name == supported.name) {
                        found.push(supported.clone());
                    }
                    exact_match = true;
                    break;
                }
            }

            // 2. Fallback для Logitech
            if !exact_match {
                if let Some(generic) =
                    get_vendor_fallback(vid, pid, device.product_string())
                {
                    if !found.iter().any(|d: &SupportedDevice| d.name == generic.name) {
                        found.push(generic);
                    }
                }
            }
        }
    }

    found
}

fn get_vendor_fallback(
    vid: u16,
    pid: u16,
    product_name: Option<&str>,
) -> Option<SupportedDevice> {
    let dev_name = product_name.unwrap_or("Unknown").to_string();

    match vid {
        0x046d => Some(SupportedDevice {
            name: format!("Logitech Device ({})", dev_name),
            vid,
            pid,
            options: vec![SoftwareOption {
                name: "Logitech G HUB".into(),
                description: "Основной драйвер".into(),
                url: "https://download01.logi.com/web/ftp/pub/techsupport/gaming/lghub_installer.exe"
                    .into(),
                filename: "lghub.exe".into(),
            }],
        }),
        _ => None,
    }
}

// --- СКАЧИВАНИЕ ФАЙЛА ---

async fn download_file_async(
    url: String,
    filename: String,
    ui_handle: Weak<AppWindow>,
) -> Result<std::path::PathBuf, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Ошибка сервера: {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);

    let mut dest_path = std::env::temp_dir();
    dest_path.push(filename);

    let mut file =
        tokio::fs::File::create(&dest_path).await.map_err(|e| e.to_string())?;
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