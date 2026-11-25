//! Загрузка payload-каталога мода (сниппеты, renderer, node_modules)
//! и подстановка токенов.

use crate::asar::AsarArchive;
use std::fs;
use std::path::{Path, PathBuf};

pub const MOD_NAME: &str = "MakhnoYMMod";
pub const MOD_VERSION: &str = "2.2.3";
pub const DEFAULT_REPO_URL: &str = "https://github.com/Ortodoxian88/MakhnoYMMod";
pub const DEFAULT_META_URL: &str =
    "https://raw.githubusercontent.com/makhno/MakhnoYMMod/refs/heads/main/.meta/meta.json";

pub struct Payload {
    pub head: String,
    pub main: String,
    pub preload: String,
    /// (относительный путь, содержимое) для app/makhnoYMMod/*
    /// (renderer.js уже с подставленными значениями).
    pub assets: Vec<(String, Vec<u8>)>,
    /// (относительный путь node_modules/..., содержимое).
    pub node_modules: Vec<(String, Vec<u8>)>,
}

/// Вычислить URL meta.json для проверки обновлений из URL репозитория.
pub fn derive_meta_url(repo_url: &str) -> String {
    if repo_url == DEFAULT_REPO_URL {
        return DEFAULT_META_URL.to_string();
    }
    for prefix in ["https://github.com/", "http://github.com/"] {
        if let Some(rest) = repo_url.strip_prefix(prefix) {
            let rest = rest.trim_end_matches('/').trim_end_matches(".git");
            return format!("https://raw.githubusercontent.com/{rest}/refs/heads/main/.meta/meta.json");
        }
    }
    // Не GitHub: просим raw-файл по стандартным путям форджа.
    // Если сервер не отдаёт raw по такому пути — проверка обновлений просто
    // не сработает (молча), без ошибок в UI.
    let base = repo_url.trim_end_matches('/').trim_end_matches(".git");
    if base.contains("gitlab.com") {
        format!("{base}/-/raw/main/.meta/meta.json")
    } else {
        format!("{base}/raw/main/.meta/meta.json")
    }
}

pub fn load_payload_dir(dir: &Path, repo_url: &str, meta_url: &str) -> Result<Payload, String> {
    let read = |p: PathBuf| fs::read(&p).map_err(|e| format!("{}: {e}", p.display()));
    let head = String::from_utf8(read(dir.join("head.js"))?)
        .map_err(|_| "head.js не utf-8".to_string())?;
    let mut main = String::from_utf8(read(dir.join("main.js"))?)
        .map_err(|_| "main.js не utf-8".to_string())?;
    let mut preload = String::from_utf8(read(dir.join("preload.js"))?)
        .map_err(|_| "preload.js не utf-8".to_string())?;
    // Нормализация хвостовых переводов строк — обёртки добавляет патчер.
    while main.ends_with('\n') {
        main.pop();
    }
    while preload.ends_with('\n') {
        preload.pop();
    }

    let assets_dir = dir.join("app/makhnoYMMod");
    let mut assets = Vec::new();
    if assets_dir.is_dir() {
        collect_files(&assets_dir, &assets_dir, &mut assets)?;
    } else {
        return Err(format!("payload без app/makhnoYMMod: {}", dir.display()));
    }

    // Токены подставляем в renderer.js.
    let mut renderer_js = None;
    let mut norm_assets = Vec::new();
    for (rel, bytes) in assets {
        if rel == "renderer.js" {
            let mut txt = String::from_utf8(bytes.clone()).map_err(|_| "renderer.js не utf-8".to_string())?;
            txt = substitute_tokens(&txt, repo_url, meta_url)?;
            renderer_js = Some(txt.into_bytes());
        } else {
            norm_assets.push((rel, bytes));
        }
    }
    let renderer_js = renderer_js.ok_or("payload без renderer.js")?;
    norm_assets.push(("renderer.js".to_string(), renderer_js));
    let nm_dir = dir.join("node_modules");
    let mut node_modules = Vec::new();
    if nm_dir.is_dir() {
        collect_files(&nm_dir, &nm_dir, &mut node_modules)?;
        // collect_files даёт пути относительно node_modules — возвращаем префикс
        node_modules = node_modules
            .into_iter()
            .map(|(rel, bytes)| (format!("node_modules/{rel}"), bytes))
            .collect();
    }

    Ok(Payload { head, main, preload, assets: norm_assets, node_modules })
}

pub fn substitute_tokens(txt: &str, repo_url: &str, meta_url: &str) -> Result<String, String> {
    let out = txt
        .replace("__MAKHNO_REPO_URL__", repo_url)
        .replace("__MAKHNO_META_URL__", meta_url)
        .replace("__MAKHNO_MOD_VERSION__", MOD_VERSION);
    if out.contains("__MAKHNO_") {
        return Err("в renderer.js остался неизвестный токен __MAKHNO_*".to_string());
    }
    Ok(out)
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_files(root, &p, out)?;
        } else {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?));
        }
    }
    Ok(())
}

/// Применить payload к дереву asar: удалить старое, добавить новое.
pub fn apply_tree(asar: &mut AsarArchive, payload: &Payload) {
    // Старые каталоги мода и сплэш.
    asar.remove_prefix("app/yandexMusicMod/");
    asar.remove_prefix("app/makhnoYMMod/");
    asar.remove_prefix("app/media/splash_screen/");
    // Мёртвые зависимости.
    asar.remove_prefix("node_modules/fluent-ffmpeg/");
    asar.remove_prefix("node_modules/ts-md5/");
    asar.remove_prefix("node_modules/ffmpeg-static/");
    // Ассеты мода.
    for (rel, bytes) in &payload.assets {
        asar.set_file(&format!("app/makhnoYMMod/{rel}"), bytes.clone());
    }
    // node_modules payload: каждый модуль заменяется ЦЕЛИКОМ
    // (клиентская копия модуля удаляется вместе с его лишними файлами).
    let mut mods: Vec<String> = payload
        .node_modules
        .iter()
        .map(|(rel, _)| node_module_of(rel))
        .collect();
    mods.sort();
    mods.dedup();
    for m in &mods {
        asar.remove_prefix(&format!("node_modules/{m}/"));
    }
    for (rel, bytes) in &payload.node_modules {
        asar.set_file(rel, bytes.clone());
    }
}

/// Shim для --bundle-ffmpeg: node_modules/ffmpeg-static без бандла бинарника,
/// бинарник кладётся рядом как unpacked-файл.
pub const FFMPEG_STATIC_INDEX_JS: &str = r#"'use strict';
const path = require('path');
module.exports = path.join(__dirname, process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg');
"#;

pub fn ffmpeg_static_package_json() -> String {
    r#"{"name":"ffmpeg-static","version":"5.3.0","description":"ffmpeg static binaries (MakhnoYMMod bundle)","main":"index.js","private":true}"#
        .to_string()
}

/// node_modules/@scope/name/... -> @scope/name; node_modules/name/... -> name
fn node_module_of(rel: &str) -> String {
    let rest = rel.strip_prefix("node_modules/").unwrap_or(rel);
    let mut it = rest.splitn(3, '/');
    let first = it.next().unwrap_or("");
    if first.starts_with('@') {
        match it.next() {
            Some(second) => format!("{first}/{second}"),
            None => first.to_string(),
        }
    } else {
        first.to_string()
    }
}
