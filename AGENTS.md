EDITAR SIEMPRE EL AGENTS.MD CON EL FIN DE MANTENER EL CONTEXTO DEL PROYECTO ACTUALIZADO Y LO MAS ENTENDIBLE POSIBLE PARA QUE LAS DEMAS IA´S O AGENTES PUEDAN REVISARLO Y AHORRAR TIEMPO AVERIGUANDO PARA QUE ES, COMO FUNCIONA O COMO ESTA HECHO

# ParCut — Memoria técnica del proyecto

> **Desarrollado por: andrespar2**
> Aplicación de escritorio para Windows 10/11 (x64) que importa o descarga videos, los divide
> en clips por duración y los publica de forma secuencial en YouTube, Facebook y TikTok.

**Versión actual:** 1.0.0 · **Repositorio:** https://github.com/andrespar2/Parcut

---

## 0. Cómo usar este documento

1. Lee este archivo completo antes de tocar código.
2. Al terminar cualquier cambio relevante, actualiza las secciones afectadas (arquitectura,
   funcionalidades, pruebas, errores e historial).
3. No borres historial ni decisiones anteriores: añade y reorganiza, no elimines.
4. Antes de dar una tarea por terminada: `npm run build`, `npm test`, `cargo test` y
   `cargo check` deben pasar.
5. Documentación complementaria:
   - `README.md` — presentación pública y uso.
   - `docs/ARCHITECTURE.md` — arquitectura detallada.
   - `docs/PUBLISHING.md` — requisitos OAuth y API por plataforma.
   - `docs/BUILD.md` — compilación, instalador y licencias.
   - `CHANGELOG.md` — historial de versiones.

---

## 1. Resumen del producto

Flujo completo, sin datos simulados:

1. **Obtener el video**: importar un archivo local o descargar contenido autorizado con `yt-dlp`.
2. **Analizarlo**: FFprobe extrae duración, resolución, fps, códecs, contenedor y tamaño.
3. **Dividirlo**: FFmpeg genera N clips de la duración elegida; el último conserva el resto.
4. **Gestionarlo**: cada proyecto guarda video fuente, clips, miniaturas y metadatos en su
   propio directorio administrado.
5. **Publicarlo**: cola **estrictamente secuencial** que espera la confirmación real de la
   plataforma antes de continuar.
6. **Limpiarlo**: «Eliminar todo el proyecto» borra video, clips, miniaturas, temporales y
   metadatos, verificando que ninguna ruta salga del espacio de ParCut.

Las integraciones sociales requieren credenciales de desarrollador propias; la interfaz lo
indica y nunca finge una conexión.

---

## 2. Stack y decisiones técnicas

| Área | Tecnología | Motivo |
|---|---|---|
| Shell de escritorio | **Tauri 2** | Binario nativo pequeño, WebView2 en Windows 10/11, IPC tipado y NSIS. |
| UI | **React 19 + TypeScript + Vite 7** | Componentes reutilizables, HMR y tipado estricto. |
| Estado | **Zustand** | Store mínimo sin boilerplate. |
| Iconos | **lucide-react** | Iconografía minimalista coherente. |
| Backend | **Rust + Tokio** | Procesos, subidas en streaming, cola persistente y cancelación. |
| Persistencia | **SQLite (rusqlite `bundled`)** | Sin dependencia externa; WAL y claves foráneas. |
| Video | **FFmpeg + FFprobe** (sidecar empaquetado) | Cortes sin pérdida y recodificación controlada. |
| Descargas | **yt-dlp** (sidecar) | Único mecanismo mantenible y legalmente configurable. |
| Secretos | **Windows DPAPI** (`CryptProtectData`) | Tokens y client secrets cifrados por usuario. |
| Empaquetado | **NSIS (Tauri bundler)** | Instalación por usuario, accesos directos, desinstalación limpia. |

### Decisiones y motivos

- **Base de datos global** en `%LOCALAPPDATA%\ParCut\parcut.db`, separada de los archivos de
  proyecto: permite conservar cuentas y cola al borrar un proyecto.
- **Cuentas en tabla aparte** y tokens cifrados: «Eliminar todo el proyecto» no desconecta
  cuentas.
- **Cola en Rust, no en el frontend**: sobrevive a recargas y cierres; un único trabajador evita
  publicaciones simultáneas.
- **Dos modos de división**:
  - `copy`: `-c copy` + muxer `segment`. Rápido y sin pérdida; cortes alineados a keyframes.
  - `precise`: un proceso por parte con `-ss/-t` y recodificación H.264/AAC (CRF 20), para
    cortes exactos.
- **Detección del codificador H.264** al arrancar: se prefiere `libx264`; si no existe, se usa
  `libopenh264`, `h264_mf`, `h264_nvenc` o `mpeg4`. Evita fallos si el usuario sustituye FFmpeg.
- **FFmpeg GPL compartido**: la build LGPL no incluye `libx264`, imprescindible para una
  recodificación de calidad. Se documentan las obligaciones GPL en `docs/BUILD.md`.
- **Progreso real**: `-progress pipe:1` de FFmpeg (`out_time_us`) y `--newline` de yt-dlp.
- **Cancelación**: `AtomicBool` por tarea + `child.kill()`; limpieza de archivos parciales y de
  filas de clips antes de devolver el error.
- **Reintentos sin duplicados**: `resume_data` cifrado con DPAPI (sesión de YouTube, video de
  Facebook, `publish_id` de TikTok) y consulta de estado antes de reenviar.
- **Rutas seguras**: canonicalización + `starts_with`, identificadores de proyecto restringidos a
  hexadecimal/UUID y validación del nombre de carpeta.
- **Migración de biblioteca**: al cambiar el directorio de almacenamiento se mueven las carpetas
  de los proyectos y se reescriben las rutas absolutas en SQLite dentro de una transacción; si
  algo falla, se revierten los movimientos. Evita proyectos huérfanos.
- **Sin shell**: todos los procesos usan `Command::args`; las calidades se validan contra un
  rango numérico antes de construir el selector de yt-dlp.
- **Tema Liquid Glass** con `backdrop-filter`, capas translúcidas y bordes luminosos; contraste
  cuidado, modo claro/oscuro/sistema. Capturas en `docs/screenshots/`.

---

## 3. Estructura real del repositorio

```
Parcut/
├─ AGENTS.md · README.md · CHANGELOG.md · LICENSE · .gitignore
├─ assets/icon.svg · icon.png
├─ docs/{ARCHITECTURE,PUBLISHING,BUILD}.md
├─ scripts/{fetch-tools.mjs, generate-icon.mjs}
├─ src/
│  ├─ main.tsx · App.tsx
│  ├─ styles/globals.css
│  ├─ lib/{api.ts,format.ts,format.test.ts}
│  ├─ store/app.ts
│  ├─ components/{Sidebar,TopBar,Toasts,ui,PlatformLogo,ProjectCard,ClipCard,PlayerModal}.tsx
│  └─ pages/{HomePage,NewProjectPage,ProjectPage,ClipsPage,PublishPage,AccountsPage,SettingsPage}.tsx
└─ src-tauri/
   ├─ Cargo.toml · tauri.conf.json · capabilities/default.json · build.rs
   ├─ binaries/{ffmpeg/*, yt-dlp.exe}      # gitignored, vía npm run tools:fetch
   ├─ icons/*                              # generados con npx tauri icon
   └─ src/
      ├─ lib.rs · main.rs · state.rs · error.rs · models.rs · db.rs
      ├─ settings.rs · paths.rs · utils.rs · cleanup.rs
      ├─ commands/{mod,system,projects,accounts,publish}.rs
      ├─ media/{mod,ffmpeg,ffprobe,thumbnail,split}.rs
      ├─ download/{mod,ytdlp}.rs
      ├─ projects/mod.rs
      ├─ auth/{mod,credentials,oauth}.rs
      └─ publish/{mod,title,upload,queue}.rs
         publish/platforms/{mod? no, declarado inline,youtube,facebook,tiktok}.rs
```

> Nota: `publish/platforms` se declara como módulo inline en `publish/mod.rs`; no existe
> `platforms/mod.rs`.

---

## 4. Superficie IPC (comandos Tauri)

Registrados en `src-tauri/src/lib.rs`; envueltos tipados en `src/lib/api.ts`.

| Dominio | Comandos |
|---|---|
| Sistema | `app_info`, `tool_status`, `tools_refresh`, `storage_stats`, `temp_cleanup`, `activity_recent`, `settings_get`, `settings_save`, `reveal_path`, `open_external_url`, `project_delete_all` |
| Proyectos | `projects_list`, `project_get`, `project_update`, `project_import`, `clips_list`, `clip_delete`, `split_plan`, `split_start`, `split_cancel` |
| Descargas | `download_metadata`, `download_start`, `download_cancel` |
| Cuentas | `platform_statuses`, `accounts_list`, `oauth_connect`, `oauth_start_manual`, `oauth_complete_manual`, `account_disconnect`, `platform_save_credentials`, `platform_clear_credentials` |
| Publicación | `queue_snapshot`, `queue_enqueue`, `queue_start`, `queue_pause`, `queue_cancel_remaining`, `queue_retry`, `queue_cancel_item` |

Eventos: `toast`, `import:progress`, `download:progress|done|error`, `split:start|progress|clip|done|error|notice`,
`project:updated`, `clips:changed`, `accounts:changed`, `queue:changed|progress|finished`.

---

## 5. Base de datos

`%LOCALAPPDATA%\ParCut\parcut.db` (WAL, claves foráneas).

- `projects(id, name, source_url, platform, original_path, thumbnail_path, duration, width,
  height, fps, size_bytes, video_codec, audio_codec, container, status, hashtags,
  default_clip_seconds, created_at, updated_at)`
- `clips(id, project_id FK, part_index, path, thumbnail_path, start_seconds, end_seconds,
  duration, width, height, size_bytes, status, created_at)`
- `accounts(id, platform, display_name, external_id, scopes, token_blob, connected_at,
  updated_at)` — `token_blob` cifrado con DPAPI, único por `(platform, external_id)`.
- `secrets(key, blob, updated_at)` — client secrets cifrados.
- `publish_queue(id, project_id, clip_id, part_index, platform, account_id, title, description,
  hashtags, status, progress, remote_id, remote_url, error, attempts, resume_data, sort_index,
  created_at, updated_at)`
- `activity(id, level, message, detail, created_at)`

Estados de cola: `pending`, `preparing`, `uploading`, `published`, `error`, `canceled`.
`resume_data` contiene JSON cifrado en base64url.

---

## 6. Estado de funcionalidades

### Implementadas y probadas
- Shell Tauri 2 + React 19 + TS, tema Liquid Glass oscuro/claro/sistema y navegación completa.
- Importación local con copia por bloques, progreso, FFprobe y portada.
- Descarga autorizada con yt-dlp: metadatos, calidades, progreso, cancelación y errores legibles.
- Reproductor integrado y previsualización de clips con pantalla completa.
- División con plan/estimación, presets h/m/s, modos `copy` y `precise`, codificador H.264
  detectado, progreso real, cancelación y limpieza.
- Clips ordenados por parte, miniaturas, selección, reproducción, ubicación, copia y borrado.
- OAuth 2.0 + PKCE y callback local con puertos fijos (17421/17422/17423) y flujo manual de
  respaldo; tokens DPAPI.
- Adaptadores reales: YouTube Resumable, Facebook Page Resumable, TikTok Content Posting.
- Cola secuencial persistente: pausa, reanudar, reintentar, cancelar, progreso, estados reales,
  recuperación tras cierre y protección contra duplicados.
- Títulos `NOMBRE - PARTE N`, hashtags generales/individuales y límites por plataforma.
- Eliminación completa con informe (archivos, bytes, advertencias, verificación).
- Cambio de directorio de almacenamiento con migración real de proyectos y rutas.
- Ahorro de espacio opcional: eliminar el video original después de dividir.
- Ajustes completos, uso de disco, limpieza de temporales, actividad y créditos.
- Instalador NSIS generado, instalado, ejecutado y desinstalado correctamente (ver §8).

### Dependen de terceros (documentadas, no bloquean el resto)
- Credenciales de Google Cloud, Meta y TikTok + revisión de permisos.
- Firma de código del instalador (certificado del propietario).
- Publicación pública en TikTok (auditoría de la app).
- Traducciones a otros idiomas (textos en español, estructura preparada).

---

## 7. Instalación y ejecución

Requisitos: Windows 10/11 x64, Node 20+, Rust stable MSVC, Build Tools de Visual Studio con SDK
de Windows y WebView2.

```powershell
git clone https://github.com/andrespar2/Parcut.git
cd Parcut
npm install
npm run tools:fetch      # FFmpeg GPL compartido + yt-dlp en src-tauri/binaries
npm run tauri:dev        # desarrollo
npm run tauri:build      # instalador NSIS
```

En este equipo se instaló Rust con:

```powershell
rustup-init.exe -y --default-toolchain stable --profile minimal --no-modify-path
```

y se añadió `%USERPROFILE%\.cargo\bin` al `PATH` de la sesión.

---

## 8. Pruebas y resultados

| Prueba | Comando | Resultado |
|---|---|---|
| Typecheck + build frontend | `npm run build` | OK (1.701 módulos, ~353 kB JS / 28 kB CSS) |
| Unitarias frontend | `npm test` | 10/10 OK |
| Unitarias backend | `cargo test --lib` | 36/36 OK, sin avisos de compilación |
| Compilación backend | `cargo check` | OK, 0 avisos |
| Build release + instalador | `npm run tauri:build` | OK: `parcut.exe` 16,4 MB y `ParCut_1.0.0_x64-setup.exe` 80,50 MiB |
| Instalación NSIS silenciosa | `ParCut_1.0.0_x64-setup.exe /S /D=…` | OK; recursos `ffmpeg/` y `yt-dlp/` presentes |
| Ejecución del instalado | exe instalado + CDP | OK; herramientas detectadas en la carpeta de instalación |
| Desinstalación | `uninstall.exe /S` | OK; carpeta de instalación eliminada |
| E2E release (importar → dividir → reproducir → migrar → eliminar) | UI + CDP | OK (75 s, 3 partes de 25 s, corte exacto) |
| Modo sin recodificar | UI/IPC | OK (cortes alineados a keyframes: 25,03 / 25,02 / 10 s en un clip de 60 s) |
| Migración de almacenamiento | UI/IPC + reproducción | OK; rutas reescritas y video reproducible en el nuevo directorio |
| Eliminación del original tras dividir | Ajustes + IPC | OK; `originalPath=null` y solo quedan clips/miniaturas |
| Eliminación completa | UI | OK; 8 archivos, 22,7 MB liberados, `verified=true`, 0 proyectos restantes |
| Descarga de metadatos | `download_metadata` con YouTube | OK; título real y 8 calidades (no se descargó el archivo) |
| Publicación real | requiere credenciales | No verificable sin apps de desarrollador |

Pruebas unitarias relevantes: cobertura temporal exacta de los cortes, fusión del resto mínimo,
límites de título por plataforma, hashtags sin duplicados, rechazo de URLs locales, contención de
rutas con `..`, roundtrip DPAPI y orden de la cola.

---

## 9. Compilación del ejecutable

Ver `docs/BUILD.md`. Salidas verificadas:

- `src-tauri/target/release/parcut.exe` — 16,4 MB.
- `src-tauri/target/release/bundle/nsis/ParCut_1.0.0_x64-setup.exe` — 80,50 MiB.

Recursos empaquetados junto al ejecutable: `ffmpeg/` (ffmpeg.exe, ffprobe.exe, DLLs y
LICENSE.txt) y `yt-dlp/yt-dlp.exe`. Los binarios **no** están en Git (`src-tauri/binaries/`
ignorado); se obtienen con `npm run tools:fetch`.

La instalación silenciosa con `/S /D=<carpeta>` se comprobó en una carpeta temporal; el
ejecutable instalado detectó FFmpeg/FFprobe/yt-dlp y `uninstall.exe /S` eliminó la carpeta.

---

## 10. Licencias

- **Código ParCut**: MIT (`LICENSE`).
- **FFmpeg/FFprobe**: GPLv3, build compartido de BtbN con `libx264`. Se ejecuta como proceso
  separado; se incluye `LICENSE.txt` y se ofrece el código fuente en `docs/BUILD.md`.
- **yt-dlp**: Unlicense.
- **Tauri/React/rusqlite/reqwest/etc.**: MIT/Apache-2.0/BSD.

---

## 11. Errores encontrados y soluciones

| Fecha | Problema | Solución |
|---|---|---|
| 2026-11-13 | `C:\ffmpeg\bin\ffmpeg.exe` del sistema no ejecuta: «requiere elevación». | Se empaqueta FFmpeg propio en `src-tauri/binaries/ffmpeg` y se prioriza en la detección. |
| 2026-11-13 | Rust no estaba instalado. | `rustup-init` con toolchain MSVC y perfil mínimo. |
| 2026-11-13 | `tauri.conf.json` rechazaba `shortcutName` en NSIS. | Se eliminó; Tauri deriva el acceso directo de `productName`. |
| 2026-11-13 | `urlencoding::encode_iter` no existe. | Se construye la query con `urlencoding::encode` por par. |
| 2026-11-13 | Lectura de progreso con `timeout` mal tipada (`Result<Option<_>>` frente a `Result<Result<Option<_>>>`). | Se corrigieron los patrones `Ok(Ok(Some(line)))`, `Ok(Err(_))`, `Ok(Ok(None))`. |
| 2026-11-13 | `fail("texto".into())` no infería el tipo. | Se pasa `&str` directamente a `fail`. |
| 2026-11-13 | `is_within` no resolvía `..` y permitía rutas fuera de la base. | Normalización léxica de componentes + prueba unitaria. |
| 2026-11-13 | La build LGPL de FFmpeg no incluye `libx264` («Encoder not found»). | Se cambió a build GPL compartida y se añadió detección de codificador con alternativas. |
| 2026-11-13 | `cargo check` fallaba con «frontendDist no existe». | Se ejecuta `npm run build` antes de comprobar el backend. |
| 2026-11-13 | `npm` bloqueaba los instaladores de `sharp`/`esbuild`. | `npm install-scripts approve sharp esbuild` + `npm rebuild`. |
| 2026-11-14 | Al volver a dividir se borraba `portada.jpg` porque la limpieza vaciaba toda la carpeta `thumbs`. | La limpieza solo elimina archivos `parte-*`; la portada se conserva. |
| 2026-11-14 | El indicador superior quedaba en «1 en curso» tras dividir. | Se dejó de crear una tarea artificial al recibir `split:done`. |
| 2026-11-14 | La duración elegida volvía a 90 s tras dividir y el panel cambiaba de modo. | Se guarda la duración en el proyecto y los campos solo se reinician al abrir otro proyecto. |
| 2026-11-14 | Cambiar el directorio de almacenamiento dejaba inaccesibles los proyectos anteriores. | Migración real de carpetas + reescritura transaccional de rutas con reversión ante fallos. |
| 2026-11-14 | `tauri build` falló con «Acceso denegado» por el ejecutable abierto. | Cerrar la instancia de ParCut antes de recompilar. |

---

## 12. Convenciones

- Rust: `snake_case`, errores con `AppError`, comandos `#[tauri::command] -> Result<T, AppError>`.
- TypeScript: componentes funcionales, props tipadas, sin `any`; utilidades puras en `lib/`.
- Nombres de archivo de clips: `parte-XXX.mp4`; miniaturas `parte-XXX.jpg`.
- Títulos por defecto: `<NOMBRE> - PARTE <N>`.
- Commits: Conventional Commits (`feat:`, `fix:`, `ui:`, `docs:`, `chore:`); nunca mencionar
  herramientas de asistencia.
- Nunca registrar secretos en logs ni en la base sin cifrar.

---

## 13. Historial de cambios

### 2026-11-13 — Etapa 1: fundamentos
- Inspección del repositorio (solo README, `main` limpio) y del entorno (Node 24, sin Rust,
  FFmpeg del sistema inutilizable).
- Instalación de Rust 1.100 MSVC; descarga de FFmpeg y yt-dlp.
- Creación de `AGENTS.md`, configuración Tauri, Vite/React/TS, iconos de marca, diseño Liquid
  Glass, navegación y estructura modular.
- Archivos: `AGENTS.md`, `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`,
  `.gitignore`, `scripts/*`, `assets/*`, `src-tauri/{Cargo.toml,tauri.conf.json,build.rs,capabilities/*,icons/*}`.

### 2026-11-13 — Etapa 2: gestión de videos
- Backend: `paths`, `models`, `error`, `settings`, `db` (SQLite), `state`.
- Importación local con copia por bloques, FFprobe, portada y eventos de progreso.
- Descarga con yt-dlp: metadatos, selector de calidad validado, progreso y cancelación.
- Reproductor y pantalla de proyecto.
- Archivos: `src-tauri/src/{paths,models,error,settings,db,state,cleanup}.rs`,
  `media/{ffprobe,thumbnail}.rs`, `download/ytdlp.rs`, `projects/mod.rs`,
  `commands/{system,projects}.rs`, `src/pages/{NewProjectPage,ProjectPage}.tsx`, `src/lib/api.ts`.

### 2026-11-13 — Etapa 3: edición
- `media/ffmpeg.rs` (runner con progreso, cancelación y salida oculta),
  `media/split.rs` (plan + copy/precise + miniaturas + persistencia).
- UI de estimación, presets, progreso, cancelación y rejilla de clips; «Mis clips».
- Archivos: `media/{ffmpeg,split,mod}.rs`, `src/components/ClipCard.tsx`,
  `src/pages/{ClipsPage,ProjectPage}.tsx`, `src/lib/format.ts`.

### 2026-11-13 — Etapa 4: redes sociales
- `auth/credentials.rs` (DPAPI), `auth/oauth.rs` (PKCE, loopback, manual, refresh),
  `publish/platforms/{youtube,facebook,tiktok}.rs` (subidas reales), `publish/upload.rs`.
- UI de cuentas, credenciales y flujo manual.
- Archivos: `auth/*`, `publish/platforms/*`, `publish/upload.rs`,
  `commands/accounts.rs`, `src/pages/AccountsPage.tsx`, `src/components/PlatformLogo.tsx`.

### 2026-11-13 — Etapa 5: publicación secuencial
- `publish/title.rs` y `publish/queue.rs`: cola persistente, orden, estados, pausa, reintento,
  cancelación, `resume_data` cifrado, recuperación al arrancar y verificación previa.
- UI de preparación y seguimiento con progreso general/individual.
- Archivos: `publish/{title,queue}.rs`, `commands/publish.rs`, `src/pages/PublishPage.tsx`,
  `src/store/app.ts`.

### 2026-11-13 — Etapa 6: limpieza y configuración
- `cleanup.rs`: borrado completo con verificación, borrado de clips, temporales y estadísticas de
  disco (incluye `GetDiskFreeSpaceExW`).
- Ajustes completos, actividad, herramientas, tema y créditos.
- Archivos: `cleanup.rs`, `commands/system.rs`, `src/pages/SettingsPage.tsx`,
  `src/components/{ui,Toasts,Sidebar,TopBar}.tsx`, `src/styles/globals.css`.

### 2026-11-13 — Etapa 7: finalización
- Corrección de pruebas (selector de calidad, normalización de rutas).
- Cambio a FFmpeg GPL con `libx264` y detección de codificador con alternativas.
- Documentación pública y técnica (`README`, `CHANGELOG`, `LICENSE`, `docs/*`).
- Verificación de build, pruebas, E2E y generación del instalador NSIS.
- Archivos: `README.md`, `CHANGELOG.md`, `LICENSE`, `docs/*`, `scripts/fetch-tools.mjs`,
  `src-tauri/src/{paths,media/split}.rs`, `AGENTS.md`.

### 2026-11-14 — Verificación final y endurecimiento
- E2E automatizado sobre el binario release: importación, división exacta desde la interfaz,
  reproducción, copia, migración de biblioteca, ahorro de espacio y eliminación verificada.
- Corregidos: portada borrada al re-dividir, indicador de tarea persistente, duración elegida
  restablecida y pérdida de proyectos al cambiar de directorio (ahora hay migración transaccional).
- Añadidos ajustes funcionales de autoplay/silencio y borrado opcional del video original.
- Instalación NSIS silenciosa, ejecución del binario instalado y desinstalación comprobadas.
- Capturas finales en `docs/screenshots/` (oscuro y claro).
- Pruebas: 36 backend, 10 frontend, `cargo check` sin avisos y build release correcto.
- Archivos: `AGENTS.md`, `README.md`, `CHANGELOG.md`, `docs/*`, `docs/screenshots/*`,
  `src-tauri/src/{projects,media/split,commands/system}.rs`, `src/pages/*`, `src/components/*`,
  `src/styles/globals.css`.
