// MakhnoYMMod 2.2.3 — prologue. Вставляется в начало index.js ПЕРЕД кодом клиента.
// Читает настройки мода до старта приложения (нужно для devtools/systemToolbar).
const makhnoYMMod_fs = require("fs");
const makhnoYMMod_path = require("path");
const makhnoYMMod_electron = require("electron");
const makhnoYMMod_appFolder = makhnoYMMod_electron.app.getPath("userData");
const makhnoYMMod_settingsFilePath = makhnoYMMod_path.join(makhnoYMMod_appFolder, "mod_settings.json");
let enableSystemToolbar = false;
try {
  const makhnoYMMod_settings = JSON.parse(makhnoYMMod_fs.readFileSync(makhnoYMMod_settingsFilePath, "utf8"));
  enableSystemToolbar = makhnoYMMod_settings["devtools/systemToolbar"] === true;
} catch (e) {}
