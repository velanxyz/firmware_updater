# Firmware Updater

Приложение на Rust + Slint для авто-обновления прошивок и ПО USB-устройств через базу Supabase.

## Скачать
[![Download Releases](https://img.shields.io/badge/Скачать-Releases-2ea44f?style=for-the-badge&logo=github)](https://github.com/velanxyz/firmware_updater/releases)

Готовую скомпилированную версию для Windows можно скачать по кнопке выше.

## Поддерживаемые устройства

*   **Logitech**: G Pro X Superlight 1/2, G502 (все версии), G102-G903, серия MX.
*   **Razer**: Viper, DeathAdder, Basilisk, Naga, Cobra, Mamba, Orochi, Lancehead.
*   **Lamzu**: Atlantis (Wireless).
*   **Ninjutso**: Sora (Wireless).
*   **Zowie**: EC2-CW.
*   **Glorious**: Model O 2 Wireless.
*   **SteelSeries**: Aerox 3 Wireless.
*   **HyperX**: Pulsefire Haste 2 Wireless.

> База поддерживаемых устройств будет постоянно пополняться.

## Стек технологий
*   **Rust**, **Slint** (GUI), **hidapi** (USB), **reqwest/tokio** (Сеть), **Supabase** (Бэкенд).

## Настройка и запуск (для разработчиков)

1.  Создайте `.env` файл в корне:
    ```env
    SUPABASE_URL=ваш_url
    SUPABASE_KEY=ваш_key
    ```
2.  Запуск в режиме разработки:
    ```bash
    cargo run
    ```
3.  Сборка релиза (Windows без консоли):
    ```bash
    cargo build --release
    ```

## Структура
*   `src/main.rs` — Логика UI и связывание модулей.
*   `src/usb.rs` — Сканирование HID устройств.
*   `src/network.rs` — API запросы и загрузка файлов.
*   `ui/appwindow.slint` — Графический интерфейс.
