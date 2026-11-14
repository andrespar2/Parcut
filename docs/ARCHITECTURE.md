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
| `src/commands/` | Superficie IPC dividida por dominio (`projects`, `accounts`, `publish`, `browser`, `assisted`, `system`). |
| `src/projects/` | Creación de proyectos, importación local con progreso, análisis y portada. |
| `src/media/` | Ejecución de FFmpeg con progreso/cancelación, FFprobe, miniaturas y planificación/ejecución de cortes. |
| `src/download/ytdlp.rs` | Metadatos, selector de calidad validado y descarga con progreso. |
| `src/auth/` | PKCE + callback local, canje/renovación de tokens y cifrado DPAPI. |
| `src/publish/` | Límites de texto, cola secuencial, adaptadores de YouTube, Facebook y TikTok y cola asistida (`assisted.rs`). |
| `src/browser/` | Navegador WebView2: perfiles persistentes, pestañas, descargas, seguridad de URL y Protección web (`protection.rs`). |
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
| `assisted_queue` | Cola de publicación asistida por navegador: parte, plataforma, títulos, estado y error. |
| `browser_sites` | Excepciones de la Protección web por perfil y dominio. |
| `activity` | Registro local de eventos sin secretos. |

Las sesiones del navegador **no** se guardan en SQLite: viven en los perfiles de WebView2
(`%LOCALAPPDATA%\ParCut\browser-profiles\{youtube,facebook,tiktok}`) y se administran con las APIs
de WebView2. Ningún comando lee, exporta ni registra cookies.

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

1. `split_plan(projectId, segundos, inicio, fin)` calcula los cortes sin tocar el disco.
2. `split_start(projectId, segundos, mode, inicio, fin)`:
   - valida rutas, duración y rango (mínimo 2 s, fin posterior al inicio);
   - si el rango no es el video completo, exige `precise` (los keyframes impiden respetar
     un inicio/fin arbitrario con `copy`);
   - limpia clips previos (archivos + filas);
   - registra una bandera de cancelación por proyecto;
   - ejecuta FFmpeg:
     - `copy`: muxer `segment` con `-c copy` para el video completo;
     - `precise`: un proceso por parte con `-ss/-t` absolutos y el codificador H.264 detectado;
   - emite `split:progress` leyendo `out_time_us=` de `-progress pipe:1`;
   - mide cada archivo con FFprobe, crea su miniatura e inserta la fila;
   - guarda la duración elegida en el proyecto para futuras divisiones;
   - si el modo `copy` falla por códecs incompatibles, reintenta en `precise`;
   - ante cancelación o error, borra los archivos parciales, limpia las filas y deja el proyecto
     en estado `ready`.
3. `split:done` o `split:error` cierran la tarea en la UI.

El último clip absorbe el resto del rango; si el resto es menor que 2 s, se fusiona con el
anterior para evitar fragmentos inútiles. La numeración de partes empieza en 1 dentro del
fragmento seleccionado y el tamaño estimado se calcula proporcionalmente al rango.

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

## 5.1 Cola de publicación asistida

`src/publish/assisted.rs` gestiona una cola **independiente** de la anterior, pensada para el
navegador integrado y sin credenciales de desarrollador:

- tabla `assisted_queue`; orden por `part_index` y `sort_index`;
- solo un elemento puede estar en `preparing`/`waiting`; el siguiente no se abre hasta que el
  usuario confirma el anterior (`assisted_confirm`);
- `waiting` significa que la página oficial de subida ya está abierta y ParCut espera al usuario;
- `assisted_fail` registra el motivo, deja el elemento en `error` y pausa la cola;
- `assisted_retry` devuelve un elemento con error o cancelado a `pending`;
- al arrancar, `recover_assisted_queue` convierte los `preparing` interrumpidos en `waiting` y la
  cola queda en pausa hasta que el usuario decida continuar;
- el avance solo lo provoca una confirmación explícita: no hay scraping ni automatización.

## 5.2 Navegador WebView2

`src/browser/` crea pestañas reales con `Window::add_child` (característica `unstable` de Tauri).
El estado no `Send` (interfaces COM, entornos por perfil y el motor `adblock::Engine`) vive en
`thread_local` del hilo principal y las operaciones se serializan con
`AppHandle::run_on_main_thread`.

- `profiles.rs`: raíz `browser-profiles`, creación/borrado con reintentos, tamaños y descargas.
- `runtime.rs`: entornos WebView2 por perfil (`CreateCoreWebView2EnvironmentWithOptions`),
  pestañas, navegación, límites, títulos reales, descargas y limpieza de datos
  (`ICoreWebView2Profile2::ClearBrowsingData`).
- `protection.rs`: motor `adblock-rust`, listas, bloqueo con `WebResourceRequested` (respuesta 403),
  contadores y excepciones por sitio.
- `mod.rs`: instantánea para la interfaz, normalización de URL y lista blanca de ventanas
  emergentes.

La interfaz reserva un área y envía sus límites con `browser_set_bounds`; al abrir un panel o
cambiar de página se oculta el WebView para que el HTML siga siendo clicable.

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
| `browser:changed` / `browser:notice` / `browser:protection` / `browser:download` | refresco del navegador, avisos, estado de protección y descargas |
| `assisted:changed` | instantánea de la cola asistida |

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
- **Navegador**: solo http(s), sin `javascript:`/`data:`/`file:`, HTTPS forzado en plataformas,
  ventanas emergentes limitadas a dominios oficiales, descargas peligrosas bloqueadas y perfiles
  fuera del alcance de los borrados de proyectos. Ver [`BROWSER.md`](BROWSER.md).

## 8. Pruebas

| Prueba | Comando | Resultado |
|---|---|---|
| Typecheck + build frontend | `npm run build` | OK |
| Unitarias frontend | `npm test` | 10/10 |
| Unitarias backend | `cargo test` | 53/53 |
| Compilación | `cargo check` | Sin avisos |
| Release + instalador | `npm run tauri:build` | `parcut.exe` 19,3 MB · instalador 1.2.0 (81,4 MiB) |
| Instalación / desinstalación | NSIS `/S` y `uninstall.exe /S` | OK |
| E2E release (importar, dividir, recortar, reproducir, migrar, eliminar) | UI + CDP | OK |
| Metadatos de YouTube | `download_metadata` | OK (8 calidades) |
| Navegador, sesiones y protección | UI + CDP | 35 comprobaciones OK (ver [`BROWSER.md`](BROWSER.md)) |
| Persistencia tras reiniciar | UI + CDP | 15 comprobaciones OK |
| Publicación real por API | requiere credenciales | Pendiente externo |

## 9. Cómo ampliar

- **Nueva plataforma**: añadir variante en `Platform`, implementar `publish/platforms/<x>.rs`,
  registrar el despacho en `publish/mod.rs` y documentar credenciales/permisos. Para el navegador,
  añadir el perfil a `browser::profiles::PROFILES` y sus URL base.
- **Nuevo comando**: crear función `#[tauri::command]` en `commands/`, registrarla en `lib.rs` y
  exponer el wrapper tipado en `src/lib/api.ts`.
- **Nuevo evento**: emitir desde Rust y escuchar en `src/App.tsx` o en la página correspondiente;
  documentarlo en la tabla anterior.
