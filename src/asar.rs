//! Чтение/запись Electron-архивов asar.
//!
//! Формат: [u32=4][u32 header_len][u32 header_len-4][u32 json_len][json][\0-выравнивание][данные].
//! Заголовок — дерево {"files": {имя: {"files": {...}} | {"size": N, "offset": "N" | "unpacked": true}}}.
//! Патчер пишет архив без integrity-полей (Electron их не требует) и выравнивает
//! префикс заголовка по 4 байта нулями — так же, как @electron/asar.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub rel: String,
    pub size: u64,
    pub offset: u64,
    pub unpacked: bool,
}

pub struct AsarArchive {
    /// Файлы в порядке заголовка.
    pub files: Vec<FileEntry>,
    /// Упакованные данные (блоб после заголовка).
    pub data: Vec<u8>,
    /// Содержимое unpacked-файлов (ключ — относительный путь).
    pub unpacked: BTreeMap<String, Vec<u8>>,
}

fn read_u32(f: &mut fs::File) -> io::Result<u32> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

impl AsarArchive {
    /// Прочитать asar. `unpacked_dir` — соседний каталог app.asar.unpacked (если есть).
    pub fn open(asar_path: &Path, unpacked_dir: Option<&Path>) -> io::Result<AsarArchive> {
        let mut f = fs::File::open(asar_path)?;
        let magic = read_u32(&mut f)?;
        if magic != 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "не похоже на asar (magic != 4)"));
        }
        let header_len = read_u32(&mut f)? as u64;
        let _payload_len = read_u32(&mut f)? as u64;
        let json_len = read_u32(&mut f)? as u64;
        // Проверки против битых/подделанных заголовков: без них
        // header_len - 8 мог бы переполниться и уронить патчер.
        if header_len < 8 || json_len + 8 > header_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("битый заголовок asar (header_len={header_len}, json_len={json_len})"),
            ));
        }
        let mut header_buf = vec![0u8; header_len as usize - 8];
        f.read_exact(&mut header_buf)?;
        if header_buf.len() < json_len as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "битый заголовок asar"));
        }
        let header: Value = serde_json::from_slice(&header_buf[..json_len as usize])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("JSON заголовка: {e}")))?;
        let data_start = 8 + header_len;

        let mut files = Vec::new();
        walk(&header, "", &mut files);

        let data_len = f.metadata()?.len().saturating_sub(data_start);
        let mut data = Vec::with_capacity(data_len as usize);
        f.seek(io::SeekFrom::Start(data_start))?;
        f.read_to_end(&mut data)?;

        // unpacked-файлы
        let mut unpacked = BTreeMap::new();
        for fe in &files {
            if !fe.unpacked {
                continue;
            }
            if let Some(dir) = unpacked_dir {
                let p = dir.join(&fe.rel);
                if p.is_file() {
                    unpacked.insert(fe.rel.clone(), fs::read(&p)?);
                    continue;
                }
            }
            // Файл недоступен: содержимое пустое. Патчер либо удалит такую
            // запись, либо при сохранении она уйдёт в app.asar.unpacked пустой.
            unpacked.insert(fe.rel.clone(), Vec::new());
        }
        Ok(AsarArchive { files, data, unpacked })
    }

    /// Безопасно вырезать байты упакованного файла (проверяет границы блоба).
    pub fn slice(&self, f: &FileEntry) -> Option<&[u8]> {
        if f.unpacked {
            return None;
        }
        let start = usize::try_from(f.offset).ok()?;
        let end = start.checked_add(usize::try_from(f.size).ok()?)?;
        (end <= self.data.len()).then(|| &self.data[start..end])
    }

    pub fn get(&self, rel: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|f| f.rel == rel && !f.unpacked)
            .and_then(|f| self.slice(f))
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.files.iter().any(|f| f.rel == rel)
    }

    /// Есть ли файлы с этим префиксом (каталог или вложенный путь).
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.files.iter().any(|f| f.rel.starts_with(prefix))
    }

    /// Удалить файлы по префиксу пути (например "node_modules/fluent-ffmpeg/").
    /// Возвращает число удалённых.
    pub fn remove_prefix(&mut self, prefix: &str) -> usize {
        let before = self.files.len();
        self.files.retain(|f| !f.rel.starts_with(prefix));
        self.unpacked.retain(|k, _| !k.starts_with(prefix));
        let n = before - self.files.len();
        if n > 0 {
            self.rebuild();
        }
        n
    }

    /// Заменить (или добавить) упакованный файл.
    pub fn set_file(&mut self, rel: &str, contents: Vec<u8>) {
        self.remove_exact(rel);
        let offset = self.data.len() as u64;
        let size = contents.len() as u64;
        self.data.extend_from_slice(&contents);
        self.files.push(FileEntry { rel: rel.to_string(), size, offset, unpacked: false });
    }

    /// Добавить/заменить unpacked-файл (попадёт в app.asar.unpacked).
    pub fn set_unpacked(&mut self, rel: &str, contents: Vec<u8>) {
        self.remove_exact(rel);
        let size = contents.len() as u64;
        self.unpacked.insert(rel.to_string(), contents);
        self.files.push(FileEntry { rel: rel.to_string(), size, offset: u64::MAX, unpacked: true });
    }

    fn remove_exact(&mut self, rel: &str) {
        self.files.retain(|f| f.rel != rel);
        self.unpacked.remove(rel);
    }

    /// Пересобрать блоб данных в порядке files (упакованные — подряд).
    fn rebuild(&mut self) {
        let mut new_data = Vec::with_capacity(self.data.len());
        let mut new_unpacked = BTreeMap::new();
        for f in self.files.iter_mut() {
            if f.unpacked {
                let bytes = self.unpacked.get(&f.rel).cloned().unwrap_or_default();
                f.size = bytes.len() as u64;
                new_unpacked.insert(f.rel.clone(), bytes);
            } else {
                // границы проверяем прямо здесь: метод self.slice(f) нельзя —
                // f уже держит мутабельный заем self.files
                let start = f.offset as usize;
                let end = start.saturating_add(f.size as usize);
                let bytes: Vec<u8> = if f.offset != u64::MAX && end <= self.data.len() {
                    self.data[start..end].to_vec()
                } else {
                    Vec::new()
                };
                f.offset = new_data.len() as u64;
                f.size = bytes.len() as u64;
                new_data.extend_from_slice(&bytes);
            }
        }
        self.data = new_data;
        self.unpacked = new_unpacked;
    }

    /// Записать asar. unpacked-файлы кладёт в `unpacked_dir` (app.asar.unpacked).
    pub fn save(&self, asar_path: &Path, unpacked_dir: Option<&Path>) -> io::Result<()> {
        // Дерево в порядке files: каталоги создаются неявно.
        let mut dir_maps: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
        dir_maps.insert(String::new(), Map::new());
        for f in &self.files {
            let (parent, name) = split_parent(&f.rel);
            ensure_dir(&mut dir_maps, &parent);
            let entry = if f.unpacked {
                json!({ "size": f.size, "unpacked": true })
            } else {
                json!({ "size": f.size, "offset": f.offset.to_string() })
            };
            dir_maps.get_mut(&parent).unwrap().insert(name, entry);
        }
        // Собираем вложенные деревья снизу вверх (от глубоких к корню).
        let mut dirs: Vec<String> = dir_maps.keys().cloned().collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
        for d in &dirs {
            if d.is_empty() {
                continue;
            }
            let (parent, name) = split_parent(d);
            let files_map = dir_maps.remove(d).unwrap();
            let mut node = Map::new();
            node.insert("files".to_string(), Value::Object(files_map));
            dir_maps.get_mut(&parent).unwrap().insert(name.clone(), Value::Object(node));
        }
        let mut root = Map::new();
        root.insert("files".to_string(), Value::Object(dir_maps.remove("").unwrap()));
        let header = Value::Object(root);

        let json_bytes = serde_json::to_vec(&header)?;
        let json_len = json_bytes.len();
        // (8 + 8 + json_len) должно делиться на 4 → паддинг нулями.
        let pad = ((4 - ((16 + json_len) % 4)) % 4) as usize;
        let header_total = 8 + json_len + pad;

        if let Some(parent) = asar_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(asar_path)?;
        out.write_all(&4u32.to_le_bytes())?;
        out.write_all(&(header_total as u32).to_le_bytes())?;
        out.write_all(&((header_total - 4) as u32).to_le_bytes())?;
        out.write_all(&(json_len as u32).to_le_bytes())?;
        out.write_all(&json_bytes)?;
        out.write_all(&vec![0u8; pad])?;
        out.write_all(&self.data)?;
        out.flush()?;

        // unpacked-файлы
        if !self.unpacked.is_empty() {
            let dir = unpacked_dir
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| default_unpacked_dir(asar_path));
            for (rel, bytes) in &self.unpacked {
                let p = dir.join(rel);
                fs::create_dir_all(p.parent().unwrap())?;
                fs::write(p, bytes)?;
            }
        }
        Ok(())
    }
}

fn split_parent(rel: &str) -> (String, String) {
    match rel.rfind('/') {
        Some(i) => (rel[..i].to_string(), rel[i + 1..].to_string()),
        None => (String::new(), rel.to_string()),
    }
}

fn ensure_dir(dirs: &mut BTreeMap<String, Map<String, Value>>, path: &str) {
    if dirs.contains_key(path) {
        return;
    }
    let mut chain: Vec<String> = Vec::new();
    let mut cur = path.to_string();
    while !cur.is_empty() && !dirs.contains_key(&cur) {
        chain.push(cur.clone());
        let (p, _) = split_parent(&cur);
        cur = p;
    }
    for d in chain.into_iter().rev() {
        dirs.insert(d, Map::new());
    }
}

fn walk(node: &Value, prefix: &str, out: &mut Vec<FileEntry>) {
    let Some(files) = node.get("files").and_then(|v| v.as_object()) else { return };
    for (name, ent) in files {
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if ent.get("files").is_some() {
            walk(ent, &rel, out);
        } else {
            let size = ent.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            let unpacked = ent.get("unpacked").and_then(|v| v.as_bool()).unwrap_or(false);
            let offset = ent.get("offset").and_then(|v| v.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            out.push(FileEntry { rel, size, offset, unpacked });
        }
    }
}

/// Найти каталог app.asar.unpacked рядом с asar-файлом.
pub fn default_unpacked_dir(asar_path: &Path) -> PathBuf {
    let mut s = asar_path.as_os_str().to_os_string();
    s.push(".unpacked");
    PathBuf::from(s)
}
