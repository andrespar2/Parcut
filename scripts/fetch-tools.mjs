#!/usr/bin/env node
/**
 * Descarga las herramientas externas que ParCut empaqueta:
 *   - FFmpeg + FFprobe (build GPL compartido, incluye libx264) en src-tauri/binaries/ffmpeg
 *   - yt-dlp (Unlicense) en src-tauri/binaries/yt-dlp.exe
 *
 * Los binarios no se versionan en Git por tamaño. Ejecuta `npm run tools:fetch`
 * tras clonar el repositorio.
 */
import { createWriteStream } from "node:fs";
import { cp, mkdir, readdir, rm, stat } from "node:fs/promises";
import { execFile } from "node:child_process";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { promisify } from "node:util";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const binaries = path.join(root, "src-tauri", "binaries");
const ffmpegDir = path.join(binaries, "ffmpeg");
const ytdlpPath = path.join(binaries, "yt-dlp.exe");
const force = process.argv.includes("--force");

const FFMPEG_URL =
  "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip";
const YTDLP_URL = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

async function exists(target) {
  try {
    await stat(target);
    return true;
  } catch {
    return false;
  }
}

async function download(url, destination) {
  console.log(`→ Descargando ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`No se pudo descargar (${response.status}): ${url}`);
  }
  const total = Number(response.headers.get("content-length") ?? 0);
  let received = 0;
  let lastLog = Date.now();
  const source = Readable.fromWeb(response.body);
  source.on("data", (chunk) => {
    received += chunk.length;
    if (Date.now() - lastLog > 4000) {
      lastLog = Date.now();
      const mb = (received / 1048576).toFixed(1);
      const pct = total ? ` (${((received / total) * 100).toFixed(0)}%)` : "";
      process.stdout.write(`  ${mb} MB${pct}\n`);
    }
  });
  await pipeline(source, createWriteStream(destination));
}

async function fetchFfmpeg() {
  if (!force && (await exists(path.join(ffmpegDir, "ffmpeg.exe")))) {
    console.log("• FFmpeg ya está instalado (usa --force para actualizarlo).");
    return;
  }
  const tmp = await mkdir(path.join(os.tmpdir(), `parcut-ffmpeg-${Date.now()}`), { recursive: true });
  const zip = path.join(tmp, "ffmpeg.zip");
  try {
    await download(FFMPEG_URL, zip);
    console.log("→ Extrayendo FFmpeg…");
    try {
      await execFileAsync("tar", ["-xf", zip, "-C", tmp]);
    } catch {
      await execFileAsync("powershell", [
        "-NoProfile",
        "-Command",
        `Expand-Archive -LiteralPath '${zip}' -DestinationPath '${tmp}' -Force`,
      ]);
    }
    const entries = await readdir(tmp, { withFileTypes: true });
    const dir = entries.find((entry) => entry.isDirectory() && entry.name.startsWith("ffmpeg-"));
    if (!dir) throw new Error("El paquete de FFmpeg no tiene la estructura esperada.");
    const extractedBin = path.join(tmp, dir.name, "bin");
    await rm(ffmpegDir, { recursive: true, force: true });
    await mkdir(ffmpegDir, { recursive: true });
    const files = await readdir(extractedBin);
    for (const file of files) {
      if (file === "ffplay.exe") continue;
      await cp(path.join(extractedBin, file), path.join(ffmpegDir, file));
    }
    const license = path.join(tmp, dir.name, "LICENSE.txt");
    if (await exists(license)) {
      await cp(license, path.join(ffmpegDir, "LICENSE.txt"));
    }
    console.log("✓ FFmpeg (GPL, con libx264) instalado en src-tauri/binaries/ffmpeg");
  } finally {
    await rm(tmp, { recursive: true, force: true });
  }
}

async function fetchYtDlp() {
  if (!force && (await exists(ytdlpPath))) {
    console.log("• yt-dlp ya está instalado (usa --force para actualizarlo).");
    return;
  }
  await mkdir(binaries, { recursive: true });
  await download(YTDLP_URL, ytdlpPath);
  console.log("✓ yt-dlp instalado en src-tauri/binaries/yt-dlp.exe");
}

async function main() {
  if (process.platform !== "win32") {
    console.warn("Aviso: este script descarga binarios de Windows x64.");
  }
  await mkdir(binaries, { recursive: true });
  await fetchFfmpeg();
  await fetchYtDlp();
  console.log("\nHerramientas listas. Ejecuta `npm run tauri:dev`.");
}

main().catch((error) => {
  console.error(`\nError: ${error.message}`);
  process.exit(1);
});
