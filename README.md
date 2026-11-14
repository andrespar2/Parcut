# ParCut

**Divide y publica videos en clips, sin complicaciones.**

ParCut es una aplicación de escritorio para Windows 10 y 11 (64 bits) que importa o descarga
videos autorizados, los divide automáticamente en partes de la duración que elijas y las publica
de forma **secuencial** en YouTube, Facebook y TikTok, una parte a la vez y con confirmación real
de cada plataforma.

Desarrollado por **andrespar2**.

![Pantalla de inicio de ParCut](docs/screenshots/home.png)

### Capturas

| Proyecto y división | Clips generados |
|---|---|
| ![Proyecto](docs/screenshots/project.png) | ![Clips](docs/screenshots/project-clips.png) |

| Cola de publicaciones | Cuentas conectadas |
|---|---|
| ![Publicaciones](docs/screenshots/publish.png) | ![Cuentas](docs/screenshots/accounts.png) |

| Configuración (oscuro) | Configuración (claro) |
|---|---|
| ![Configuración](docs/screenshots/settings.png) | ![Configuración clara](docs/screenshots/settings-light.png) |

---

## Funcionalidades

### Videos
- Importación local con copia segura al espacio de trabajo.
- Descarga mediante `yt-dlp` de contenido público y autorizado (YouTube, Facebook y otros
  sitios compatibles), con metadatos, selección de calidad, progreso real y cancelación.
- Análisis técnico con FFprobe: duración, resolución, fps, códecs, contenedor y tamaño.
- Reproductor integrado con controles, volumen y pantalla completa.

### División automática
- Duración por partes en horas, minutos y segundos, con valores predeterminados.
- Estimación previa: número de clips, duración del último fragmento y espacio aproximado.
- Dos modos de corte:
  - **Sin recodificar**: instantáneo y sin pérdida, alineado a keyframes.
  - **Corte exacto**: recodificación H.264/AAC controlada (CRF 20) cuando hace falta precisión.
- Miniaturas automáticas, progreso real, cancelación segura y limpieza de archivos parciales.
- Clips ordenados por número de parte y persistidos aunque cierres la aplicación.

### Publicación
- Cuentas vinculadas con OAuth 2.0 (PKCE + callback local) de YouTube, Facebook y TikTok.
- Tokens cifrados con **Windows DPAPI**; nunca se guardan contraseñas ni secretos en texto plano.
- Cola **estrictamente secuencial**: una publicación activa, espera la confirmación de la
  plataforma antes de continuar y pausa la cola ante cualquier error.
- Estados por clip: pendiente, preparando, subiendo, publicado, error y cancelado.
- Reintentos sin duplicar: se conserva la sesión de subida (YouTube resumable, Facebook por
  bloques, TikTok FILE_UPLOAD) y se consulta el estado antes de reintentar.
- Progreso general y por publicación, pausa, reanudación y cancelación de las restantes.
- Títulos con formato `NOMBRE DEL VIDEO - PARTE N`, hashtags generales y edición individual.

### Organización y limpieza
- Proyectos con video fuente, clips, miniaturas y metadatos propios.
- Cambio de carpeta de almacenamiento con migración real de los proyectos y sus rutas.
- Opción para eliminar el video original después de dividir y ahorrar espacio.
- **Eliminar todo el proyecto** en un solo paso: video, clips, miniaturas, temporales, caché y
  metadatos, con verificación y bloqueo de rutas ajenas al espacio de ParCut.
- Uso de disco, limpieza de temporales y registro local de actividad.
- Tema Liquid Glass oscuro por defecto y modo claro.

---

## Requisitos

- Windows 10/11 de 64 bits (WebView2 incluido o instalado por el instalador).
- Para usar el instalador: no se requieren dependencias adicionales; FFmpeg, FFprobe y yt-dlp
  se incluyen empaquetados.
- Para desarrollo: Node.js 20+, Rust stable (MSVC), Microsoft C++ Build Tools y WebView2.

---

## Instalación

### Instalador (usuario final)

1. Descarga `ParCut_1.0.0_x64-setup.exe` desde la sección de releases.
2. Ejecuta el instalador (instalación por usuario, sin privilegios de administrador).
3. Abre ParCut desde el menú Inicio o el acceso directo del escritorio.

El paquete incluye FFmpeg, FFprobe y yt-dlp; no hace falta instalar nada más. La instalación,
la ejecución del binario instalado y la desinstalación se verificaron en Windows de 64 bits.

La desinstalación elimina la aplicación. Los proyectos y la base de datos se guardan en
`%LOCALAPPDATA%\ParCut` y pueden eliminarse por separado si lo deseas.

### Desde el código

```powershell
git clone https://github.com/andrespar2/Parcut.git
cd Parcut
npm install
npm run tools:fetch     # descarga FFmpeg y yt-dlp a src-tauri/binaries
npm run tauri:dev       # desarrollo con recarga en caliente
npm run tauri:build     # instalador NSIS en src-tauri/target/release/bundle/nsis
```

Consulta [`docs/BUILD.md`](docs/BUILD.md) para el detalle del empaquetado y las licencias.

---

## Cómo se usa

1. **Nuevo proyecto**: pega un enlace autorizado o importa un archivo local.
2. **Divide**: elige la duración de cada parte (por ejemplo, 1 min 30 s) y pulsa
   «Crear N clips». El último clip conserva el tiempo restante.
3. **Revisa**: en el proyecto o en **Mis clips** puedes reproducir, abrir la ubicación, copiar el
   nombre o eliminar cada parte.
4. **Conecta cuentas**: configura tus credenciales de desarrollador y autoriza las cuentas en
   **Cuentas conectadas**.
5. **Publica**: selecciona clips, plataforma y cuenta, revisa títulos y hashtags, añádelos a la
   cola y pulsa **Iniciar cola**. ParCut publica la parte 1, espera la confirmación, continúa con
   la parte 2 y así sucesivamente.
6. **Limpia**: «Eliminar todo» borra el video, los clips, las miniaturas y los metadatos del
   proyecto. También puedes mover la biblioteca a otra carpeta desde Ajustes: los proyectos se
   migran y las rutas se actualizan automáticamente.

### Credenciales de plataformas

ParCut implementa el flujo completo, pero **no incluye credenciales de terceros**. Cada
plataforma exige registrar una aplicación propia:

| Plataforma | Requisito principal |
|---|---|
| YouTube | Proyecto en Google Cloud con YouTube Data API v3 y credenciales de escritorio. |
| Facebook | App de Meta con Facebook Login y permisos de Página (`pages_manage_posts`). |
| TikTok | App con Content Posting API; sin auditar, solo permite publicar como «Solo yo». |

Los pasos detallados, las URI de redirección y los permisos están en
[`docs/PUBLISHING.md`](docs/PUBLISHING.md). Si una plataforma no está configurada, ParCut lo
indica y no simula la conexión.

---

## Seguridad y privacidad

- Sin telemetría ni recopilación de datos personales.
- Los tokens OAuth y los secretos de aplicación se cifran con DPAPI (ámbito del usuario de
  Windows) y se guardan en la base local.
- Los procesos externos se ejecutan con argumentos separados y validados; nunca mediante una
  shell concatenada.
- Las URL de descarga se validan (solo http/https, sin redes locales).
- Las rutas de borrado se canonicalizan y deben estar dentro del espacio de trabajo de ParCut.
- Los registros de actividad no incluyen tokens, contraseñas ni códigos OAuth.

---

## Aviso legal

Usa ParCut únicamente con videos propios, con licencia o con autorización del titular. ParCut no
evade DRM, protecciones de acceso, restricciones de autenticación ni controles de pago. Respeta
los términos de uso y los derechos de autor de cada plataforma. Si un sitio no permite la
descarga, importa el archivo desde tu equipo.

FFmpeg se distribuye como programa independiente bajo GPLv3 (build con `libx264`), con su texto
de licencia y enlace al código fuente. yt-dlp se distribuye bajo Unlicense. El código de ParCut
usa licencia MIT. Consulta [`docs/BUILD.md`](docs/BUILD.md#licencias-de-terceros).

---

## Estructura del proyecto

```
src/            Interfaz React + TypeScript (páginas, componentes, store, API tipada)
src-tauri/      Backend Rust (comandos, SQLite, FFmpeg, OAuth, cola de publicación)
docs/           Arquitectura, publicación por plataforma y compilación
scripts/        Descarga de herramientas y generación de iconos
```

La memoria técnica completa está en [`AGENTS.md`](AGENTS.md) y el historial en
[`CHANGELOG.md`](CHANGELOG.md).

---

## Créditos

**Desarrollado por: andrespar2**

Licencia MIT para el código de ParCut. Terceros: FFmpeg (GPLv3), yt-dlp (Unlicense),
Tauri, React y demás dependencias con sus licencias correspondientes.
