  const makhno_electron = require("electron");

  makhno_electron.contextBridge.exposeInMainWorld("makhnoYMMod", {
    getStorageValue: (key) => makhno_electron.ipcRenderer.invoke("makhnoYMMod.getStorageValue", key),
    setStorageValue: (key, value) => makhno_electron.ipcRenderer.send("makhnoYMMod.setStorageValue", key, value),
    onStorageChanged: (cb) => {
      const listener = (_e, key, value) => cb(key, value);
      makhno_electron.ipcRenderer.on("makhnoYMMod.storageValueUpdated", listener);
      return () => makhno_electron.ipcRenderer.removeListener("makhnoYMMod.storageValueUpdated", listener);
    },
    downloadTrack: (downloadInfo, trackMeta, customDownloadPath) =>
      makhno_electron.ipcRenderer.invoke(
        "makhnoYMMod.downloadTrack",
        downloadInfo,
        trackMeta,
        customDownloadPath,
      ),
    openDownloadDirectory: () => makhno_electron.ipcRenderer.send("makhnoYMMod.openDownloadDirectory"),
    selectDownloadFolder: () => makhno_electron.ipcRenderer.invoke("makhnoYMMod.selectDownloadFolder"),
    openFolder: (folderPath) => makhno_electron.ipcRenderer.invoke("makhnoYMMod.openFolder", folderPath),
    axios: (config) => makhno_electron.ipcRenderer.invoke("makhnoYMMod.axios", config),
  });

  console.log("[MakhnoYMMod] preload bridge ready");
