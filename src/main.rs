//! MakhnoYMMod 2.2.3 — патчер клиента Яндекс Музыки.
//! Rust-реализация: распаковка/упаковка asar, патчи main/preload/renderer,
//! пересборка deb, встроенная верификация.

mod asar;
mod cli;
mod deb;
mod patch;
mod payload;
mod verify;

use asar::AsarArchive;
use patch::PatchOptions;
use std::path::{Path, PathBuf};
use std::process::exit;

fn main() {
    let args = match cli::parse(std::env::args().skip(1).collect()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ОШИБКА: {e}\n");
            eprint!("{}", cli::USAGE);
            exit(1);
        }
    };
    if let Err(e) = run(&args) {
        eprintln!();
        eprintln!("ОШИБКА: {e}");
        exit(1);
    }
}

fn run(args: &cli::Args) -> Result<(), String> {
    let get = |k: &str, cmd: &str| -> Result<String, String> {
        args.flags
            .get(k)
            .cloned()
            .ok_or_else(|| format!("{cmd}: требуется --{k} <значение>"))
    };
    match args.command.as_str() {
        "patch" => {
            let app_dir = PathBuf::from(get("app-dir", "")?);
            let payload_dir = args.flags.get("payload-dir").map(PathBuf::from);
            let repo_url = args
                .flags
                .get("repo-url")
                .cloned()
                .unwrap_or_else(|| payload::DEFAULT_REPO_URL.to_string());
            let meta_url = args.flags.get("meta-url").cloned();
            let keep_web_security = args.booleans.contains("keep-web-security");
            let bundle_ffmpeg = args.flags.get("bundle-ffmpeg").map(PathBuf::from);
            let no_deb = args.booleans.contains("no-deb");
            let dry_run = args.booleans.contains("dry-run");
            cmd_patch(
                &app_dir,
                payload_dir.as_deref(),
                &repo_url,
                meta_url.as_deref(),
                keep_web_security,
                bundle_ffmpeg.as_deref(),
                no_deb,
                dry_run,
            )
        }
        "verify" => {
            let target = PathBuf::from(get("target", "")?);
            cmd_verify(&target)
        }
        "unpack" => {
            let asar = PathBuf::from(get("asar", "")?);
            let out = PathBuf::from(get("out", "")?);
            cmd_unpack(&asar, &out)
        }
        "pack" => {
            let dir = PathBuf::from(get("dir", "")?);
            let out = PathBuf::from(get("out", "")?);
            cmd_pack(&dir, &out)
        }
        "help" => Ok(()),
        other => Err(format!("неизвестная команда: {other}")),
    }
}

fn find_payload_dir(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        if p.join("head.js").is_file() {
            return Ok(p.to_path_buf());
        }
        return Err(format!("payload не найден: {}", p.display()));
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let mut cands: Vec<PathBuf> = vec![PathBuf::from("payload")];
    if let Some(d) = &exe_dir {
        cands.push(d.join("payload"));
        if let Some(parent) = d.parent() {
            cands.push(parent.join("payload"));
        }
    }
    for c in &cands {
        if c.join("head.js").is_file() {
            return Ok(c.clone());
        }
    }
    Err(format!(
        "payload не найден; искали: {} (укажите --payload-dir)",
        cands.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    ))
}

fn cmd_patch(
    app_dir: &Path,
    payload_dir: Option<&Path>,
    repo_url: &str,
    meta_url: Option<&str>,
    keep_web_security: bool,
    bundle_ffmpeg: Option<&Path>,
    no_deb: bool,
    dry_run: bool,
) -> Result<(), String> {
    let payload_path = find_payload_dir(payload_dir)?;
    let meta_url = meta_url
        .map(|s| s.to_string())
        .unwrap_or_else(|| payload::derive_meta_url(repo_url));
    let payload = payload::load_payload_dir(&payload_path, repo_url, &meta_url)?;
    println!("payload: {}", payload_path.display());

    let opts = PatchOptions {
        keep_web_security,
        bundle_ffmpeg: bundle_ffmpeg.map(|p| p.to_path_buf()),
        dry_run,
    };

    // абсолютные пути: утилиты ar/tar запускаются с другим cwd
    let app_dir_abs = app_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", app_dir.display()))?;
    let app_dir = app_dir_abs.as_path();

    let kind = detect_target(app_dir);
    match kind {
        Target::DebFile => {
            let work = app_dir.with_extension("makhno-work");
            let (ctl, root) = deb::unpack_deb(app_dir, &work)?;
            let asar_path = deb::find_asar(&root).ok_or("в deb нет resources/app.asar")?;
            patch_asar_file(&asar_path, &payload, &opts)?;
            // Бэкап app.asar.orig не должен попадать в пересобранный deb:
            // нетронутый оригинал и так лежит в исходном .deb-файле.
            strip_orig_backup(&asar_path, &root, false);
            if no_deb {
                println!("--no-deb: asar пропатчен, deb не пересобирается: {}", root.display());
            } else {
                let (pkg, version, arch) = deb::makhnofy_control(&ctl.join("control"))?;
                let out_name = format!("{pkg}_{version}_{arch}.deb");
                let out_path = app_dir.parent().unwrap_or(Path::new(".")).join(&out_name);
                deb::repack_deb(&ctl, &root, &out_path)?;
                println!("deb пересобран: {}", out_path.display());
            }
            let _ = std::fs::remove_dir_all(&work);
        }
        Target::ExeFile => {
            let work = app_dir.with_extension("makhno-extract");
            let asar_path = deb::unpack_exe(app_dir, &work)?;
            patch_asar_file(&asar_path, &payload, &opts)?;
            let client_ver = client_version(&asar_path);
            let out = app_dir.parent().unwrap_or(Path::new(".")).join(format!(
                "MakhnoYMMod-{}-{}-app.asar",
                payload::MOD_VERSION,
                client_ver.unwrap_or_else(|| "unknown".into())
            ));
            std::fs::copy(&asar_path, &out).map_err(|e| e.to_string())?;
            // Вычищаем временный каталог распаковки (~400 МБ) — результат уже в out.
            let _ = std::fs::remove_dir_all(&work);
            println!("готово: {}", out.display());
            println!("Замените app.asar в каталоге установки и удалите app.asar.unpacked.");
        }
        Target::DebRoot => {
            let asar_path = deb::find_asar(app_dir).ok_or("в deb-корне нет resources/app.asar")?;
            patch_asar_file(&asar_path, &payload, &opts)?;
            let ctl = app_dir.join("DEBIAN");
            if no_deb || !ctl.join("control").is_file() {
                println!("asar пропатчен на месте: {}", asar_path.display());
            } else {
                // Бэкап не кладём в deb: сохраняем его рядом с будущим deb-файлом.
                strip_orig_backup(&asar_path, app_dir, true);
                let (pkg, version, arch) = deb::makhnofy_control(&ctl.join("control"))?;
                let out_name = format!("{pkg}_{version}_{arch}.deb");
                let out_path = app_dir.parent().unwrap_or(Path::new(".")).join(&out_name);
                deb::repack_deb(&ctl, app_dir, &out_path)?;
                println!("deb пересобран: {}", out_path.display());
            }
        }
        Target::Dir => {
            let asar_path = deb::find_asar(app_dir).ok_or_else(|| {
                format!("в {} не найден resources/app.asar", app_dir.display())
            })?;
            patch_asar_file(&asar_path, &payload, &opts)?;
            println!("asar пропатчен на месте: {}", asar_path.display());
        }
        Target::AsarFile => {
            patch_asar_file(app_dir, &payload, &opts)?;
            println!("asar пропатчен на месте: {}", app_dir.display());
        }
    }
    Ok(())
}

/// Убрать app.asar.orig из данных deb. При `keep` бэкап переносится в
/// родительский каталог (рядом с будущим deb), иначе просто удаляется
/// (для .deb-файла оригинал и так сохранён в самом исходнике).
fn strip_orig_backup(asar_path: &Path, root: &Path, keep: bool) {
    let backup = asar_path.with_extension("asar.orig");
    if !backup.is_file() {
        return;
    }
    if !keep {
        let _ = std::fs::remove_file(&backup);
        println!("бэкап app.asar.orig удалён из данных deb (оригинал остаётся в исходном .deb)");
        return;
    }
    let dest = root
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("app.asar.orig");
    match std::fs::rename(&backup, &dest) {
        Ok(()) => println!("бэкап вынесен из данных deb: {}", dest.display()),
        Err(_) => {
            let _ = std::fs::copy(&backup, &dest);
            let _ = std::fs::remove_file(&backup);
            println!("бэкап вынесен из данных deb: {}", dest.display());
        }
    }
}

fn client_version(asar_path: &Path) -> Option<String> {
    let asar = AsarArchive::open(asar_path, None).ok()?;
    let pkg: serde_json::Value = serde_json::from_slice(asar.get("package.json")?).ok()?;
    pkg.get("version").and_then(|v| v.as_str()).map(|s| s.to_string())
}

enum Target {
    DebFile,
    ExeFile,
    AsarFile,
    DebRoot,
    Dir,
}

fn detect_target(p: &Path) -> Target {
    if p.is_file() {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.ends_with(".deb") {
            Target::DebFile
        } else if name.ends_with(".exe") {
            Target::ExeFile
        } else {
            Target::AsarFile
        }
    } else if p.join("DEBIAN/control").is_file() {
        Target::DebRoot
    } else {
        Target::Dir
    }
}

fn patch_asar_file(asar_path: &Path, payload: &payload::Payload, opts: &PatchOptions) -> Result<(), String> {
    let unpacked_dir = asar::default_unpacked_dir(asar_path);
    let unpacked_opt = if unpacked_dir.is_dir() { Some(unpacked_dir.as_path()) } else { None };
    let mut asar = AsarArchive::open(asar_path, unpacked_opt)
        .map_err(|e| format!("{}: {e}", asar_path.display()))?;

    let mut report = patch::PatchReport::default();
    patch::patch_archive(&mut asar, payload, opts, &mut report)?;

    println!();
    for (what, detail) in &report.steps {
        if detail.is_empty() {
            println!("  [патч] {what}");
        } else {
            println!("  [патч] {what} — {detail}");
        }
    }

    if opts.dry_run {
        println!();
        println!("--dry-run: изменения НЕ записаны");
        return Ok(());
    }

    // бэкап оригинала
    let backup = asar_path.with_extension("asar.orig");
    if !backup.exists() {
        std::fs::copy(asar_path, &backup).map_err(|e| format!("бэкап: {e}"))?;
        println!("бэкап: {}", backup.display());
    }

    asar.save(asar_path, Some(&unpacked_dir))
        .map_err(|e| format!("запись {}: {e}", asar_path.display()))?;

    // вычистить unpacked от удалённых модулей
    for m in ["fluent-ffmpeg", "ts-md5", "ffmpeg-static"] {
        let p = unpacked_dir.join("node_modules").join(m);
        if p.exists() {
            let _ = std::fs::remove_dir_all(&p);
            println!("удалён unpacked-модуль: {}", p.display());
        }
    }
    if unpacked_dir.exists() && unpacked_dir.read_dir().map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(&unpacked_dir);
        println!("удалён пустой {}", unpacked_dir.display());
    }
    println!("app.asar записан: {}", asar_path.display());
    Ok(())
}

fn cmd_verify(target: &Path) -> Result<(), String> {
    // абсолютный путь: ar запускается из другого каталога
    let target_abs = target
        .canonicalize()
        .map_err(|e| format!("{}: {e}", target.display()))?;
    let target = target_abs.as_path();
    // .deb-файл: распаковать во временный каталог и проверить asar внутри
    if target.is_file() && target.extension().map(|e| e == "deb").unwrap_or(false) {
        let work = std::env::temp_dir().join(format!("makhno-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let (_ctl, root) = deb::unpack_deb(target, &work)?;
        let asar = deb::find_asar(&root).ok_or("в deb нет resources/app.asar")?;
        let res = verify_asar_path(&asar);
        let _ = std::fs::remove_dir_all(&work);
        return res;
    }
    let asar_path = if target.is_file() {
        target.to_path_buf()
    } else {
        deb::find_asar(target).ok_or_else(|| format!("в {} не найден app.asar", target.display()))?
    };
    verify_asar_path(&asar_path)
}

fn verify_asar_path(asar_path: &Path) -> Result<(), String> {
    let unpacked = asar::default_unpacked_dir(&asar_path);
    let asar = AsarArchive::open(&asar_path, if unpacked.is_dir() { Some(unpacked.as_path()) } else { None })
        .map_err(|e| e.to_string())?;
    let v = verify::verify_asar(&asar_path, &asar)?;
    if !verify::print_result(&v) {
        exit(1);
    }
    Ok(())
}

fn cmd_unpack(asar_path: &Path, out: &Path) -> Result<(), String> {
    let unpacked = asar::default_unpacked_dir(asar_path);
    let arch = AsarArchive::open(asar_path, if unpacked.is_dir() { Some(unpacked.as_path()) } else { None })
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    for f in &arch.files {
        let p = out.join(&f.rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        let bytes = if f.unpacked {
            arch.unpacked.get(&f.rel).cloned().unwrap_or_default()
        } else {
            arch.slice(f)
                .ok_or_else(|| format!("битая запись asar: {}", f.rel))?
                .to_vec()
        };
        if f.unpacked && bytes.is_empty() && !unpacked.join(&f.rel).is_file() {
            return Err(format!("unpacked-файл отсутствует: {}", unpacked.join(&f.rel).display()));
        }
        std::fs::write(p, bytes).map_err(|e| e.to_string())?;
    }
    println!("распаковано в {}", out.display());
    Ok(())
}

fn cmd_pack(dir: &Path, out: &Path) -> Result<(), String> {
    let mut arch = AsarArchive { files: Vec::new(), data: Vec::new(), unpacked: Default::default() };
    let mut paths: Vec<PathBuf> = Vec::new();
    fn collect(dir: &Path, base: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                collect(&p, base, paths)?;
            } else {
                paths.push(p);
            }
        }
        Ok(())
    }
    collect(dir, dir, &mut paths)?;
    for p in paths {
        let rel = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
        arch.set_file(&rel, bytes);
    }
    arch.save(out, None).map_err(|e| e.to_string())?;
    println!("упакован {}", out.display());
    Ok(())
}

