# Compilación y distribución

## 1. Requisitos del entorno

| Componente | Versión recomendada | Notas |
|---|---|---|
| Windows | 10/11 x64 | WebView2 Runtime (incluido en Windows 11). |
| Node.js | 20 LTS o superior | `npm` incluido. |
| Rust | stable MSVC | Instalar con [rustup](https://rustup.rs). |
| Microsoft C++ Build Tools | 2022/2026 | Carga de trabajo «Desarrollo para el escritorio con C++» + Windows SDK. |
| WebView2 | Evergreen | El instalador de Tauri puede descargarlo si falta. |

Ejemplo de instalación de Rust en PowerShell:

```powershell
# Descarga rustup-init y usa el toolchain MSVC por defecto
Invoke-WebRequest https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe -OutFile rustup-init.exe
.\rustup-init.exe -y --default-toolchain stable --profile minimal
```

Añade `%USERPROFILE%\.cargo\bin` al `PATH` de la sesión o del sistema.

## 2. Preparar dependencias

```powershell
git clone https://github.com/andrespar2/Parcut.git
cd Parcut
npm install
npm run tools:fetch
```

`tools:fetch` descarga a `src-tauri/binaries/` (ignorado por Git):

- **FFmpeg + FFprobe**: build compartido `ffmpeg-master-latest-win64-gpl-shared` de
  [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds), con `libx264` incluido.
- **yt-dlp**: ejecutable oficial del repositorio [yt-dlp](https://github.com/yt-dlp/yt-dlp).

Si los binarios no existen, la aplicación arranca igualmente: la importación y la división avisan
de que falta FFmpeg; las descargas avisan de que falta yt-dlp.

## 3. Desarrollo

```powershell
npm run tauri:dev
```

Vite sirve la interfaz en `http://localhost:1420` y Tauri abre la ventana con el backend Rust.
Los datos de prueba se guardan en `%LOCALAPPDATA%\ParCut`.

Variables útiles:

- `PARCUT_FFMPEG_DIR`: carpeta alternativa que contiene `ffmpeg.exe`/`ffprobe.exe`.
- `PARCUT_YTDLP_DIR`: carpeta alternativa que contiene `yt-dlp.exe`.

## 4. Pruebas

```powershell
npm test                     # pruebas de la interfaz
npm run build                # typecheck + build de producción del frontend
cd src-tauri; cargo test     # pruebas del backend
```

## 5. Compilación del instalador

```powershell
npm run tauri:build
```

Salidas:

- Ejecutable: `src-tauri/target/release/parcut.exe`
- Instalador NSIS: `src-tauri/target/release/bundle/nsis/ParCut_1.0.0_x64-setup.exe`

El instalador es **por usuario** (`installMode: currentUser`), no requiere UAC, crea el acceso
directo y registra la desinstalación. Los recursos empaquetados (FFmpeg, FFprobe, yt-dlp y sus
DLLs) se copian junto al ejecutable en las carpetas `ffmpeg\` y `yt-dlp\`.

Para generar además un MSI, cambia en `src-tauri/tauri.conf.json`:

```json
"targets": ["nsis", "msi"]
```

(El MSI requiere WiX, que Tauri descarga automáticamente.)

### Firma de código (opcional, recomendado para distribución)

Sin certificado, Windows SmartScreen mostrará una advertencia. Con un certificado de firma de
código:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "..."
# o configura el certificado en las opciones de bundling de Tauri
```

No se incluye ningún certificado en el repositorio.

## 6. Iconos e identidad

```powershell
npm run icons
npx tauri icon assets/icon.png
```

`assets/icon.svg` contiene la marca; `src-tauri/icons/` el conjunto completo (ICO, PNG, Store).

## 7. Licencias de terceros

### FFmpeg (GPLv3)

ParCut distribuye FFmpeg/FFprobe como **programas independientes** invocados por proceso. Se usa
una build **GPLv3 compartida** para disponer de `libx264` y garantizar cortes precisos con buena
calidad. Obligaciones al redistribuir:

1. Incluir el texto de licencia (`src-tauri/binaries/ffmpeg/LICENSE.txt`, empaquetado como
   recurso).
2. Ofrecer el código fuente correspondiente de FFmpeg y de las bibliotecas GPL incluidas
   (por ejemplo, x264):
   - Fuente de FFmpeg: <https://ffmpeg.org/download.html>
   - Build utilizada: <https://github.com/BtbN/FFmpeg-Builds> (carpeta `ffmpeg-master-latest-win64-gpl-shared`)
   - Fuente de x264: <https://code.videolan.org/videolan/x264>
3. No imponer restricciones adicionales sobre el binario de FFmpeg.
4. Mantener los avisos de copyright.

El código propio de ParCut es independiente y se distribuye bajo MIT; FFmpeg se ejecuta como
proceso separado y no se enlaza estáticamente.

### yt-dlp (Unlicense)

Se distribuye el ejecutable oficial sin modificaciones. No impone obligaciones de atribución,
pero se acredita en README y en esta documentación.

### Dependencias Rust y JavaScript

Tauri, React, rusqlite, reqwest, tokio y el resto de crates/paquetes usan licencias MIT,
Apache-2.0 o BSD. Los avisos están disponibles en sus repositorios y en `Cargo.lock` /
`package-lock.json`. Antes de una distribución comercial, revisa los avisos generados por tu
herramienta de cumplimiento.

## 8. Verificación del ejecutable

Resultados de la compilación verificada:

- `parcut.exe`: 16,4 MB.
- Instalador NSIS: 80,50 MiB.
- Instalación silenciosa con `/S /D=<carpeta>`: correcta.
- El ejecutable instalado detectó FFmpeg, FFprobe y yt-dlp desde su carpeta de instalación.
- Desinstalación con `uninstall.exe /S`: eliminó la carpeta de instalación.
- Los datos de usuario (`%LOCALAPPDATA%\ParCut`) se conservan tras desinstalar; bórralos
  manualmente si quieres eliminar proyectos y cuentas.

Lista de comprobación antes de publicar:

1. `npm run build` sin errores de TypeScript.
2. `cargo test` y `npm test` en verde.
3. `npm run tauri:build` genera el `.exe` y el instalador.
4. Instalar en una cuenta limpia de Windows 10/11 y abrir ParCut.
5. Comprobar que FFmpeg, FFprobe y yt-dlp aparecen como «Disponible» en Ajustes.
6. Importar un video local, dividirlo y reproducir un clip.
7. Verificar que las miniaturas y los clips persisten tras cerrar y abrir.
8. Comprobar espacio usado, limpieza de temporales y eliminación completa de un proyecto.
9. (Con credenciales) conectar una cuenta, publicar una parte y verificar el ID remoto.
10. Desinstalar y confirmar que no quedan accesos directos ni servicios.

## 9. Problemas conocidos del empaquetado

| Síntoma | Solución |
|---|---|
| `failed to run custom build command … unknown field` | La CLI de Tauri y los crates deben ser de la misma generación (2.x). Ejecuta `cargo update` en `src-tauri`. |
| `frontendDist` no existe | Ejecuta `npm run build` antes de `cargo check`/`build`. |
| El instalador intenta descargar NSIS y falla | Comprueba la conexión o configura `TAURI_BUNDLER_TOOLS`/caché de herramientas. |
| «FFmpeg no está disponible» tras instalar | Verifica que `ffmpeg\ffmpeg.exe` (junto al ejecutable) exista y que el antivirus no haya puesto en cuarentena los binarios. |
| El video no se reproduce | Revisa que el protocolo `asset` esté habilitado y que el archivo esté dentro de `%LOCALAPPDATA%\ParCut` o del directorio configurado. |
| Error de enlace `link.exe not found` | Instala los Build Tools de Visual Studio con el SDK de Windows. |
