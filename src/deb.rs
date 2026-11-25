//! Пересборка .deb и распаковка установщиков (deb/NSIS-exe) через системные
//! утилиты ar/tar/xz/7z. На Windows deb-путь не требуется.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("{what}: не запустить ({e})"))?;
    if !out.status.success() {
        return Err(format!(
            "{what}: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

pub fn have_tool(name: &str) -> bool {
    // 7-Zip не знает флага --version (код возврата 7) — для него пробуем
    // «i» (информация о сборке), для остальных утилит — классический --version
    let probe = if name.starts_with("7z") { "i" } else { "--version" };
    Command::new(name)
        .arg(probe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Распаковать deb: control-часть в <work>/ctl, данные в <work>/root.
pub fn unpack_deb(deb: &Path, work: &Path) -> Result<(PathBuf, PathBuf), String> {
    let ctl = work.join("ctl");
    let root = work.join("root");
    fs::create_dir_all(&ctl).map_err(|e| e.to_string())?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let tmp = work.join("ar");
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    run(Command::new("ar").arg("x").arg(deb).current_dir(&tmp), "ar x")?;
    // данные
    let data = fs::read_dir(&tmp).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("data.tar")).unwrap_or(false))
        .ok_or("в deb нет data.tar*")?;
    run(
        Command::new("tar").arg("xf").arg(&data).arg("-C").arg(&root),
        "tar xf data",
    )?;
    // control
    let ctl_tar = fs::read_dir(&tmp).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("control.tar")).unwrap_or(false))
        .ok_or("в deb нет control.tar*")?;
    run(
        Command::new("tar").arg("xf").arg(&ctl_tar).arg("-C").arg(&ctl),
        "tar xf control",
    )?;
    Ok((ctl, root))
}

/// Изменить DEBIAN/control: Package=makhnoymod, Version=+makhno, Conflicts/Provides/Replaces.
/// Возвращает (имя_пакета, версия) для имени файла.
pub fn makhnofy_control(control_path: &Path) -> Result<(String, String, String), String> {
    let txt = fs::read_to_string(control_path).map_err(|e| e.to_string())?;
    let mut out = String::new();
    let mut version = String::new();
    let mut arch = String::new();
    for line in txt.lines() {
        if let Some(rest) = line.strip_prefix("Package:") {
            let _ = rest;
            out.push_str(&format!("Package: makhnoymod\n"));
        } else if let Some(rest) = line.strip_prefix("Version:") {
            // повторный патч своего deb не должен удваивать суффикс +makhno*
            version = rest.trim().to_string();
            if let Some(pos) = version.find("+makhno") {
                version.truncate(pos);
            }
            out.push_str(&format!("Version: {}+makhno{}\n", version, crate::payload::MOD_VERSION));
        } else if let Some(rest) = line.strip_prefix("Architecture:") {
            arch = rest.trim().to_string();
            out.push_str(&format!("{line}\n"));
        } else if line.starts_with("Conflicts:") || line.starts_with("Provides:") || line.starts_with("Replaces:") {
            // старые значения не теряем
            out.push_str(line);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !out.contains("Conflicts:") {
        out.push_str("Conflicts: yandex-music, yandexmusic\n");
    } else {
        // дописать yandexmusic, если конфликтует только с yandex-music
        out = out
            .lines()
            .map(|l| {
                if l.starts_with("Conflicts:") && !l.contains("yandexmusic") {
                    format!("{l}, yandexmusic")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
    }
    if !out.contains("Provides:") {
        out.push_str("Provides: yandex-music, yandexmusic\n");
    }
    if !out.contains("Replaces:") {
        out.push_str("Replaces: yandex-music, yandexmusic\n");
    }
    fs::write(control_path, out).map_err(|e| e.to_string())?;
    if arch.is_empty() {
        arch = "amd64".to_string();
    }
    if version.is_empty() {
        return Err("control без поля Version".into());
    }
    let full_version = format!("{version}+makhno{}", crate::payload::MOD_VERSION);
    Ok(("makhnoymod".to_string(), full_version, arch))
}

/// Собрать deb из ctl/ и root/. Возвращает путь к готовому файлу.
pub fn repack_deb(ctl: &Path, root: &Path, out_path: &Path) -> Result<(), String> {
    for t in ["ar", "tar", "xz"] {
        if !have_tool(t) {
            return Err(format!("нужна утилита {t} (apt install {t} / binutils)"));
        }
    }
    // md5sums регенерируем по факту: старый файл врал бы на пропатченных файлах
    if ctl.join("md5sums").is_file() {
        regenerate_md5sums(root, &ctl.join("md5sums"))?;
    }
    let tmp = out_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let debian_binary = tmp.join("debian-binary");
    fs::write(&debian_binary, "2.0\n").map_err(|e| e.to_string())?;

    let data_tar = tmp.join("data.tar");
    run(
        Command::new("tar")
            .arg("--numeric-owner")
            .arg("--owner=0").arg("--group=0")
            .arg("-cf").arg(&data_tar)
            .arg("-C").arg(root).arg("."),
        "tar cf data",
    )?;
    run(Command::new("xz").arg("-T0").arg("-f").arg(&data_tar), "xz data")?;

    // control: порядок control, md5sums, postinst, postrm (как в оригинале)
    let mut ctl_files: Vec<PathBuf> = fs::read_dir(ctl).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    ctl_files.sort_by_key(|p| {
        let n = p.file_name().unwrap().to_string_lossy().to_string();
        match n.as_str() {
            "control" => 0,
            "md5sums" => 1,
            "postinst" => 2,
            "postrm" => 3,
            _ => 10,
        }
    });
    let mut cmd = Command::new("tar");
    cmd.arg("--numeric-owner").arg("--owner=0").arg("--group=0").arg("-cf");
    let ctl_tar = tmp.join("control.tar");
    cmd.arg(&ctl_tar).arg("-C").arg(ctl);
    for f in &ctl_files {
        cmd.arg(f.file_name().unwrap());
    }
    run(&mut cmd, "tar cf control")?;
    run(Command::new("xz").arg("-T0").arg("-f").arg(&ctl_tar), "xz control")?;

    // ar: debian-binary, control.tar.xz, data.tar.xz
    if out_path.exists() {
        fs::remove_file(out_path).map_err(|e| e.to_string())?;
    }
    run(
        Command::new("ar")
            .arg("rcS")
            .arg(out_path)
            .arg(&debian_binary)
            .arg(tmp.join("control.tar.xz"))
            .arg(tmp.join("data.tar.xz")),
        "ar rcS deb",
    )?;
    let _ = fs::remove_file(&debian_binary);
    let _ = fs::remove_file(tmp.join("control.tar.xz"));
    let _ = fs::remove_file(tmp.join("data.tar.xz"));
    Ok(())
}

/// Найти resources/app.asar внутри каталога клиента.
pub fn find_asar(dir: &Path) -> Option<PathBuf> {
    for cand in ["resources/app.asar", "app.asar"] {
        let p = dir.join(cand);
        if p.is_file() {
            return Some(p);
        }
    }
    // корень deb: root/opt/<имя>/resources/app.asar
    if let Ok(rd) = fs::read_dir(dir.join("opt")).map_err(|e| e.to_string()) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path().join("resources/app.asar");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // ресурсы могут лежать на два уровня ниже (распакованный NSIS)
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let sub = e.path();
            if sub.is_dir() {
                for cand in ["resources/app.asar", "app/resources/app.asar"] {
                    let p = sub.join(cand);
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// Распаковать NSIS-инсталлятор 7-Zip'ом (нужен 7z/7za/7zr в PATH).
pub fn unpack_exe(exe: &Path, work: &Path) -> Result<PathBuf, String> {
    let sevenz = ["7z", "7za", "7zr"].iter().find(|t| have_tool(t));
    let sevenz = sevenz.ok_or("для .exe нужен 7-Zip (7z/7za/7zr в PATH)")?;
    fs::create_dir_all(work).map_err(|e| e.to_string())?;
    run(
        // ВАЖНО: 7-Zip требует слитную форму -o<path>; раздельные аргументы
        // ("-o", path) он отвергает с «Too short switch: -o».
        Command::new(sevenz)
            .arg("x")
            .arg("-y")
            .arg(format!("-o{}", work.display()))
            .arg(exe),
        "7z x exe",
    )?;

    // Внутренние архивы app-64.7z у electron-builder-инсталлеров лежат в
    // $PLUGINSDIR (глубина 2), а не на верхнем уровне — ищем рекурсивно.
    let mut inner_archives: Vec<PathBuf> = Vec::new();
    collect_inner_7z(work, &mut inner_archives, 3);
    inner_archives.sort();
    for (i, p) in inner_archives.iter().enumerate() {
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| format!("inner{i}"));
        let sub = work.join(format!(".makhno-inner-{i}-{stem}"));
        fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
        run(
            Command::new(sevenz)
                .arg("x")
                .arg("-y")
                .arg(format!("-o{}", sub.display()))
                .arg(p),
            "7z x inner",
        )?;
    }

    // ищем app.asar по всему дереву (ограниченная глубина)
    find_asar_deep(work, 5)
        .ok_or_else(|| "в распакованном exe не найден resources/app.asar".to_string())
}

/// Собрать *.7z внутри каталога на глубине до `depth` (сам work — уровень 1).
fn collect_inner_7z(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth == 0 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            collect_inner_7z(&p, out, depth - 1);
        } else if p.extension().map(|e| e == "7z").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// Рекурсивный поиск resources/app.asar с ограничением глубины.
fn find_asar_deep(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    if let Some(p) = find_asar(dir) {
        return Some(p);
    }
    let Ok(rd) = fs::read_dir(dir) else { return None };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            if let Some(found) = find_asar_deep(&p, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Пересобрать DEBIAN/md5sums по фактическому содержимому root
/// (формат как в оригинальных deb: "<md5>  <путь>" с двумя пробелами).
fn regenerate_md5sums(root: &Path, md5sums_path: &Path) -> Result<(), String> {
    let mut rels: Vec<String> = Vec::new();
    collect_rel_paths(root, root, &mut rels)?;
    rels.sort();
    let mut out = String::new();
    for rel in rels {
        let bytes = fs::read(root.join(&rel)).map_err(|e| format!("md5 {rel}: {e}"))?;
        out.push_str(&format!("{}  {}\n", md5_hex(&bytes), rel));
    }
    fs::write(md5sums_path, out).map_err(|e| e.to_string())
}

fn collect_rel_paths(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for e in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            collect_rel_paths(root, &p, out)?;
        } else {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

/// MD5 на чистом Rust (RFC 1321) — чтобы не тащить зависимость ради одного вызова.
fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20,
        5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;
    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f2 = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f2.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let digest = [a0.to_le_bytes(), b0.to_le_bytes(), c0.to_le_bytes(), d0.to_le_bytes()].concat();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
