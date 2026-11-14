# Changelog

Todos los cambios relevantes de ParCut se documentan aquí.
El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y
[Semantic Versioning](https://semver.org/lang/es/).

## [1.2.0] — 2026-11-14

### Añadido
- **Navegador integrado** con motor WebView2 dentro de la ventana de ParCut: pestañas por
  plataforma, barra de direcciones con indicador de seguridad, atrás/adelante/recargar/inicio,
  accesos directos a YouTube Studio, Facebook y TikTok Studio, descargas controladas y botón para
  abrir la página actual en el navegador del sistema.
- **Sesiones persistentes por plataforma** en
  `%LOCALAPPDATA%\ParCut\browser-profiles\{youtube,facebook,tiktok}`. Las cookies y el
  `localStorage` se conservan al cerrar y reabrir ParCut, y cada plataforma mantiene su propio
  perfil aislado.
- Opción **Mantener sesión iniciada**, activada por defecto, con limpieza independiente por
  plataforma: borrar sesión, limpiar caché o eliminar el perfil completo (siempre con
  confirmación).
- **Protección web de ParCut** basada en `adblock-rust` (MPL-2.0): bloqueo de anuncios y
  rastreadores con EasyList, EasyPrivacy y la lista de Peter Lowe, niveles estándar e intensiva,
  contador de elementos bloqueados, actualización de listas y excepciones por sitio.
- **Publicación asistida**: desde Mis clips se seleccionan varios clips, se elige plataforma, se
  editan títulos, descripción y hashtags, y ParCut prepara una cola ordenada. Abre la página
  oficial de subida con la sesión del navegador, muestra los metadatos para copiarlos y espera la
  confirmación del usuario antes de pasar a la siguiente parte. Incluye pausa, reanudación,
  reintento, cancelación y recuperación tras un cierre inesperado.
- Sección de navegador en Configuración y panel propio del navegador: perfiles, espacio en disco,
  listas de filtros, protección por sitio, carpeta de descargas y apertura automática al terminar.
- Documentación `docs/BROWSER.md` y capturas del navegador, la protección y la publicación
  asistida.

### Cambiado
- Las pestañas muestran el título real de la página web y el estado de carga.
- La cola OAuth existente se conserva sin cambios; la publicación asistida usa una tabla y un
  flujo independientes.

### Seguridad
- Validación estricta de direcciones, bloqueo de `javascript:`, `data:`, `file:` y otros esquemas
  peligrosos, preferencia por HTTPS en las plataformas y validación de certificados delegada en
  WebView2.
- Ventanas emergentes restringidas a dominios oficiales de las plataformas y cuentas de Google.
- Descargas con nombre saneado y bloqueo de ejecutables y scripts; perfiles fuera del alcance de
  cualquier borrado de proyectos.
- ParCut no captura contraseñas, códigos de autenticación ni cookies, y no registra datos de
  navegación en los logs.

### Notas
- Google puede rechazar el inicio de sesión dentro de navegadores integrados. En ese caso la
  interfaz ofrece abrir la página en el navegador del sistema; no se intenta evadir la restricción.
- Las sesiones web dependen de las políticas de cada plataforma y pueden caducar o revocarse.

## [1.1.0] — 2026-11-14

### Añadido
- **Recorte por rango antes de dividir**: permite elegir desde y hasta qué punto del video
  original se generan los clips, con campos de horas/minutos/segundos, barra visual del rango y
  botones para marcar el inicio y el fin desde el reproductor.
- Opción «Usar video completo» para restablecer el rango.
- La estimación muestra el rango seleccionado, su duración, los clips resultantes y el espacio
  aproximado solo del fragmento elegido.
- La numeración de las partes empieza en 1 dentro del rango recortado.

### Cambiado
- Los rangos personalizados usan siempre **corte exacto** para respetar al milisegundo el inicio
  y el final. «Sin recodificar» queda disponible para dividir el video completo, donde conserva
  la calidad original y la velocidad.
- La duración elegida se guarda en el proyecto y se conserva al volver a dividir.
- El modo y los campos de duración ya no se reinician tras cada división.

### Corregido
- La portada del proyecto se conserva al volver a dividir.
- El indicador de tareas se limpia correctamente al terminar una división.

## [1.0.0] — 2026-11-13

### Añadido
- Aplicación de escritorio Tauri 2 + React 19 + TypeScript para Windows 10/11 x64.
- Interfaz Liquid Glass con tema oscuro por defecto y modo claro, navegación lateral y estados
  de carga, progreso, error, vacío y confirmación.
- Importación local de video con copia segura al espacio de trabajo.
- Descarga con yt-dlp: metadatos, calidades disponibles, progreso en tiempo real y cancelación.
- Análisis con FFprobe (duración, resolución, fps, códecs, contenedor, tamaño) y miniaturas.
- Reproductor integrado con pantalla completa.
- División automática con estimación previa, duración en horas/minutos/segundos, presets y dos
  modos: sin recodificar y corte exacto (H.264/AAC, CRF 20).
- Progreso real y cancelación segura de FFmpeg, con limpieza de archivos parciales.
- Biblioteca «Mis clips» ordenada por número de parte, con reproducción, ubicación, copia de
  nombre, eliminación y selección para publicar.
- Cuentas OAuth 2.0 con PKCE y callback local para YouTube, Facebook (Páginas) y TikTok.
- Cifrado de tokens y secretos con Windows DPAPI.
- Adaptadores reales de subida:
  - YouTube Data API v3, subida reanudable con consulta de sesión.
  - Facebook Graph API, subida por bloques a Páginas.
  - TikTok Content Posting API, FILE_UPLOAD con seguimiento de estado.
- Cola de publicaciones estrictamente secuencial, persistente en SQLite, con pausa,
  reanudación, reintento, cancelación, progreso general e individual y protección contra
  duplicados.
- Títulos `NOMBRE DEL VIDEO - PARTE N`, hashtags generales e individuales y adaptación a los
  límites de cada plataforma.
- Eliminación completa de proyectos con verificación, informe de espacio liberado y protección
  de rutas externas.
- Configuración: directorio de almacenamiento, calidad, duración por defecto, tema,
  previsualización, privacidad, credenciales, herramientas, uso de disco y limpieza de
  temporales.
- Registro local de actividad sin datos sensibles.
- Instalador NSIS por usuario con iconos propios, accesos directos y desinstalación limpia.
- Cambio de directorio de almacenamiento con migración de proyectos y rutas, sin pérdida de
  archivos.
- Eliminación opcional del video original tras dividir para ahorrar espacio.
- Pruebas unitarias de backend (planificación de cortes, títulos, validación de rutas,
  DPAPI, cola) y frontend (formato, hashtags, URL, tiempos).
- Verificado en el instalador real: ejecución, detección de FFmpeg/FFprobe/yt-dlp y
  desinstalación limpia.

### Notas
- Las credenciales de desarrollador de Google, Meta y TikTok no se distribuyen: deben
  configurarse y, cuando corresponda, pasar la revisión del proveedor.
- La firma de código del instalador requiere un certificado del propietario.
