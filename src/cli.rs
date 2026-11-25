//! Ручной парсер аргументов (без clap): CLI патчера предельно прост,
//! а отсутствие внешних зависимостей с windows-sys позволяет собирать
//! windows-бинарник кросс-компиляцией без MinGW.

pub struct Args {
    pub command: String,
    pub flags: std::collections::HashMap<String, String>,
    pub booleans: std::collections::HashSet<String>,
}

pub const USAGE: &str = "MakhnoYMMod 2.2.3 — патчер клиента Яндекс Музыки

Использование: makhno-patcher <COMMAND> [ОПЦИИ]

Команды:
  patch    Встроить мод в сборку клиента
  verify   Проверить пропатченную сборку
  unpack   Распаковать asar в каталог (утилита отладки)
  pack     Упаковать каталог в asar (утилита отладки)
  help     Показать эту справку

Опции patch:
      --app-dir <DIR>        Каталог клиента (содержит resources/app.asar), .deb-файл,
                             .exe-инсталлятор или .asar-файл
      --payload-dir <DIR>    Каталог payload (по умолчанию: ./payload,
                             <bin_dir>/payload, <bin_dir>/../payload)
      --repo-url <URL>       URL репозитория мода (кнопки в UI)
                             [по умолчанию: https://github.com/Ortodoxian88/MakhnoYMMod]
      --meta-url <URL>       URL meta.json для проверки обновлений
                             (по умолчанию выводится из --repo-url)
      --keep-web-security    Сохранить webSecurity: true (лирика musixmatch может не работать)
      --bundle-ffmpeg <PATH> Вшить ffmpeg-бинарник вместо системного ffmpeg
      --no-deb               Не пересобирать deb, даже если это deb-корень
      --dry-run              Только отчёт, без записи app.asar

Опции verify:
      --target <DIR|ASAR>    Каталог клиента или asar-файл

Опции unpack:
      --asar <ASAR> --out <DIR>

Опции pack:
      --dir <DIR> --out <ASAR>

Опции (общие):
  -V, --version              Версия патчера

Флаги значения можно передавать и в форме --флаг=значение.
";

pub fn parse(argv: Vec<String>) -> Result<Args, String> {
    let mut it = argv.into_iter();
    let first = it.next().ok_or("не указана команда (см. makhno-patcher help)")?;
    if first == "help" || first == "--help" || first == "-h" {
        print!("{USAGE}");
        std::process::exit(0);
    }
    if first == "--version" || first == "-V" {
        println!("makhno-patcher {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    let command = first;
    let mut flags = std::collections::HashMap::new();
    let mut booleans = std::collections::HashSet::new();
    const VALUE_FLAGS: [&str; 9] = [
        "--app-dir",
        "--payload-dir",
        "--repo-url",
        "--meta-url",
        "--bundle-ffmpeg",
        "--target",
        "--asar",
        "--out",
        "--dir",
    ];
    const BOOL_FLAGS: [&str; 3] = ["--keep-web-security", "--no-deb", "--dry-run"];
    while let Some(arg) = it.next() {
        if arg == "help" || arg == "--help" || arg == "-h" {
            print!("{USAGE}");
            std::process::exit(0);
        }
        if arg == "--version" || arg == "-V" {
            println!("makhno-patcher {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        if arg.starts_with("--") {
            // поддержка --флаг=значение рядом с классической формой --флаг значение
            if let Some((name, val)) = arg.strip_prefix("--").and_then(|a| a.split_once('=')) {
                let full = format!("--{name}");
                if !VALUE_FLAGS.contains(&full.as_str()) {
                    return Err(format!("неизвестный флаг: {arg}"));
                }
                flags.insert(name.to_string(), val.to_string());
                continue;
            }
            if VALUE_FLAGS.contains(&arg.as_str()) {
                let val = it.next().ok_or(format!("{arg}: требуется значение"))?;
                flags.insert(arg.trim_start_matches("--").to_string(), val);
            } else if BOOL_FLAGS.contains(&arg.as_str()) {
                booleans.insert(arg.trim_start_matches("--").to_string());
            } else {
                // опечатки больше не проходят молча: раньше любой неизвестный
                // флаг тихо превращался в бездействующий булев
                return Err(format!("неизвестный флаг: {arg}"));
            }
        } else {
            return Err(format!("неизвестный аргумент: {arg}"));
        }
    }
    Ok(Args { command, flags, booleans })
}
