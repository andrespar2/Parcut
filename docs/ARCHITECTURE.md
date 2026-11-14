# Arquitectura de ParCut

> Documento técnico. La memoria viva del proyecto es [`../AGENTS.md`](../AGENTS.md).

## 1. Vista general

ParCut sigue una arquitectura de tres capas:

```
┌──────────────────────────────────────────────────────────────┐
│ Interfaz React + TypeScript (WebView2)                       │
│  páginas · componentes · store Zustand · api tipada          │
└───────────────▲──────────────────────────────┬───────────────┘
                │ eventos Tauri                │ invoke(command)
┌───────────────┴──────────────────────────────▼───────────────┐
│ Backend Rust (Tokio)                                          │
│  comandos · proyectos · cola · OAuth · DPAPI · SQLite         │
└───────┬───────────────────┬──────────────────┬───────────────┘
        │ spawn             │ spawn            │ cifra/descifra
┌───────▼────────┐  ┌───────▼────────┐  ┌──────▼──────────────┐
│ FFmpeg/FFprobe │  │ yt-dlp         │  │ DPAPI (Windows)     │
└────────────────┘  └────────────────┘  └─────────────────────┘
```

La interfaz nunca toca archivos ni tokens: todo pasa por comandos tipados. Los procesos externos
son programas separados, invocados con `Command::args` (sin shell), con la consola oculta en
Windows.

## 2. Módulos del backend

| Ruta | Responsabilidad |
|---|---|
| `src/lib.rs` | Construcción de Tauri, plugins, setup, recuperación de cola e IPC. |
| `src/paths.rs` | Rutas base (`%LOCALAPPDATA%\ParCut`), detección de FFmpeg/FFprobe/yt-dlp y del codificador H.264 disponible. |
| `src/db.rs` | Esquema SQLite, migraciones idempotentes y CRUD de proyectos, clips, cuentas, secretos, cola y actividad. |
| `src/settings.rs` | Preferencias no sensibles en JSON, validación y escritura atómica. |
| `src/state.rs` | Estado compartido: DB, settings, herramientas, cancelaciones, OAuth pendiente y runtime de la cola. |
| `src/commands/` | Superficie IPC dividida por dominio (`projects`, `accounts`, `publish`, `system`). |
| `src/projects/` | Creación de proyectos, importación local con progreso, análisis y portada. |
| `src/media/` | Ejecución de FFmpeg con progreso/cancelación, FFprobe, miniaturas y planificación/ejecución de cortes. |
| `src/download/ytdlp.rs` | Metadatos, selector de calidad validado y descarga con progreso. |
| `src/auth/` | PKCE + callback local, canje/renovación de tokens y cifrado DPAPI. |
| `src/publish/` | Límites de texto, cola secuencial y adaptadores de YouTube, Facebook y TikTok. |
| `src/cleanup.rs` | Borrado completo de proyectos, clips, temporales y estadísticas de disco. |
| `src/utils.rs` | Saneado de nombres, validación de URL, contención de rutas, hashtags y utilidades. |

## 3. Persistencia

Base: `%LOCALAPPDATA%\ParCut\parcut.db` (SQLite con WAL, claves foráneas activas).

| Tabla | Contenido |
|---|---|
| `projects` | Metadatos del video, rutas, estado, hashtags y duración por defecto. |
| `clips` | Parte, rango de tiempo, ruta, miniatura, tamaño y estado. |
| `accounts` | Cuenta vinculada + `token_blob` cifrado con DPAPI. Única por `(platform, external_id)`. |
| `secrets` | Client secrets de aplicaciones, cifrados con DPAPI. |
| `publish_queue` | Elementos de la cola, orden, estado, progreso, IDs remotos y `resume_data` cifrado. |
| `activity` | Registro local de eventos sin secretos. |

Los archivos viven en `<storageDir>\projects\<projectId>\`:

```
source/   video original (copia o descarga)
clips/    parte-001.mp4, parte-002.mp4…
thumbs/   portada.jpg, parte-001.jpg…
tmp/      archivos temporales
```

`portada.jpg` es la portada del proyecto y **no** se elimina al volver a dividir; solo se
reemplazan los archivos `parte-*`.

Cambiar el directorio de almacenamiento en Ajustes **migra** los proyectos: se mueven las
carpetas, se reescriben las rutas absolutas de `projects` y `clips` en una transacción y, si algo
falla, se revierten los movimientos. Los videos siguen siendo reproducibles desde el nuevo
directorio.

### 3.1 Ahorro de espacio

`keepSourceAfterSplit=false` elimina el `source/` al terminar una división y deja
`original_path` en `NULL`. Los clips ya generados y sus miniaturas permanecen intactos.

## 4. Flujo de una división

1. `split_plan(projectId, segundos)` calcula los cortes sin tocar el disco.
2. `split_start(projectId, segundos, mode)`:
   - valida rutas y duración;
   - limpia clips previos (archivos + filas);
   - registra una bandera de cancelación por proyecto;
   - ejecuta FFmpeg:
     - `copy`: muxer `segment` con `-c copy` y tiempos basados en keyframes;
     - `precise`: un proceso por parte con `-ss/-t` y codificador H.264 detectado;
   - emite `split:progress` leyendo `out_time_us=` de `-progress pipe:1`;
   - mide cada archivo con FFprobe, crea su miniatura e inserta la fila;
   - guarda la duración elegida en el proyecto para futuras divisiones;
   - si el modo `copy` falla por códecs incompatibles, reintenta en `precise`;
   - ante cancelación o error, borra los archivos parciales, limpia las filas y deja el proyecto
     en estado `ready`.
3. `split:done` o `split:error` cierran la tarea en la UI.

El último clip absorbe el resto; si el resto es menor que 2 s, se fusiona con el anterior para
evitar fragmentos inútiles.

## 5. Cola de publicaciones

`QueueRuntime` mantiene un único trabajador asíncrono. Reglas:

- sólo una publicación activa;
- el siguiente elemento se toma con `status='pending' ORDER BY sort_index ASC`;
- no se marca `published` hasta recibir un identificador válido de la plataforma;
- cualquier error marca el elemento como `error` y pausa la cola;
- al arrancar, los estados `preparing`/`uploading` vuelven a `pending` (recuperación tras cierre);
- `resume_data` se cifra con DPAPI y permite continuar:
  - YouTube: URL de sesión resumable + consulta `Content-Range: bytes */N`;
  - Facebook: `video_id` + comprobación de estado antes de reiniciar;
  - TikTok: `publish_id` + `status/fetch` y bytes ya subidos.

El frontend recibe `queue:changed` (instantánea), `queue:progress` (porcentaje por elemento) y
`queue:finished`.

## 6. Eventos Tauri

| Evento | Carga |
|---|---|
| `toast` | `{ level, message }` |
| `import:progress` | `{ projectId, percent, stage, message }` |
| `download:progress` / `download:done` / `download:error` | progreso y proyecto / mensaje |
| `split:start` / `split:progress` / `split:clip` / `split:done` / `split:error` / `split:notice` | progreso y clips |
| `project:updated` | proyecto completo |
| `clips:changed` | `{ clipId }` |
| `accounts:changed` | `{}` |
| `queue:changed` / `queue:progress` / `queue:finished` | instantánea / item / fin |

## 7. Seguridad

- **Tokens**: JSON cifrado con `CryptProtectData` (DPAPI, usuario actual) en `accounts.token_blob`.
- **Secretos de app**: mismo mecanismo en `secrets.blob`.
- **Resume de subidas**: cifrado antes de guardarse en SQLite.
- **URLs**: se exige http(s), se rechazan hosts privados/localhost y se valida la calidad contra
  un rango numérico antes de construir argumentos.
- **Rutas**: `ensure_within` canonicaliza y verifica el prefijo; los borrados se limitan a
  `projects/<uuid>` y se valida el nombre de carpeta.
- **Procesos**: argumentos separados, `kill_on_drop`, consola oculta y salida de error acotada.
- **CSP**: se deja `null` porque el reproductor usa `asset:`; el contenido remoto se limita a
  miniaturas https y la API no acepta HTML.

## 8. Pruebas

| Prueba | Comando | Resultado |
|---|---|---|
| Typecheck + build frontend | `npm run build` | OK |
| Unitarias frontend | `npm test` | 10/10 |
| Unitarias backend | `cargo test` | 36/36 |
| Compilación | `cargo check` | Sin avisos |
| Release + instalador | `npm run tauri:build` | `parcut.exe` 16,4 MB · instalador 80,50 MiB |
| Instalación / desinstalación | NSIS `/S` y `uninstall.exe /S` | OK |
| E2E release (importar, dividir, reproducir, migrar, eliminar) | UI + CDP | OK |
| Metadatos de YouTube | `download_metadata` | OK (8 calidades) |
| Publicación real | requiere credenciales | Pendiente externo |

## 9. Cómo ampliar

- **Nueva plataforma**: añadir variante en `Platform`, implementar `publish/platforms/<x>.rs`,
  registrar el despacho en `publish/mod.rs` y documentar credenciales/permisos.
- **Nuevo comando**: crear función `#[tauri::command]` en `commands/`, registrarla en `lib.rs` y
  exponer el wrapper tipado en `src/lib/api.ts`.
- **Nuevo evento**: emitir desde Rust y escuchar en `src/App.tsx` o en la página correspondiente;
  documentarlo en la tabla anterior.
