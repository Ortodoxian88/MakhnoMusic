// MakhnoYMMod 2.2.3 — main-процесс. Добавляется в конец index.js (обёрнут в async IIFE патчером).
// Кроссплатформенно (Windows/Linux), ffmpeg через execFile с массивом аргументов,
// открытие папок через electron.shell, сетевой мост с белым списком доменов.
const makhno_electron = require("electron");
const makhno_fs = require("fs");
const makhno_fsp = require("fs/promises");
const makhno_path = require("path");
const makhno_os = require("os");
const makhno_sanitize = require("sanitize-filename");
const makhno_axios = require("axios");
const { execFile } = require("child_process");
// webcrypto из node:crypto со фолбэком на глобальный (старые Electron без глобального crypto)
const makhno_webcrypto = require("crypto").webcrypto || globalThis.crypto;

const MAKHNO_MOD_NAME = "MakhnoYMMod";
const MAKHNO_MOD_VERSION = "2.2.3";

// ---------- настройки ----------
const makhno_appFolder = makhno_electron.app.getPath("userData");
const makhno_settingsFilePath = makhno_path.join(makhno_appFolder, "mod_settings.json");
const makhno_defaultDownloadPath = makhno_path.join(makhno_os.homedir(), "MakhnoYMMod Downloads");

function makhno_readSettings() {
  try {
    // BOM и пустой файл не должны ронять чтение настроек
    const raw = makhno_fs.readFileSync(makhno_settingsFilePath, "utf8").replace(/^\uFEFF/, "");
    return raw.trim() ? JSON.parse(raw) : {};
  } catch (e) {
    return {};
  }
}

function makhno_writeSettings(settings) {
  try {
    // первый запуск: каталог userData может ещё не существовать
    makhno_fs.mkdirSync(makhno_appFolder, { recursive: true });
    makhno_fs.writeFileSync(makhno_settingsFilePath, JSON.stringify(settings, null, 2));
  } catch (e) {
    console.error(`[${MAKHNO_MOD_NAME}] settings write failed:`, e);
  }
}

{
  const settings = makhno_readSettings();
  if (typeof settings.downloadFolderPath !== "string" || !settings.downloadFolderPath) {
    settings.downloadFolderPath = makhno_defaultDownloadPath;
    makhno_writeSettings(settings);
  }
  console.log(`[${MAKHNO_MOD_NAME}] ${MAKHNO_MOD_VERSION} main-process hooks installed`);
}

// ---------- разрешение ffmpeg: настройка пользователя -> системный PATH -> ffmpeg-static ----------
async function makhno_resolveFfmpeg() {
  const settings = makhno_readSettings();
  if (settings.ffmpegPath && makhno_fs.existsSync(settings.ffmpegPath)) {
    return settings.ffmpegPath;
  }
  const systemFfmpeg = await new Promise((resolve) => {
    const finder = process.platform === "win32" ? "where" : "which";
    execFile(
      finder,
      ["ffmpeg"],
      { windowsHide: true },
      (err, stdout) => {
        resolve(!err && stdout && stdout.trim() ? stdout.trim().split("\n")[0].trim() : null);
      },
    );
  });
  if (systemFfmpeg) {
    console.log(`[${MAKHNO_MOD_NAME}] using system ffmpeg:`, systemFfmpeg);
    return systemFfmpeg;
  }
  try {
    const p = String(require("ffmpeg-static")).replaceAll("app.asar", "app.asar.unpacked");
    if (makhno_fs.existsSync(p)) {
      console.log(`[${MAKHNO_MOD_NAME}] using bundled ffmpeg-static:`, p);
      return p;
    }
  } catch (e) {}
  return null;
}

function makhno_execFileAsync(cmd, args) {
  return new Promise((resolve, reject) => {
    // windowsHide: без него where.exe/ffmpeg на Windows мигают консольными окнами
    execFile(cmd, args, { maxBuffer: 10 * 1024 * 1024, windowsHide: true }, (error, stdout, stderr) => {
      if (error) {
        error.stderr = stderr;
        reject(error);
      } else {
        resolve(stdout);
      }
    });
  });
}

// ---------- IPC: хранилище настроек ----------
makhno_electron.ipcMain.handle("makhnoYMMod.getStorageValue", (_ev, key) => {
  const settings = makhno_readSettings();
  return settings[key] !== undefined ? settings[key] : null;
});

makhno_electron.ipcMain.on("makhnoYMMod.setStorageValue", (_ev, key, value) => {
  const settings = makhno_readSettings();
  settings[key] = value;
  makhno_writeSettings(settings);
  makhno_electron.BrowserWindow.getAllWindows().forEach((window) =>
    window.webContents.send("makhnoYMMod.storageValueUpdated", key, value),
  );
});

// ---------- IPC: папки (кроссплатформенно, без shell-инъекций) ----------
makhno_electron.ipcMain.handle("makhnoYMMod.selectDownloadFolder", async () => {
  const result = await makhno_electron.dialog.showOpenDialog({
    properties: ["openDirectory"],
    title: "Выберите папку для загрузки треков",
  });
  if (result.canceled || !result.filePaths.length) {
    return { success: false, path: null };
  }
  return { success: true, path: result.filePaths[0] };
});

makhno_electron.ipcMain.handle("makhnoYMMod.openFolder", async (_ev, folderPath) => {
  try {
    const errorMessage = await makhno_electron.shell.openPath(folderPath);
    return errorMessage ? { success: false, error: errorMessage } : { success: true };
  } catch (error) {
    console.error(`[${MAKHNO_MOD_NAME}] failed to open folder:`, error);
    return { success: false, error: String(error) };
  }
});

makhno_electron.ipcMain.on("makhnoYMMod.openDownloadDirectory", () => {
  const settings = makhno_readSettings();
  const saveFolder = settings.downloadFolderPath || makhno_defaultDownloadPath;
  makhno_electron.shell.openPath(saveFolder);
});

// ---------- IPC: сетевой мост с белым списком доменов ----------
const makhno_allowedHosts = new Set(["api.music.yandex.net", "apic.musixmatch.com"]);

makhno_electron.ipcMain.handle("makhnoYMMod.axios", async (_ev, config) => {
  try {
    if (!config || typeof config.url !== "string" || !config.url) {
      return { success: false, error: "config.url обязателен", status: 0 };
    }
    const target = new URL(config.url);
    if (target.protocol !== "https:" && target.protocol !== "http:") {
      return { success: false, error: "протокол не поддерживается: " + target.protocol, status: 0 };
    }
    if (!makhno_allowedHosts.has(target.hostname)) {
      return { success: false, error: "host not allowed: " + target.hostname, status: 0 };
    }
    const client = makhno_axios.create({
      validateStatus: () => true,
      // редирект разрешаем только внутри доверенных зон (инфраструктура Яндекса/musixmatch),
      // чтобы белый список нельзя было обойти через перенаправление
      beforeRedirect: (options) => {
        const host = options.hostname || "";
        const allowed =
          makhno_allowedHosts.has(host) || host.endsWith(".yandex.net") || host.endsWith(".musixmatch.com");
        if (!allowed) {
          throw new Error("redirect to host not allowed: " + host);
        }
      },
    });
    const response = await client(config);
    return {
      success: true,
      data: response.data,
      status: response.status,
      statusText: response.statusText,
      headers: response.headers,
    };
  } catch (error) {
    return { success: false, error: String(error), status: 0 };
  }
});

// ---------- расшифровка трека (AES-CTR, ключ приходит от рендерера) ----------
async function makhno_decryptYandexAudio(encryptedData, secretKey) {
  const hexToUint8Array = (hexString) =>
    new Uint8Array((hexString.match(/.{1,2}/g) || []).map((byte) => parseInt(byte, 16)));
  const cryptoKey = await makhno_webcrypto.subtle.importKey(
    "raw",
    hexToUint8Array(secretKey),
    { name: "AES-CTR" },
    false,
    ["decrypt"],
  );
  const counter = new Uint8Array(16);
  return makhno_webcrypto.subtle.decrypt({ name: "AES-CTR", counter, length: 128 }, cryptoKey, encryptedData);
}

// ---------- IPC: загрузка трека ----------
makhno_electron.ipcMain.handle(
  "makhnoYMMod.downloadTrack",
  async (_ev, downloadInfo, trackMeta, customDownloadPath = null) => {
    console.log(`[${MAKHNO_MOD_NAME}] download request:`, downloadInfo.url);

    // Расшифрованные байты пишем во временный файл, а ffmpeg одним вызовом
    // делает remux (-c copy, без перекодирования!) с тегами и обложкой в
    // финальный путь. Если ffmpeg упал — в папке не остаётся битого трека.
    let trackTempFilePath = null;
    let trackCoverPath = null;
    try {
      const settings = makhno_readSettings();
      const saveFolder = customDownloadPath || settings.downloadFolderPath || makhno_defaultDownloadPath;
      if (!makhno_fs.existsSync(saveFolder)) {
        makhno_fs.mkdirSync(saveFolder, { recursive: true });
      }

      const ffmpegPath = await makhno_resolveFfmpeg();
      if (!ffmpegPath) {
        return {
          error:
            "ffmpeg не найден. Установите ffmpeg в системе (sudo apt install ffmpeg / winget install ffmpeg) или укажите путь в настройках мода.",
        };
      }

      const fileExtension = downloadInfo.codec && String(downloadInfo.codec).includes("flac") ? "flac" : "mp3";
      const artists = (Array.isArray(trackMeta?.artists) ? trackMeta.artists : [])
        .map((a) => a?.name)
        .filter(Boolean);
      const trackFileName = makhno_sanitize(
        `${artists.join(", ")} - ${trackMeta?.title || "track"} ${trackMeta?.version || ""}`,
      )
        .trim()
        .substring(0, 250);
      const trackFilePath = makhno_path.join(saveFolder, `${trackFileName}.${fileExtension}`);
      const tmpBase = `makhno-tmp-${process.pid}-${Math.random().toString(36).substring(2, 8)}`;
      trackTempFilePath = makhno_path.join(saveFolder, `${tmpBase}.${fileExtension}`);
      trackCoverPath = makhno_path.join(saveFolder, `${tmpBase}.jpg`);

      const response = await makhno_axios.get(downloadInfo.url, {
        responseType: "arraybuffer",
        validateStatus: () => true,
      });
      if (response.status !== 200) {
        console.error(`[${MAKHNO_MOD_NAME}] download failed:`, response.status);
        return { error: "Download failed: HTTP " + response.status };
      }

      const decryptedData = await makhno_decryptYandexAudio(response.data, downloadInfo.key);
      await makhno_fsp.writeFile(trackTempFilePath, Buffer.from(decryptedData));
      console.log(`[${MAKHNO_MOD_NAME}] decrypted OK`);

      // Один вызов ffmpeg: remux + метаданные (+ обложка).
      const ffmpegArgs = ["-i", trackTempFilePath];
      if (trackMeta?.coverUri) {
        try {
          const coverUrl = `https://${trackMeta.coverUri.replaceAll("%%", "orig")}`;
          const coverResponse = await makhno_axios.get(coverUrl, { responseType: "arraybuffer" });
          await makhno_fsp.writeFile(trackCoverPath, coverResponse.data);
          ffmpegArgs.push("-i", trackCoverPath, "-map", "0:a", "-map", "1:v");
        } catch (err) {
          console.warn(`[${MAKHNO_MOD_NAME}] cover download failed:`, err);
        }
      }

      ffmpegArgs.push("-y", "-c", "copy", "-id3v2_version", "3");
      if (trackMeta?.title) ffmpegArgs.push("-metadata", `title=${trackMeta.title}`);
      if (trackMeta?.version) ffmpegArgs.push("-metadata", `subtitle=${trackMeta.version}`);
      if (artists.length > 0) ffmpegArgs.push("-metadata", `artist=${artists.join("/")}`);
      if (trackMeta?.albums?.[0]?.title) ffmpegArgs.push("-metadata", `album=${trackMeta.albums[0].title}`);
      if (trackMeta?.albums?.[0]?.genre) ffmpegArgs.push("-metadata", `genre=${trackMeta.albums[0].genre}`);
      if (trackMeta?.albums?.[0]?.trackPosition?.index) {
        ffmpegArgs.push("-metadata", `track=${trackMeta.albums[0].trackPosition.index}`);
      }
      if (trackMeta?.albums?.[0]?.year) ffmpegArgs.push("-metadata", `date=${trackMeta.albums[0].year}`);
      if (trackMeta?.albums?.[0]?.releaseDate) {
        ffmpegArgs.push("-metadata", `releaseDate=${trackMeta.albums[0].releaseDate}`);
      }
      ffmpegArgs.push("-metadata", `encoded_by=${MAKHNO_MOD_NAME} ${MAKHNO_MOD_VERSION}`);
      ffmpegArgs.push(trackFilePath);

      await makhno_execFileAsync(ffmpegPath, ffmpegArgs);

      console.log(`[${MAKHNO_MOD_NAME}] download completed:`, trackFilePath);
      return { ok: true, path: trackFilePath };
    } catch (err) {
      console.error(`[${MAKHNO_MOD_NAME}] download failed:`, err);
      const ffmpegHint = err.stderr ? `\nffmpeg: ${String(err.stderr).split("\n").slice(-3).join(" ").trim()}` : "";
      return {
        error: "Download or decryption failed: " + (err && err.message ? err.message : String(err)) + ffmpegHint,
      };
    } finally {
      // Уборка временных файлов в любом исходе
      for (const tempFile of [trackTempFilePath, trackCoverPath]) {
        if (!tempFile) continue;
        try {
          if (makhno_fs.existsSync(tempFile)) await makhno_fsp.unlink(tempFile);
        } catch (e) {}
      }
    }
  },
);

// ---------- Discord RPC ----------
(() => {
  const { BrowserWindow } = makhno_electron;
  const { Client } = require("@xhayper/discord-rpc");

  const CLIENT_ID = "1283109459463377011";
  const ACTIVITY_COOLDOWN = 10 * 1000;

  let makhno_lastActivityChanged = 0;
  let makhno_activityVisible = false;
  let makhno_rpcClient = null;
  let makhno_rpcStarting = false;

  function makhno_initRpc() {
    if (makhno_rpcStarting) return;
    makhno_rpcStarting = true;
    // старый клиент освобождает IPC-сокет Discord, иначе копились бы утечки
    try {
      makhno_rpcClient?.destroy?.();
    } catch (e) {}
    makhno_rpcClient = null;
    const client = new Client({ clientId: CLIENT_ID });
    makhno_rpcClient = client;
    client
      .login()
      .catch((e) => {
        console.error("[DISCORD RPC]", e?.message || e);
        setTimeout(makhno_initRpc, 5000);
      })
      .finally(() => {
        makhno_rpcStarting = false;
      });
    client.on("ready", () => console.log("[DISCORD RPC] Hooked!"));
    client.on("disconnected", () => setTimeout(makhno_initRpc, 5000));
    client.on("error", () => setTimeout(makhno_initRpc, 5000));
    client.on("close", () => setTimeout(makhno_initRpc, 5000));
  }

  async function makhno_getAppPlayerState() {
    const [win] = BrowserWindow.getAllWindows();
    if (win && !win.isDestroyed()) {
      return win.webContents.executeJavaScript(
        "(() => window.__getPlayerState ? window.__getPlayerState() : null)()",
      );
    }
    return null;
  }

  async function makhno_updateActivity() {
    setTimeout(makhno_updateActivity, 500);
    if (makhno_lastActivityChanged + ACTIVITY_COOLDOWN > Date.now()) return;
    if (!makhno_rpcClient || !makhno_rpcClient.user) return;
    try {
      const playerState = await makhno_getAppPlayerState();
      if (!playerState || !playerState.enabled || !playerState.data || !playerState.data.isPlaying) {
        // clearActivity дёргаем только при реальном переходе в паузу,
        // а не каждые полсекунды (как было раньше)
        if (makhno_activityVisible) {
          makhno_activityVisible = false;
          makhno_rpcClient.user.clearActivity();
        }
        return;
      }
      const d = playerState.data;
      const position = Number(d.playback?.position) || 0;
      const duration = Number(d.playback?.duration) || 0;
      const startTimestamp = Math.round(Date.now() - position * 1000);
      const endTimestamp = Math.round(Date.now() + (duration - position) * 1000);
      const rpcRequest = {
        type: 2,
        details: d.trackMeta.version ? `${d.trackMeta.title} ${d.trackMeta.version}` : d.trackMeta.title,
        largeImageKey: d.trackMeta.coverUri
          ? `https://${d.trackMeta.coverUri.replaceAll("%%", "300x300")}`
          : undefined,
        state: (Array.isArray(d.trackMeta.artists) ? d.trackMeta.artists : []).map((a) => a?.name).filter(Boolean).join(", "),
        startTimestamp,
        endTimestamp,
        buttons: [
          {
            label: "Открыть",
            url: `https://music.yandex.ru/track/${d.trackMeta.id}`,
          },
        ],
        instance: false,
      };
      if (playerState.showModButton) {
        rpcRequest.buttons.push({
          label: "MakhnoYMMod",
          url: "https://github.com/Ortodoxian88/MakhnoYMMod",
        });
      }
      makhno_rpcClient.user.setActivity(rpcRequest);
      makhno_lastActivityChanged = Date.now();
      makhno_activityVisible = true;
    } catch (ex) {
      console.log("[DISCORD RPC]", ex);
    }
  }

  makhno_initRpc();
  makhno_updateActivity();
})();
