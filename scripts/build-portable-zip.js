// Build the portable (green) distribution zip from the release build.
//
// Tauri 2 on Windows only ships `msi`/`nsis` bundles — there is no built-in
// portable zip target. This script produces one: the standalone `ipet.exe`
// plus the `WebView2Loader.dll` it loads at runtime, zipped together. The
// version is read from src-tauri/tauri.conf.json so the archive name stays in
// sync with `npm run tauri:build` output.
//
// Run AFTER `npm run tauri:build` (the release exe must exist):
//   npm run dist:zip
//
// Output: target/release/bundle/zip/iPet_<version>_x64_portable.zip

import { readFile, writeFile, mkdir, stat } from "node:fs/promises";
import { deflateRawSync } from "node:zlib";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = new URL("../", import.meta.url);
const TARGET_RELEASE = new URL("../target/release/", import.meta.url);
const Tauri_CONF = new URL("../src-tauri/tauri.conf.json", import.meta.url);

async function readVersion() {
  const conf = JSON.parse(await readFile(Tauri_CONF, "utf8"));
  if (!conf.version) throw new Error("tauri.conf.json missing `version`");
  return conf.version;
}

/// Minimal ZIP writer (store + deflate). We hand-roll it instead of adding a
/// dep so the script stays zero-install. Each entry: local file header + raw
/// deflated data; followed by a central directory + EOCD.
function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return (~c) >>> 0;
}

function dosTime(date) {
  return (
    ((date.getHours() & 0x1f) << 11) |
    ((date.getMinutes() & 0x3f) << 5) |
    ((date.getSeconds() >> 1) & 0x1f)
  );
}

function dosDate(date) {
  return (
    (((date.getFullYear() - 1980) & 0x7f) << 9) |
    (((date.getMonth() + 1) & 0x0f) << 5) |
    (date.getDate() & 0x1f)
  );
}

function u16(n) {
  return Buffer.from([n & 0xff, (n >>> 8) & 0xff]);
}
function u32(n) {
  return Buffer.from([
    n & 0xff,
    (n >>> 8) & 0xff,
    (n >>> 16) & 0xff,
    (n >>> 24) & 0xff,
  ]);
}

async function buildZip(entryPaths, outPath) {
  const entries = [];
  const chunks = [];
  let offset = 0;

  for (const absPath of entryPaths) {
    const data = await readFile(absPath);
    const name = basename(absPath);
    const nameBuf = Buffer.from(name, "utf8");
    const crc = crc32(data);

    // Deflate the payload; fall back to STORE (method 0) if deflate doesn't
    // shrink it (rare for already-compressed binaries, but correct).
    let method = 0;
    let payload = data;
    try {
      const deflated = deflateRawSync(data);
      if (deflated.length < data.length) {
        method = 8;
        payload = deflated;
      }
    } catch {
      // STORE — uncompressed.
    }

    const fileMod = new Date();
    const localHeader = Buffer.concat([
      u32(0x04034b50), // local file header sig
      u16(20), // version needed
      u16(0), // flags
      u16(method),
      u16(dosTime(fileMod)),
      u16(dosDate(fileMod)),
      u32(crc),
      u32(payload.length),
      u32(data.length),
      u16(nameBuf.length),
      u16(0), // extra len
      nameBuf,
    ]);

    chunks.push(localHeader, payload);
    entries.push({ name, nameBuf, crc, method, payload, compressedSize: payload.length, uncompressedSize: data.length, offset });
    offset += localHeader.length + payload.length;
  }

  // Central directory.
  const cdEntries = [];
  for (const e of entries) {
    const fileMod = new Date();
    cdEntries.push(
      Buffer.concat([
        u32(0x02014b50), // central dir sig
        u16(20), // version made by
        u16(20), // version needed
        u16(0), // flags
        u16(e.method),
        u16(dosTime(fileMod)),
        u16(dosDate(fileMod)),
        u32(e.crc),
        u32(e.compressedSize),
        u32(e.uncompressedSize),
        u16(e.nameBuf.length),
        u16(0), // extra
        u16(0), // comment
        u16(0), // disk number
        u16(0), // internal attrs
        u32(0), // external attrs
        u32(e.offset),
        e.nameBuf,
      ]),
    );
  }
  const cd = Buffer.concat(cdEntries);
  const eocd = Buffer.concat([
    u32(0x06054b50), // EOCD sig
    u16(0), u16(0), // disk numbers
    u16(entries.length), u16(entries.length),
    u32(cd.length),
    u32(offset), // CD offset
    u16(0), // comment len
  ]);

  await mkdir(dirname(outPath), { recursive: true });
  await writeFile(outPath, Buffer.concat([...chunks, cd, eocd]));
  return outPath;
}

async function main() {
  const version = await readVersion();
  const releaseDir = fileURLToPath(TARGET_RELEASE);

  // Files a portable copy needs at runtime: the exe + the WebView2 loader.
  // The icon is compiled into the exe, so no extra resource files.
  const files = ["ipet.exe", "WebView2Loader.dll"].map((f) => join(releaseDir, f));
  for (const f of files) {
    try {
      await stat(f);
    } catch {
      throw new Error(
        `缺少 ${f}。请先运行 \`npm run tauri:build\` 生成 release 产物,再打包 zip。`,
      );
    }
  }

  const outDir = join(releaseDir, "bundle", "zip");
  const outPath = join(outDir, `iPet_${version}_x64_portable.zip`);
  await buildZip(files, outPath);

  const size = (await stat(outPath)).size;
  console.log(`✓ portable zip: ${outPath} (${(size / 1048576).toFixed(2)} MiB)`);
  console.log(`  contents: ${files.map((f) => basename(f)).join(", ")}`);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
