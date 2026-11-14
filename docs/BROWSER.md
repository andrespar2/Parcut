# Navegador integrado, sesiones y protección web

Documento técnico del navegador que ParCut incorpora desde la versión 1.2.0. Explica cómo se
crean las pestañas, dónde viven los perfiles, cómo funciona la Protección web de ParCut, qué
medidas de seguridad se aplican y cómo se publica de forma asistida.

> Desarrollado por **andrespar2**.

---

## 1. Por qué WebView2 y no un navegador embebido

ParCut es una aplicación Tauri 2. En Windows, Tauri usa **WebView2** (motor Chromium de Microsoft
Edge) como motor de webview. El navegador integrado **no** incrusta el ejecutable de Brave ni
distribuye Chromium propio: crea instancias reales de WebView2 dentro de la ventana principal.

Cada pestaña es una instancia de `Webview` creada con `Window::add_child` (API de Tauri con la
característica `unstable`), posicionada sobre un área reservada de la interfaz React. El
"chrome" (barra de direcciones, pestañas, botones y paneles) sigue siendo HTML de ParCut, por lo
que permanece clicable y con el estilo Liquid Glass.

Consecuencias prácticas:

- Las páginas se renderizan con el motor real de Edge y su validación de certificados.
- El WebView2 Runtime debe estar instalado (Windows 10/11 lo incluye; Tauri lo exige).
- No hay extensiones de Chrome: el bloqueo se implementa con las APIs nativas de red de WebView2.

---

## 2. Perfiles persistentes

`src-tauri/src/browser/profiles.rs` define los tres perfiles:

```
%LOCALAPPDATA%\ParCut\browser-profiles\
  youtube\
  facebook\
  tiktok\
```

- La raíz se obtiene del directorio de datos de la aplicación (`AppPaths.root`), nunca de una
  carpeta temporal.
- Cada perfil se crea con `CreateCoreWebView2EnvironmentWithOptions` usando su propio
  `userDataFolder`. Ese entorno se reutiliza para todas las pestañas de la plataforma, de modo
  que comparten cookies y `localStorage` entre pestañas del mismo perfil.
- Las pestañas de perfiles distintos usan entornos distintos: **las cookies no se comparten**.
- El directorio se reutiliza en cada ejecución. No se genera un perfil nuevo al abrir el
  navegador ni se limpian cookies al cerrar.
- Al terminar la aplicación, Tauri destruye las ventanas y WebView2 cierra sus procesos; los
  perfiles quedan consistentes en disco.

### «Mantener sesión iniciada»

Activada por defecto. Se guarda en `settings.json` (`browserKeepSession`).

- **Activada**: las cookies y el almacenamiento local persisten entre ejecuciones y reinicios.
- **Desactivada**: al abrir el navegador se eliminan los tres perfiles (`browser_purge_sessions`).
  ParCut pide confirmación antes de desactivarla.

### Limpieza por plataforma

| Acción | Comando | Efecto |
|---|---|---|
| Borrar sesión | `browser_clear_data(profile, "session")` | Cookies, `localStorage`, IndexedDB, CacheStorage y service workers. |
| Limpiar caché | `browser_clear_data(profile, "cache")` | Solo caché de disco. La sesión se conserva. |
| Eliminar perfil | `browser_clear_data(profile, "profile")` | Cierra las pestañas del perfil, libera el entorno, borra la carpeta completa con reintentos y vuelve a crear una pestaña limpia. |

La limpieza se ejecuta con `ICoreWebView2Profile2::ClearBrowsingData`, así que la propia plataforma
invalida sus cookies; ParCut no lee ni copia cookies en ningún momento.

> El borrado de un proyecto de video (`project_delete_all`) **nunca** toca `browser-profiles`.
> Son espacios independientes: la base de datos guarda proyectos y colas; los perfiles guardan
> sesiones web.

### Límites reales

Una sesión puede caducar o revocarse por decisión de la plataforma. ParCut conserva el perfil,
pero no puede garantizar que la sesión siga siendo válida indefinidamente.

---

## 3. Protección web de ParCut

Nombre deliberado: **no es Brave Shields** ni incluye todas las protecciones de Brave. Es un
bloqueo de red propio construido con la biblioteca open source `adblock-rust` (la misma que usa
Brave para su motor de filtros), con licencia MPL-2.0.

### Motor y listas

`src-tauri/src/browser/protection.rs`:

- `adblock::Engine` se construye a partir de un `FilterSet` con reglas de red
  (`RuleTypes::NetworkOnly`); las reglas cosméticas se descartan para no alterar el DOM de las
  plataformas.
- Listas:
  - **ParCut Baseline**: lista propia incluida en el binario (`filters/baseline.txt`), con
    dominios publicitarios y de seguimiento muy conocidos.
  - **EasyList** (`https://easylist.to/easylist/easylist.txt`, CC BY-SA 3.0 / GPLv3).
  - **EasyPrivacy** (`https://easylist.to/easylist/easyprivacy.txt`, CC BY-SA 3.0 / GPLv3).
  - **Peter Lowe's Ad and tracking server list** (`https://pgl.yoyo.org/adservers/`, uso libre
    con atribución). Solo en modo intensivo.
- Las listas se descargan en el equipo del usuario y se guardan en
  `%LOCALAPPDATA%\ParCut\web-protection\lists\`. **No se empaquetan en el instalador**, de modo
  que ParCut no redistribuye el trabajo de terceros.
- Se actualizan automáticamente si tienen más de 7 días y manualmente desde la interfaz.
- El motor se serializa con `Engine::serialize()` en un hilo de trabajo y se deserializa en el
  hilo principal, porque `adblock::Engine` y las interfaces COM no son `Send`.

### Bloqueo real en WebView2

Por cada pestaña se registra un filtro global y un manejador de eventos nativo:

```rust
core.AddWebResourceRequestedFilter(HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
core.add_WebResourceRequested(&handler, &mut token);
```

Cuando llega una petición, `inspect_request`:

1. Comprueba que la protección esté activa y que el sitio no esté excluido.
2. Ignora navegaciones de documento (`Sec-Fetch-Dest: document`): así las páginas de las
   plataformas siempre cargan aunque sus anuncios se bloqueen.
3. Ignora URLs de autenticación (`accounts.google.*`, `login.*`, rutas `/login`, `/signin`,
   `/oauth`, `/checkpoint`, `/captcha`, `/challenge`…).
4. Consulta `Engine::check_network_request` con el tipo de recurso deducido de
   `Sec-Fetch-Dest`.
5. Si hay coincidencia, responde con un `403` creado por WebView2 (`CreateWebResourceResponse`)
   y suma un elemento al contador del perfil.

Los contadores se guardan en memoria y se muestran por plataforma en la barra de direcciones y en
el panel de protección. No se registran URLs bloqueadas ni datos de navegación en disco.

### Niveles y excepciones

- **Estándar**: Baseline + EasyList + EasyPrivacy.
- **Intensiva**: añade la lista de Peter Lowe. Puede afectar a contenido incrustado.
- **Desactivar en este sitio**: crea una excepción por perfil y dominio en la tabla
  `browser_sites` (`profile`, `host`, `enabled`). La coincidencia se aplica al dominio y a sus
  subdominios.
- **Desactivar protección**: apaga el bloqueo global; persiste en `settings.json`.

No se modifica el *fingerprinting* ni se inyecta JavaScript en las plataformas. No se alteran
cabeceras de autenticación.

---

## 4. Seguridad del navegador

| Medida | Implementación |
|---|---|
| Validación de URL | `browser::normalize_url`; solo `http`/`https`. Si el texto no parece una URL se busca en Google. |
| Esquemas peligrosos | Se rechazan `javascript:`, `data:`, `file:`, `vbscript:` y `about:` (salvo `about:blank` interno). |
| HTTPS preferente | Los dominios de YouTube, Google, Facebook y TikTok se fuerzan a `https`. |
| Validación de certificados | No se desactiva ni se intercepta TLS; la realiza WebView2. |
| Ventanas emergentes | `on_new_window` solo permite `https` en dominios de las plataformas y cuentas de Google; el resto se bloquea con aviso. |
| Navegación externa | Botón explícito «Abrir en el navegador del sistema»; la URL se valida con `utils::validate_remote_url`. |
| Descargas | Se guardan en `downloads/browser/<perfil>/` (o la carpeta configurada), con nombre saneado y extensión verificada. Ejecutables y scripts (`exe`, `msi`, `bat`, `cmd`, `ps1`, `vbs`, `js`, `jar`, `reg`, `lnk`, `iso`…) se bloquean. |
| Protección de perfiles | Rutas canonicalizadas y comprobadas con `utils::is_within`; los identificadores de perfil se limitan a `youtube`, `facebook` y `tiktok`. |
| Credenciales | ParCut no lee, captura ni registra contraseñas, códigos de autenticación ni cookies. |

### Inicio de sesión en navegadores integrados

Google puede rechazar el inicio de sesión dentro de webviews integrados por su política de
seguridad (protección contra *embedded user agents*). ParCut **no intenta evadirla**:

- Si la plataforma muestra el aviso, la interfaz ofrece **Abrir en el navegador del sistema**.
- En el panel de configuración del navegador se documenta esta limitación.
- Facebook y TikTok normalmente permiten iniciar sesión en WebView2; YouTube funciona en la
  mayoría de instalaciones, pero puede requerir el navegador externo según la cuenta.

---

## 5. Publicación asistida desde «Mis clips»

No usa OAuth, Google Cloud Console, Meta for Developers ni TikTok for Developers. Aprovecha la
sesión web que el usuario ya tiene en el navegador integrado.

1. En **Mis clips** se seleccionan clips de un mismo proyecto y se pulsa
   **Publicar en redes sociales**.
2. Se elige plataforma, se revisan títulos (`NOMBRE - PARTE N`), descripción y hashtags.
3. `assisted_enqueue` ordena por número de parte y guarda la cola en SQLite
   (`assisted_queue`), ignorando duplicados activos del mismo clip y plataforma.
4. `assisted_start` marca la primera parte como `preparing`, abre la página oficial de subida en
   el perfil correspondiente y la deja en `waiting`.
5. La interfaz muestra título, descripción y hashtags, con botones para copiarlos y para abrir la
   ubicación del archivo. El usuario elige el archivo en el formulario de la plataforma y publica.
6. Solo cuando el usuario pulsa **Confirmar publicado** se marca la parte como `published` y se
   prepara la siguiente.
7. **Pausar** detiene el avance; **Cancelar restantes** cancela pendientes; **Marcar problema**
   registra el motivo, pasa la parte a `error` y pausa la cola. **Reintentar** devuelve la parte a
   `pending`.

Estados: `pending`, `preparing`, `waiting`, `published`, `error`, `canceled`.

Reglas explícitas:

- Nunca hay dos publicaciones activas: la cola avanza de una en una y solo tras confirmación.
- No hay scraping, automatización de clics ni evasión de CAPTCHA o verificaciones.
- No se marca como publicada ninguna parte sin confirmación del usuario.
- `recover_assisted_queue` convierte los `preparing` interrumpidos en `waiting` al arrancar, de
  modo que la cola sobrevive a cierres inesperados.
- La cola asistida es independiente de `publish_queue` (OAuth); ambas coexisten.

---

## 6. Interfaz

- Entrada **Navegador** en el menú lateral.
- Pestañas por plataforma con icono, título, indicador de carga y cierre; botón `+` para abrir una
  pestaña nueva en el perfil elegido.
- Barra: atrás, adelante, recargar/detener, inicio, campo de dirección con candado de seguridad,
  contador de bloqueos, botón de protección, abrir en navegador externo y configuración.
- Accesos directos a YouTube Studio, Facebook y TikTok Studio.
- Paneles translúcidos para protección y configuración que ocultan temporalmente el WebView para
  seguir siendo clicables.
- Barra de publicación asistida integrada sobre el área de navegación.

La posición del WebView se sincroniza con el hueco reservado mediante `ResizeObserver` y
`browser_set_bounds`; al cambiar de página o abrir un panel se oculta (`browser_hide`) para no
tapar la interfaz.

---

## 7. Depuración

Con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` se puede inspeccionar el
WebView en `http://127.0.0.1:9222`. Es una ayuda de desarrollo: no debe activarse en producción.

---

## 8. Licencias y atribuciones

| Componente | Licencia | Uso |
|---|---|---|
| `adblock-rust` | MPL-2.0 | Motor de filtros de red. |
| EasyList | CC BY-SA 3.0 / GPLv3 | Lista de filtros descargada en tiempo de ejecución. |
| EasyPrivacy | CC BY-SA 3.0 / GPLv3 | Lista de filtros descargada en tiempo de ejecución. |
| Peter Lowe's list | Uso libre con atribución | Lista opcional del modo intensivo. |
| WebView2 | Distribuido por Microsoft | Motor de navegación del sistema. |

ParCut no se presenta como Brave ni como una implementación completa de Brave Shields.
