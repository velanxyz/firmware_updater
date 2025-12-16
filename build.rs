fn main() {
    slint_build::compile("ui/appwindow.slint").unwrap();
    if let Ok(iter) = dotenvy::from_filename_iter(".env") {
        for item in iter {
            if let Ok((key, value)) = item {
                println!("cargo:rustc-env={}={}", key, value);
            }
        }
    } else {
        println!("cargo:warning=Файл .env не найден при сборке. Убедитесь, что переменные заданы.");
    }
}