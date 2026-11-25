#!/usr/bin/env python3
"""Независимая верификация пропатченных asar (python-реализация, не Rust).
Проверяет: структуру asar, содержимое index.js/preload.js/package.json/renderer.js,
консистентность IPC-каналов между тремя файлами, отсутствие DSN и legacy-имён."""
import struct, json, sys, os, re

def read_asar_files(path, wanted):
    f = open(path, 'rb')
    b = f.read(8)
    header_len = struct.unpack('<I', b[4:8])[0]
    hb = f.read(header_len)
    json_len = struct.unpack('<I', hb[4:8])[0]
    header = json.loads(hb[8:8+json_len])
    data_start = 8 + header_len
    entries = {}
    def walk(node, prefix):
        for name, ent in node.get('files', {}).items():
            rel = prefix + name
            if 'files' in ent:
                walk(ent, rel + '/')
            else:
                entries[rel] = ent
    walk(header, '')
    out = {}
    for rel in wanted:
        ent = entries.get(rel)
        if ent is None:
            out[rel] = None
            continue
        if ent.get('unpacked'):
            out[rel] = '<unpacked>'
            continue
        off = int(ent['offset']); size = ent['size']
        f.seek(data_start + off)
        out[rel] = f.read(size).decode('utf-8', errors='replace')
    return out, entries

def verify(asar_path):
    print(f"===== {asar_path} =====")
    wanted = ['index.js', 'preload.js', 'package.json', 'app/makhnoYMMod/renderer.js']
    files, entries = read_asar_files(asar_path, wanted)
    ok, fail = 0, 0
    def chk(cond, what):
        nonlocal ok, fail
        if cond: ok += 1
        else:
            fail += 1
            print(f"  [FAIL] {what}")

    index = files['index.js'] or ''
    preload = files['preload.js'] or ''
    pkg = json.loads(files['package.json']) if files['package.json'] else {}
    renderer = files['app/makhnoYMMod/renderer.js'] or ''

    # 1. структура
    chk('app/makhnoYMMod/renderer.css' in entries, 'renderer.css в asar')
    chk('app/makhnoYMMod/renderer.js' in entries, 'renderer.js в asar')
    chk('app/yandexMusicMod/renderer.js' not in entries, 'старого каталога нет в asar')
    chk('app/media/splash_screen' not in json.dumps(entries), 'splash отсутствует')
    chk('node_modules/fluent-ffmpeg/package.json' not in entries, 'fluent-ffmpeg отсутствует')
    chk('node_modules/ts-md5/package.json' not in entries, 'ts-md5 отсутствует')
    chk('node_modules/ffmpeg-static/package.json' not in entries, 'ffmpeg-static отсутствует (не bundlится)')
    chk('node_modules/axios/package.json' in entries, 'axios в asar')
    chk('node_modules/sanitize-filename/package.json' in entries, 'sanitize-filename в asar')
    chk('node_modules/@xhayper/discord-rpc/package.json' in entries, 'discord-rpc в asar')

    # 2. index.js
    chk(index.startswith('// MakhnoYMMod head begin'), 'index.js начинается с пролога')
    chk('// MakhnoYMMod main begin' in index and '// MakhnoYMMod main end' in index, 'main-блок с маркерами')
    chk('makhnoYMMod.getStorageValue' in index, 'IPC getStorageValue')
    chk('makhnoYMMod.downloadTrack' in index, 'IPC downloadTrack')
    chk('yandexMusicMod' not in index, 'в index.js нет yandexMusicMod')
    chk('encoded_by' in index, 'encoded_by метка')
    chk('MakhnoYMMod Downloads' in index, 'каталог загрузок по умолчанию')
    chk('shell.openPath' in index, 'кроссплатформенное открытие папок')
    chk('execFile' in index, 'execFile вместо exec')
    chk('makhnoYMMod.axios' in index and 'makhno_allowedHosts' in index, 'axios-мост с белым списком')
    chk('window.__getPlayerState' in index, 'Discord RPC хук плеера')
    chk('1283109459463377011' in index, 'Discord CLIENT_ID')
    chk(index.count('constructor() {') >= 1 and re.search(r'constructor\(\)\s*\{\s*return', index), 'апдейтер убит')

    # 3. IPC-каналы: полный набор в index.js и preload.js
    channels = ['getStorageValue', 'setStorageValue', 'storageValueUpdated', 'selectDownloadFolder',
                'openFolder', 'openDownloadDirectory', 'downloadTrack', 'axios']
    for ch in channels:
        chk(f'makhnoYMMod.{ch}' in index, f'index.js: канал {ch}')
        chk(f'makhnoYMMod.{ch}' in preload, f'preload.js: канал {ch}')
    chk('exposeInMainWorld("makhnoYMMod"' in preload, 'preload открывает мост makhnoYMMod')
    chk('yandexMusicMod' not in preload, 'preload без legacy')

    # 4. renderer
    chk('window.makhnoYMMod' in renderer, 'renderer зовёт мост makhnoYMMod')
    chk('yandexMusicMod' not in renderer, 'renderer без legacy-имён')
    chk('ingest.de.sentry.io' not in renderer, 'DSN удалён')
    chk('@o451088' not in renderer, 'проект телеметрии удалён')
    chk('dsn: ""' in renderer, 'Sentry.init с пустым DSN')
    chk('__makhnoYMModAnalyticsEnabled' in renderer, 'флаг аналитики переименован')
    chk('"2.2.3"' in renderer, 'версия 2.2.3')
    chk('__MAKHNO_' not in renderer, 'токены подставлены')
    chk('Stephanzion' not in renderer, 'атрибуты старого автора удалены')
    chk('YandexMusicBetaMod' not in renderer, 'упоминаний старого мода нет')

    # 5. package.json
    chk(pkg.get('name') == 'MakhnoYMMod', 'name = MakhnoYMMod')
    chk(pkg.get('modVersion') == '2.2.3', 'modVersion = 2.2.3')
    deps = pkg.get('dependencies', {})
    for banned in ['fluent-ffmpeg', 'ts-md5', 'ffmpeg-static', 'cmdk', '@sentry/react', 'neverthrow', 'zod', '@radix-ui/react-slider']:
        chk(banned not in deps, f'dep {banned} удалён')
    for needed in ['axios', 'sanitize-filename', '@xhayper/discord-rpc']:
        chk(needed in deps, f'dep {needed} есть')

    print(f"  OK: {ok}, FAIL: {fail}")
    return fail

if __name__ == '__main__':
    total = 0
    for p in sys.argv[1:]:
        total += verify(p)
    sys.exit(1 if total else 0)
