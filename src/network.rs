use crate::models::SupportedDevice;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::Value;
use slint::Weak;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;

slint::include_modules!();

// --- ФУНКЦИИ РАБОТЫ С СЕРВЕРОМ ---

pub async fn fetch_database(
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
                            if let Ok(options) = serde_json::from_value::<Vec<crate::models::SoftwareOption>>(options_json.clone()) {
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

// --- СКАЧИВАНИЕ ФАЙЛА ---

pub async fn download_file_async(
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

