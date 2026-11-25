//! Проверка пропатченной сборки: содержимое index.js/preload.js/package.json/renderer.js,
//! структура asar, node_modules, синтаксис через node --check (если node есть).

use crate::asar::AsarArchive;
use crate::patch;
use std::path::Path;
use std::process::Command;

pub struct Verifier {
    pub ok: usize,
    pub fail: usize,
}

impl Verifier {
    fn chk(&mut self, cond: bool, what: &str) {
        if cond {
            println!("  [OK] {what}");
            self.ok += 1;
        } else {
            println!("  [FAIL] {what}");
            self.fail += 1;
        }
    }

    fn node_check(&mut self, code: &str, label: &str) {
        if which_node().is_none() {
            return;
        }
        // PID в имени: параллельные проверки не конфликтуют временными файлами
        let tmp = std::env::temp_dir().join(format!(
            "makhno_check_{}_{}.js",
            std::process::id(),
            label.replace('/', "_")
        ));
        if std::fs::write(&tmp, code).is_err() {
            return;
        }
        let status = Command::new(which_node().unwrap())
            .arg("--check")
            .arg(&tmp)
            .output();
        let good = matches!(&status, Ok(o) if o.status.success());
        let _ = std::fs::remove_file(&tmp);
        self.chk(good, &format!("node --check {label}: OK"));
    }
}

fn which_node() -> Option<String> {
    let n = if cfg!(windows) { "node.exe" } else { "node" };
    match Command::new(n).arg("--version").output() {
        Ok(o) if o.status.success() => Some(n.to_string()),
        _ => None,
    }
}

pub fn verify_asar(asar_path: &Path, asar: &AsarArchive) -> Result<Verifier, String> {
    println!("== MakhnoYMMod {} verify ==", crate::payload::MOD_VERSION);
    println!("цель: {}", asar_path.display());
    let mut v = Verifier { ok: 0, fail: 0 };

    let index = String::from_utf8(asar.get("index.js").ok_or("нет index.js")?.to_vec())
        .map_err(|_| "index.js не utf-8".to_string())?;
    let preload = String::from_utf8(asar.get("preload.js").ok_or("нет preload.js")?.to_vec())
        .map_err(|_| "preload.js не utf-8".to_string())?;
    let pkg_txt = String::from_utf8(asar.get("package.json").ok_or("нет package.json")?.to_vec())
        .map_err(|_| "package.json не utf-8".to_string())?;
    let pkg: serde_json::Value = serde_json::from_str(&pkg_txt).map_err(|e| e.to_string())?;
    let renderer = String::from_utf8(
        asar.get("app/makhnoYMMod/renderer.js")
            .ok_or("нет app/makhnoYMMod/renderer.js")?
            .to_vec(),
    )
    .map_err(|_| "renderer.js не utf-8".to_string())?;

    // --- index.js ---
    v.chk(index.starts_with(patch::HEAD_BEGIN), "index.js: пролог мода");
    v.chk(index.contains("// MakhnoYMMod main begin") && index.contains("// MakhnoYMMod main end"), "index.js: main-блок с маркерами");
    v.chk(!index.contains("updater.start();"), "index.js: вызова updater.start() нет");
    // У клиента 5.86 апдейтер устроен иначе: проверяем отсутствие живого конструктора.
    v.chk(
        !index.contains("new Logger(\"UpdateLogger\")") || index.contains("constructor() { return"),
        "index.js: конструктор апдейтера вырезан",
    );
    v.chk(index.contains("makhnoYMMod.getStorageValue"), "index.js: IPC getStorageValue");
    v.chk(index.contains("makhnoYMMod.downloadTrack"), "index.js: загрузчик треков");
    v.chk(index.contains("encoded_by"), "index.js: метка encoded_by");
    v.chk(index.contains("MakhnoYMMod Downloads"), "index.js: каталог загрузок по умолчанию");
    v.chk(index.contains("shell.openPath"), "index.js: кроссплатформенное открытие папок");
    v.chk(index.contains("execFile"), "index.js: execFile вместо exec");
    v.chk(index.contains("makhnoYMMod.axios") && index.contains("makhno_allowedHosts"), "index.js: axios-мост с белым списком");
    v.chk(index.contains("window.__getPlayerState"), "index.js: Discord RPC хук плеера");
    v.chk(index.contains("1283109459463377011"), "index.js: Discord CLIENT_ID");
    v.chk(index.contains("webSecurity"), "index.js: webSecurity присутствует");
    v.chk(index.contains("// MakhnoYMMod:devtools-shortcut"), "index.js: хоткей devtools");
    v.chk(index.contains("/*MakhnoYMMod:telemetry-begin*/") && index.contains("/*MakhnoYMMod:telemetry-end*/"), "index.js: блокировщик телеметрии");
    v.chk(!index.contains("yandexMusicMod"), "index.js: нет legacy-имён");
    for ch in [
        "makhnoYMMod.getStorageValue",
        "makhnoYMMod.setStorageValue",
        "makhnoYMMod.storageValueUpdated",
        "makhnoYMMod.selectDownloadFolder",
        "makhnoYMMod.openFolder",
        "makhnoYMMod.openDownloadDirectory",
        "makhnoYMMod.downloadTrack",
        "makhnoYMMod.axios",
    ] {
        v.chk(index.contains(ch), &format!("index.js: канал {ch}"));
    }
    v.node_check(&index, "index.js");

    // --- preload.js ---
    v.chk(preload.contains("// MakhnoYMMod preload begin"), "preload.js: мост мода");
    v.chk(preload.contains("exposeInMainWorld(\"makhnoYMMod\""), "preload.js: makhnoYMMod в window");
    v.chk(!preload.contains("yandexMusicMod"), "preload.js: нет legacy");
    for ch in [
        "makhnoYMMod.getStorageValue",
        "makhnoYMMod.setStorageValue",
        "makhnoYMMod.storageValueUpdated",
        "makhnoYMMod.selectDownloadFolder",
        "makhnoYMMod.openFolder",
        "makhnoYMMod.openDownloadDirectory",
        "makhnoYMMod.downloadTrack",
        "makhnoYMMod.axios",
    ] {
        v.chk(preload.contains(ch), &format!("preload.js: канал {ch}"));
    }
    v.node_check(&preload, "preload.js");

    // --- package.json ---
    v.chk(pkg.get("name").and_then(|x| x.as_str()) == Some(crate::payload::MOD_NAME), "package.json: name = MakhnoYMMod");
    v.chk(
        pkg.get("modVersion").and_then(|x| x.as_str()) == Some(crate::payload::MOD_VERSION),
        &format!("package.json: modVersion = {}", crate::payload::MOD_VERSION),
    );
    let deps = pkg.get("dependencies").and_then(|d| d.as_object()).cloned().unwrap_or_default();
    for need in ["axios", "sanitize-filename", "@xhayper/discord-rpc"] {
        v.chk(deps.contains_key(need), &format!("package.json: {need}"));
    }
    for banned in patch::BANNED_DEPS {
        v.chk(!deps.contains_key(banned), &format!("package.json: {banned} {}", if banned == "fluent-ffmpeg" || banned == "ts-md5" { "вырезан" } else { "удалён" }));
    }

    // --- renderer.js ---
    v.chk(!renderer.contains("ingest.de.sentry.io") && !renderer.contains("@o451088"), "renderer.js: телеметрия Sentry удалена");
    v.chk(renderer.contains("dsn: \"\""), "renderer.js: Sentry.init с пустым DSN");
    v.chk(renderer.contains("__makhnoYMModAnalyticsEnabled"), "renderer.js: флаг аналитики");
    v.chk(renderer.contains("window.makhnoYMMod"), "renderer.js: мост makhnoYMMod");
    v.chk(!renderer.contains("yandexMusicMod"), "renderer.js: переименован полностью");
    v.chk(!renderer.contains("__MAKHNO_"), "renderer.js: токены заменены");
    v.chk(
        renderer.contains(&format!("\"{}\"", crate::payload::MOD_VERSION)),
        &format!("renderer.js: версия {}", crate::payload::MOD_VERSION),
    );
    v.chk(!renderer.contains("Stephanzion"), "renderer.js: атрибуты старого автора удалены");
    v.chk(!renderer.contains("YandexMusicBetaMod"), "renderer.js: упоминаний старого мода нет");
    v.chk(!renderer.contains("discord.gg"), "renderer.js: ссылок на Discord-сообщество нет");
    v.chk(!renderer.contains("Discord сервер"), "renderer.js: кнопки Discord-сообщества нет");
    v.chk(renderer.contains("#2E7D32"), "renderer.js: тёмно-зелёный акцент по умолчанию");
    v.chk(
        renderer.contains("\"custom-themes/enabled\")) !== !1"),
        "renderer.js: собственная тема включена по умолчанию",
    );
    v.chk(
        renderer.contains("--ym-controls-color-primary-on_default-enabled: #fff"),
        "renderer.js: контент на зелёных кнопках белый",
    );
    v.node_check(&renderer, "app_makhnoYMMod_renderer.js");

    // --- renderer.css: тёмно-зелёный primary ---
    let rcss = asar
        .get("app/makhnoYMMod/renderer.css")
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();
    v.chk(
        rcss.contains("--primary: oklch(52.3% 0.135 144.2)"),
        "renderer.css: primary тёмно-зелёный (обе темы)",
    );
    v.chk(!rcss.contains("oklch(88% 0.17 93.42)"), "renderer.css: жёлтый primary удалён");

    // --- клиентские CSS/SVG/чанки: жёлтого не осталось ---
    let css_svg: Vec<String> = asar
        .files
        .iter()
        .map(|f| f.rel.clone())
        .filter(|r| patch::is_recolor_target(r))
        .collect();
    let mut yellow_left: Vec<String> = Vec::new();
    let mut on_default_white = false;
    let re_on = regex::Regex::new(r"(?i)--ym-controls-color-primary-on_default-enabled:\s*#fff").unwrap();
    let re_rgb = regex::Regex::new(r"(?i)rgba?\(\s*255\s*,\s*255\s*,\s*0\s*[,)]").unwrap();
    for rel in &css_svg {
        if let Some(b) = asar.get(rel) {
            let t = String::from_utf8_lossy(b).to_lowercase();
            for (from, _) in patch::YELLOW_HEX {
                if t.contains(&from.to_lowercase()) {
                    yellow_left.push(format!("{rel}:{from}"));
                }
            }
            if re_rgb.is_match(&t) {
                yellow_left.push(format!("{rel}:rgba(255,255,0)"));
            }
            if re_on.is_match(&t) {
                on_default_white = true;
            }
        }
    }
    v.chk(yellow_left.is_empty(), "клиентские CSS/SVG/чанки: жёлтый перекрашен");
    if !yellow_left.is_empty() {
        println!("     остатки: {}", yellow_left.iter().take(5).cloned().collect::<Vec<_>>().join(", "));
    }
    v.chk(on_default_white, "клиентские CSS: контент на primary-кнопках белый");
    v.chk(!css_svg.is_empty(), "клиентские CSS-файлы найдены");


    // --- структура ---
    v.chk(asar.exists("app/makhnoYMMod/renderer.css"), "renderer.css на месте");
    v.chk(!asar.has_prefix("app/yandexMusicMod/"), "старый каталог yandexMusicMod удалён");
    v.chk(!asar.has_prefix("app/media/splash_screen/"), "сплэш-видео удалено");
    v.chk(!asar.has_prefix("node_modules/fluent-ffmpeg/"), "node_modules: fluent-ffmpeg удалён");
    v.chk(!asar.has_prefix("node_modules/ts-md5/"), "node_modules: ts-md5 удалён");
    for need in ["node_modules/axios/", "node_modules/sanitize-filename/", "node_modules/@xhayper/discord-rpc/"] {
        v.chk(asar.has_prefix(need), &format!("node_modules: {need}"));
    }

    // --- HTML (страницы приложения; доки node_modules без <head> не считаются) ---
    let html: Vec<&crate::asar::FileEntry> = asar
        .files
        .iter()
        .filter(|f| f.rel.ends_with(".html") && f.rel.starts_with("app/"))
        .collect();
    v.chk(!html.is_empty(), &format!("HTML-страниц с инъекцией: {}", html.len()));
    let all = html.iter().all(|f| {
        asar.get(&f.rel)
            .map(|b| {
                let t = String::from_utf8_lossy(b);
                !t.contains("<head") || t.contains("makhnoYMMod")
            })
            .unwrap_or(false)
    });
    v.chk(all, "HTML: все страницы содержат makhnoYMMod");

    Ok(v)
}

pub fn print_result(v: &Verifier) -> bool {
    println!();
    if v.fail == 0 {
        println!("ПРОВЕРКА ПРОЙДЕНА ({} проверок)", v.ok);
        true
    } else {
        println!("ПРОВЕРКА НЕ ПРОЙДЕНА: {} ошибок из {}", v.fail, v.ok + v.fail);
        false
    }
}
