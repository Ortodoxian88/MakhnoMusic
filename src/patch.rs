//! Патч-пайплайн: index.js, preload.js, package.json, HTML-страницы.

use crate::payload::{self, Payload};
use crate::asar::AsarArchive;
use serde_json::{Map, Value};

pub struct PatchOptions {
    pub keep_web_security: bool,
    pub bundle_ffmpeg: Option<std::path::PathBuf>,
    pub dry_run: bool,
}

#[derive(Default)]
pub struct PatchReport {
    pub steps: Vec<(String, String)>, // (описание, статус/детали)
}

fn ok(report: &mut PatchReport, what: &str, detail: impl Into<String>) {
    report.steps.push((what.to_string(), detail.into()));
}

// ---------- index.js ----------

pub const HEAD_BEGIN: &str = "// MakhnoYMMod head begin\n";
pub const HEAD_END: &str = "\n// MakhnoYMMod head end\n\n";
pub const MAIN_BEGIN: &str = "\n\n// MakhnoYMMod main begin\n(async () => {\n";
pub const MAIN_END: &str = "\n\n})();\n// MakhnoYMMod main end\n";
pub const PRELOAD_BEGIN: &str = "\n\n// MakhnoYMMod preload begin\n";
pub const PRELOAD_END: &str = "})();\n\n// MakhnoYMMod preload end\n";

const DEVTOOLS_HOTKEY: &str = concat!(
    "\n",
    "          // MakhnoYMMod:devtools-shortcut\n",
    "          electron.globalShortcut.register(\"CommandOrControl+Shift+I\", () => {\n",
    "            const focusedWindow = electron.BrowserWindow.getFocusedWindow();\n",
    "            if (focusedWindow) focusedWindow.webContents.toggleDevTools();\n",
    "          });\n",
);

const TELEMETRY_URLS: [&str; 17] = [
    "https://yandex.ru/clck/*",
    "https://mc.yandex.ru/*",
    "https://api.music.yandex.net/dynamic-pages/trigger/*",
    "https://api.music.yandex.net/lyric-views",
    "https://log.strm.yandex.ru/*",
    "https://api.acquisition-gwe.plus.yandex.net/*",
    "https://api.events.plus.yandex.net/*",
    "https://events.plus.yandex.net/*",
    "https://plus.yandex.net/*",
    "https://yandex.ru/ads/*",
    "https://strm.yandex.ru/ping",
    "https://yandex.ru/an/*",
    "https://strm.yandex.ru/perf",
    "https://lumen.yandex.ru/*",
    "https://csp.yandex.net/csp",
    "https://center.yandex-team.ru/*",
    "https://an.yandex.ru/*",
];

fn telemetry_block() -> String {
    let mut s = String::from("  /*MakhnoYMMod:telemetry-begin*/\n");
    s.push_str("  window.webContents.session.webRequest.onBeforeRequest(\n    {\n      urls: [\n");
    for u in TELEMETRY_URLS {
        s.push_str(&format!("        \"{u}\",\n"));
    }
    s.push_str("      ],\n    },\n    (details, callback) => {\n      callback({ cancel: true });\n    },\n  );\n");
    s.push_str("  /*MakhnoYMMod:telemetry-end*/\n");
    s
}

// ---------- перекраска жёлтого Яндекса в тёмно-зелёный ----------

/// Жёлтое семейство клиента (enabled/hovered/pressed/strokes/иконки) -> тёмно-зелёная палитра.
/// Порядок важен: длинные шаблоны раньше коротких (#fc0 — последним).
pub const YELLOW_HEX: &[(&str, &str)] = &[
    ("#fed42a", "#2E7D32"), // --ym-controls-color-primary-default-enabled (5.86)
    ("#fed42b", "#2E7D32"), // вариант написания жёлтого (в т.ч. inline-SVG в чанках)
    ("#ffd466", "#388E3C"), // hovered (5.86)
    ("#fecb01", "#1B5E20"), // pressed (5.86)
    ("#ffc533", "#43A047"), // outline-selected stroke (5.86)
    ("#996e00", "#164A1E"), // focused stroke (тёмный жёлтый, 5.86)
    ("#ffdb4d", "#2E7D32"), // UserID outline, фолбэки
    ("#ffcc00", "#2E7D32"), // иконка графиков в sprite.svg
    ("#fce000", "#2E7D32"), // запасные жёлтые
    ("#ffd500", "#2E7D32"),
    ("#dada0b", "#388E3C"), // hovered/text-hovered (5.126)
    ("#ffff80", "#43A047"), // pressed (5.126, бледно-жёлтый)
    ("#ffbc0d", "#2E7D32"), // лого-variant (светлая тема 5.86) + favicon
    ("#ffbb0f", "#2E7D32"), // default-enabled (светлая тема 5.86)
    ("#f5e400", "#388E3C"), // hovered (светлая тема 5.126)
    ("#fcca00", "#2E7D32"), // inline-SVG иконки в чанках, favicon
    ("#ffee00", "#2E7D32"), // favicon (5.126)
    ("#ffff00", "#2E7D32"), // glow-иллюстрации
    ("#e5bf27", "#2E7D32"), // градиент обложки ThenNowComparison
    ("#ff0", "#2E7D32"), // 3-значный жёлтый: primary (5.126)
    ("#fe0", "#2E7D32"), // 3-значный жёлтый: лого (светлая тема 5.126)
    ("#fc0", "#2E7D32"), // 3-значный жёлтый (bear-focus.svg)
];

/// Файлы, в которых живёт жёлтый: CSS клиента, SVG-иконки (включая корневые
/// favicon/иконки приложения) и JS-чанки с inline-SVG.
pub fn is_recolor_target(rel: &str) -> bool {
    (rel.starts_with("app/_next/static/css/") && rel.ends_with(".css"))
        || (rel.starts_with("app/icons/") && rel.ends_with(".svg"))
        || (rel.starts_with("app/_next/static/media/") && rel.ends_with(".svg"))
        || (rel.starts_with("app/_next/static/chunks/") && rel.ends_with(".js"))
        || (rel.strip_prefix("app/").is_some_and(|r| r.ends_with(".svg") && !r.contains('/')))
}

/// Перекраска одного текста (CSS/SVG). Возвращает (новый текст, число замен).
pub fn recolor_text(txt: &str) -> (String, usize) {
    let mut out = txt.to_string();
    let mut total = 0usize;
    for (from, to) in YELLOW_HEX {
        let re = regex::Regex::new(&format!("(?i){}\\b", from)).unwrap();
        let n = re.find_iter(&out).count();
        if n > 0 {
            out = re.replace_all(&out, to.to_string()).to_string();
            total += n;
        }
    }
    // Чёрный контент на бывших жёлтых кнопках -> белый (на тёмно-зелёном читается).
    let re_on = regex::Regex::new(r"(?i)(--ym-controls-color-primary-on_default-enabled:\s*)#000\b").unwrap();
    if re_on.is_match(&out) {
        let n = re_on.find_iter(&out).count();
        out = re_on.replace_all(&out, "${1}#fff").to_string();
        total += n;
    }
    // Жёлтый в функциональной форме: rgb(255,255,0)/rgba(255,255,0,.5) -> зелёный.
    let re_rgb = regex::Regex::new(r"(?i)(rgba?\(\s*)255(\s*,\s*)255(\s*,\s*)0(\s*[,)])").unwrap();
    if re_rgb.is_match(&out) {
        let n = re_rgb.find_iter(&out).count();
        out = re_rgb.replace_all(&out, "${1}46${2}125${3}50${4}").to_string();
        total += n;
    }
    (out, total)
}

/// Перекрашивает жёлтые ассеты клиента в asar (CSS + SVG). Идемпотентно.
pub fn recolor_assets(asar: &mut AsarArchive, report: &mut PatchReport) {
    let targets: Vec<String> = asar
        .files
        .iter()
        .map(|f| f.rel.clone())
        .filter(|r| is_recolor_target(r))
        .collect();
    let mut files_changed = 0usize;
    let mut total = 0usize;
    for rel in targets {
        let bytes = match asar.get(&rel) {
            Some(b) => b.to_vec(),
            None => continue,
        };
        let txt = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let (new, n) = recolor_text(&txt);
        if n > 0 {
            asar.set_file(&rel, new.into_bytes());
            files_changed += 1;
            total += n;
        }
    }
    ok(report, "перекраска жёлтого -> тёмно-зелёный", format!("{files_changed} файлов, {total} замен"));
}

pub fn patch_index(src: &str, payload: &Payload, opts: &PatchOptions, report: &mut PatchReport) -> Result<String, String> {
    let mut out = src.to_string();

    // --- 1. Пролог (head) ---
    if let Some(pos) = out.find(HEAD_BEGIN) {
        // наш собственный блок — снять и поставить заново (идемпотентность)
        if let Some(end_rel) = out[pos..].find(HEAD_END) {
            let end = pos + end_rel + HEAD_END.len();
            out = format!("{}{}", &out[..pos], &out[end..]);
        }
    } else if out.starts_with("const yandexMusicMod_fs") {
        // пролог старого YandexMusicMod — вырезать до конца его try/catch
        let end = out
            .find("} catch (e) {}")
            .ok_or("не найден конец старого пролога yandexMusicMod")?
            + "} catch (e) {}".len();
        let mut tail = &out[end..];
        // съесть пустые строки после старого пролога
        while tail.starts_with('\n') {
            tail = &tail[1..];
        }
        out = tail.to_string();
        ok(report, "index.js: старый пролог YandexMusicMod удалён", "");
    }
    out = format!("{}{}{}{}", HEAD_BEGIN, payload.head, HEAD_END, out);
    ok(report, "index.js: пролог мода установлен", "");

    // --- 2. Убить конструктор апдейтера ---
    let re_ctor = regex::Regex::new(
        r#"constructor\(\)\s*\{\s*this\.logger = new Logger\("UpdateLogger"\);"#,
    )
    .unwrap();
    if re_ctor.is_match(&out) {
        out = re_ctor.replace(&out, "constructor() { return;").to_string();
        ok(report, "index.js: конструктор апдейтера вырезан", "");
    }

    // --- 3. devTools: true в webPreferences ---
    if out.contains(" devTools: true, \n") || out.contains("devTools: true") {
        ok(report, "index.js: devTools уже включён", "");
    } else if out.contains("devTools: false") {
        // будущие версии клиента могут задать devTools явно — просто переключаем
        out = out.replacen("devTools: false", "devTools: true", 1);
        ok(report, "index.js: devTools включён (замена false)", "");
    } else {
        let anchor = "  const webPreferences = {\n";
        match out.find(anchor) {
            Some(pos) => {
                out.insert_str(pos + anchor.len(), " devTools: true, \n\n");
                ok(report, "index.js: devTools включён", "");
            }
            None => {
                return Err("index.js: не найден якорь webPreferences (devTools)".into());
            }
        }
    }

    // --- 4. webSecurity ---
    if !opts.keep_web_security {
        if out.contains("webSecurity: true") {
            out = out.replacen("webSecurity: true", "webSecurity: false", 1);
            ok(report, "index.js: webSecurity отключён", "лирика musixmatch может не работать");
        }
    } else {
        ok(report, "index.js: webSecurity сохранён (--keep-web-security)", "");
    }

    // --- 5. show: true ---
    let re_show = regex::Regex::new(r"\n[ \t]*show: false,").unwrap();
    if re_show.is_match(&out) {
        out = re_show.replace(&out, "\n show: true,").to_string();
        ok(report, "index.js: окно показывается сразу", "");
    }

    // --- 6. titleBarStyle ---
    if !out.contains("enableSystemToolbar ? 'hidden' : 'default'")
        && !out.contains("enableSystemToolbar ? \"hidden\" : \"default\"")
    {
        if out.contains("titleBarStyle: \"hidden\",") {
            out = out.replacen(
                "titleBarStyle: \"hidden\",",
                "titleBarStyle: !enableSystemToolbar ? 'hidden' : 'default',",
                1,
            );
            ok(report, "index.js: системный тулбар по настройке devtools/systemToolbar", "");
        }
    }

    // --- 7. минимальные размеры окна ---
    if out.contains("minWidth: 768,") {
        out = out.replacen("minWidth: 768,", "minWidth: 360,", 1);
        out = out.replacen("minHeight: 650,", "minHeight: 550,", 1);
        ok(report, "index.js: minWidth/minHeight 360/550", "");
    }

    // --- 8. хоткей devtools ---
    if !out.contains("// MakhnoYMMod:devtools-shortcut") {
        let anchor = "window.once(\"ready-to-show\", () => {";
        match out.find(anchor) {
            Some(pos) => {
                out.insert_str(pos + anchor.len(), DEVTOOLS_HOTKEY);
                ok(report, "index.js: хоткей Ctrl+Shift+I", "");
            }
            None => return Err("index.js: не найден якорь ready-to-show".into()),
        }
    }

    // --- 9. блокировщик телеметрии ---
    {
        // Убираем свой предыдущий блок и легаси-блоки старого мода.
        let re_region = regex::Regex::new(
            r#"(  \}\);\n)(?:(?:\n?  window\.webContents\.session\.webRequest\.onBeforeRequest\([\s\S]*?\n  \);\n)|(?:  /\*MakhnoYMMod:telemetry-begin\*/\n[\s\S]*?  /\*MakhnoYMMod:telemetry-end\*/\n))*(  return window;)"#,
        )
        .unwrap();
        let block = telemetry_block();
        match re_region.captures(&out) {
            Some(caps) => {
                let m = caps.get(0).unwrap();
                let replacement = format!("{}{}{}", &caps[1], block, &caps[2]);
                out = format!("{}{}{}", &out[..m.start()], replacement, &out[m.end()..]);
                ok(report, "index.js: блокировщик телеметрии (17 правил)", "");
            }
            None => return Err("index.js: не найден якорь return window".into()),
        }
    }

    // --- 10. погасить автообновление ---
    let re_upd = regex::Regex::new(r"([ \t]*)updater\.start\(\);").unwrap();
    if re_upd.is_match(&out) {
        out = re_upd
            .replace(&out, "${1}/*MakhnoYMMod: автообновление отключено */")
            .to_string();
        ok(report, "index.js: updater.start() погашен", "");
    }

    // --- 11. main-блок мода ---
    if let Some(pos) = out.find("\n\n// yandexMusicMod main.js\n") {
        out = out[..pos].to_string();
        ok(report, "index.js: старый main-блок YandexMusicMod удалён", "");
    } else if let Some(pos) = out.find("\n\n// MakhnoYMMod main begin\n") {
        out = out[..pos].to_string();
    }
    out = format!("{}{}{}{}", out, MAIN_BEGIN, payload.main, MAIN_END);
    // MAIN_END уже заканчивается \n; итоговый файл заканчивается "\n// MakhnoYMMod main end\n"
    ok(report, "index.js: main-блок мода добавлен", "");

    Ok(out)
}

// ---------- preload.js ----------

pub fn patch_preload(src: &str, payload: &Payload, report: &mut PatchReport) -> String {
    let mut out = src.to_string();
    // Снять старые мосты (наш или YandexMusicMod).
    if let Some(pos) = out.find(PRELOAD_BEGIN) {
        out = out[..pos].to_string();
    } else if let Some(pos) = out.find("\n\n// yandexMusicMod preload.js\n") {
        out = out[..pos].to_string();
    }
    // Убрать возможный хвостовой перевод строки для детерминизма.
    while out.ends_with('\n') {
        out.pop();
    }
    out.push_str(&format!(
        "{}// MakhnoYMMod {} — preload-мост. Добавляется в конец preload.js (обёрнут в IIFE патчером).\n// Самодостаточен: собственные require, ничего не берёт из внешнего скоупа.\n(() => {{\n{}\n{}",
        PRELOAD_BEGIN,
        payload::MOD_VERSION,
        payload.preload,
        PRELOAD_END
    ));
    ok(report, "preload.js: мост makhnoYMMod", "8 IPC-каналов");
    out
}

// ---------- package.json ----------

pub const BANNED_DEPS: [&str; 8] = [
    "fluent-ffmpeg",
    "ts-md5",
    "ffmpeg-static",
    "cmdk",
    "@sentry/react",
    "neverthrow",
    "zod",
    "@radix-ui/react-slider",
];
pub const NEEDED_DEPS: [(&str, &str); 3] = [
    ("axios", "1.13.5"),
    ("sanitize-filename", "1.6.3"),
    ("@xhayper/discord-rpc", "1.3.0"),
];

pub fn patch_package_json(src: &str, opts: &PatchOptions, report: &mut PatchReport) -> Result<String, String> {
    let mut pkg: Map<String, Value> = serde_json::from_str(src).map_err(|e| format!("package.json: {e}"))?;
    pkg.insert("name".into(), Value::String(payload::MOD_NAME.into()));
    pkg.insert("author".into(), Value::String("Makhno".into()));
    let deps = pkg
        .entry("dependencies".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let deps = deps.as_object_mut().ok_or("dependencies не объект")?;
    for b in BANNED_DEPS {
        deps.shift_remove(b);
    }
    if opts.bundle_ffmpeg.is_some() {
        deps.shift_insert(deps.len(), "ffmpeg-static".into(), Value::String("^5.3.0".into()));
    } else {
        deps.shift_remove("ffmpeg-static");
    }
    // Нужные зависимости — в конец в фиксированном порядке.
    // (Map::remove у serde_json с preserve_order — это swap_remove,
    //  он ломает порядок; поэтому сдвиговыми операциями.)
    for (name, ver) in NEEDED_DEPS {
        deps.shift_remove(name);
        deps.shift_insert(deps.len(), name.to_string(), Value::String(ver.into()));
    }
    pkg.insert("modName".into(), Value::String(payload::MOD_NAME.into()));
    pkg.insert("modVersion".into(), Value::String(payload::MOD_VERSION.into()));
    ok(report, "package.json: имя, версия, зависимости", "мёртвые библиотеки вырезаны");
    serde_json::to_string_pretty(&Value::Object(pkg)).map_err(|e| e.to_string())
}

// ---------- HTML ----------

pub const HTML_INJECTION: &str = concat!(
    "<script>window.__makhnoYMModAnalyticsEnabled = false;</script>",
    "<script src=\"/makhnoYMMod/renderer.js\"></script>",
    "<link rel=\"stylesheet\" href=\"/makhnoYMMod/renderer.css\">"
);

pub fn patch_html(src: &str, report: &mut PatchReport) -> Result<String, String> {
    let mut out = src.to_string();
    // Легаси-инъекции старого мода (в разных форматах).
    // Script-тег убираем вместе с ведущим переводом строки и отступом его строки.
    let re_ymod_script = regex::Regex::new(
        r#"\n[ \t]*<script src=\"/yandexMusicMod/renderer\.js\"></script>"#,
    )
    .unwrap();
    if re_ymod_script.is_match(&out) {
        // тег вместе с переводом строки и отступом его строки
        out = re_ymod_script.replace(&out, "").to_string();
    } else {
        out = out.replace("<script src=\"/yandexMusicMod/renderer.js\"></script>", "");
    }
    for pat in [
        "<link rel=\"stylesheet\" href=\"/yandexMusicMod/renderer.css\" />",
        "<link rel=\"stylesheet\" href=\"/yandexMusicMod/renderer.css\">",
    ] {
        out = out.replace(pat, "");
    }
    // Наша прежняя инъекция — снять и поставить заново.
    if out.contains(HTML_INJECTION) {
        out = out.replace(HTML_INJECTION, "");
    }
    // Эталон мода не оставляет хвостовой перевод строки в HTML.
    while out.ends_with('\n') {
        out.pop();
    }
    let head = "<head>";
    match out.find(head) {
        Some(pos) => {
            out.insert_str(pos + head.len(), HTML_INJECTION);
            Ok(out)
        }
        None => {
            report.steps.push(("html: нет <head>".into(), "пропущен".into()));
            Err("нет тега <head>".into())
        }
    }
}

// ---------- весь asar ----------

pub fn patch_archive(
    asar: &mut AsarArchive,
    payload: &Payload,
    opts: &PatchOptions,
    report: &mut PatchReport,
) -> Result<(), String> {
    // index.js
    let index = asar
        .get("index.js")
        .ok_or("в asar нет index.js")?
        .to_vec();
    let index = String::from_utf8(index).map_err(|_| "index.js не utf-8")?;
    let index = patch_index(&index, payload, opts, report)?;
    asar.set_file("index.js", index.into_bytes());

    // preload.js
    let preload = asar.get("preload.js").ok_or("в asar нет preload.js")?.to_vec();
    let preload = String::from_utf8(preload).map_err(|_| "preload.js не utf-8")?;
    let preload = patch_preload(&preload, payload, report);
    asar.set_file("preload.js", preload.into_bytes());

    // package.json
    let pkg = asar.get("package.json").ok_or("в asar нет package.json")?.to_vec();
    let pkg = String::from_utf8(pkg).map_err(|_| "package.json не utf-8")?;
    let pkg = patch_package_json(&pkg, opts, report)?;
    asar.set_file("package.json", pkg.into_bytes());

    // HTML-страницы приложения (доки node_modules не трогаем)
    let html_files: Vec<String> = asar
        .files
        .iter()
        .map(|f| f.rel.clone())
        .filter(|r| r.starts_with("app/") && r.ends_with(".html"))
        .collect();
    let mut patched_html = 0;
    let mut failed_html = Vec::new();
    for rel in html_files {
        let bytes = asar.get(&rel).ok_or_else(|| format!("{rel}: не читается"))?.to_vec();
        let txt = String::from_utf8(bytes).map_err(|_| format!("{rel}: не utf-8"))?;
        match patch_html(&txt, report) {
            Ok(new) => {
                asar.set_file(&rel, new.into_bytes());
                patched_html += 1;
            }
            Err(_) => failed_html.push(rel),
        }
    }
    ok(report, "HTML: инъекция renderer", format!("{patched_html} страниц"));
    if !failed_html.is_empty() {
        ok(report, "HTML: пропущены", failed_html.join(", "));
    }

    // Перекраска жёлтых ассетов клиента (CSS + SVG-иконки).
    recolor_assets(asar, report);

    // Ассеты, node_modules, чистка.
    payload::apply_tree(asar, payload);
    ok(report, "app/makhnoYMMod", format!("{} файлов", payload.assets.len()));
    ok(
        report,
        "node_modules",
        format!("{} файлов payload, мёртвые вырезаны", payload.node_modules.len()),
    );

    // --bundle-ffmpeg
    if let Some(bin) = &opts.bundle_ffmpeg {
        let bytes = std::fs::read(bin).map_err(|e| format!("ffmpeg: {e}"))?;
        let exe_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
        asar.set_file("node_modules/ffmpeg-static/index.js", payload::FFMPEG_STATIC_INDEX_JS.as_bytes().to_vec());
        asar.set_file(
            "node_modules/ffmpeg-static/package.json",
            payload::ffmpeg_static_package_json().into_bytes(),
        );
        asar.set_unpacked(&format!("node_modules/ffmpeg-static/{exe_name}"), bytes);
        ok(report, "ffmpeg вшит", format!("node_modules/ffmpeg-static/{exe_name} (unpacked)"));
    }

    Ok(())
}
