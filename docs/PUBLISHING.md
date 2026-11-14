# Publicación en plataformas

ParCut implementa los flujos oficiales de autorización y subida, pero **no incluye credenciales
de terceros**. Cada propietario debe registrar su aplicación y, cuando corresponda, superar la
revisión del proveedor. Sin esa configuración la interfaz muestra «falta configuración» y no
simula conexiones.

## URI de redirección (común)

ParCut usa un servidor local de un solo uso con puerto fijo. Registra exactamente estas URI en
cada panel:

| Plataforma | URI |
|---|---|
| YouTube | `http://127.0.0.1:17421/callback` |
| Facebook | `http://127.0.0.1:17422/callback` |
| TikTok | `http://127.0.0.1:17423/callback` |

Los puertos fijos evitan errores de coincidencia. Si el puerto está ocupado, cierra otras
instancias de ParCut. Como alternativa, la interfaz ofrece **autorización manual**: se pega la URL
de redirección y el backend canjea el código.

Los tokens se cifran con DPAPI y se guardan en la base local. No se solicitan ni almacenan
contraseñas.

---

## YouTube

**API**: YouTube Data API v3 (`videos.insert`, subida resumable) y `channels.list` para mostrar el
canal.

**Scopes**:
- `https://www.googleapis.com/auth/youtube.upload`
- `https://www.googleapis.com/auth/youtube.readonly`

**Configuración**:
1. Crea un proyecto en [Google Cloud Console](https://console.cloud.google.com/).
2. Habilita **YouTube Data API v3**.
3. Configura la pantalla de consentimiento OAuth (tipo externo). En modo prueba añade tu cuenta
   como usuario de prueba.
4. Crea credenciales **OAuth client ID → Aplicación de escritorio**.
5. Copia el client ID y el client secret en la tarjeta de YouTube de ParCut.
6. Pulsa **Conectar cuenta** y autoriza en el navegador.

**Subida**:
- `POST https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status`
- `PUT` del archivo con `Content-Range`.
- Si se interrumpe, ParCut consulta la sesión con `Content-Range: bytes */<total>` y continúa desde
  el byte confirmado; si el video ya existe, recupera su ID y no lo duplica.
- Límites aplicados: título 100 caracteres, descripción 5000, etiquetas ~450 caracteres.
- Privacidad por defecto: **privado** (configurable en Ajustes). Los videos subidos por apps en
  modo prueba pueden quedar restringidos por Google.

**Requisitos externos**: cuota diaria de la API, verificación de la app para uso público y
cumplimiento de las políticas de YouTube.

---

## Facebook (Páginas)

**API**: Graph API v21.0, publicación de video en una Página.

**Scopes**:
- `pages_show_list`
- `pages_read_engagement`
- `pages_manage_posts`

**Configuración**:
1. Crea una app en [Meta for Developers](https://developers.facebook.com/apps/).
2. Añade el producto **Facebook Login** (flujo web) y configura la URI de redirección.
3. Solicita los permisos anteriores; para uso fuera de cuentas de prueba necesitas **App Review**
   y, según el caso, verificación de negocio.
4. Copia el **App ID** y el **App Secret** en ParCut.
5. Indica el **ID de Página** en Ajustes (opcional: si está vacío se usa la primera Página
   administrada).
6. Conecta la cuenta; ParCut obtiene un token de larga duración y el token de la Página.

**Subida**:
- Fase `start` en `graph.facebook.com/v21.0/{page}/videos`.
- Bloques de 4 MB a `graph-video.facebook.com/v21.0/{page}/videos` (`upload_phase=transfer`).
- Fase `finish` con título, descripción y `published=true`.
- Antes de reintentar, ParCut consulta `GET /{video_id}?fields=status`; si ya está `ready`, no
  vuelve a subirlo.
- Límites aplicados: título 255 caracteres, descripción 63 206.

**Requisitos externos**: sólo Páginas (no perfiles personales), permisos aprobados por Meta y
token de Página válido. Los tokens de Página pueden invalidarse si se revoca la app o cambia el
rol del administrador; en ese caso hay que reconectar.

---

## TikTok

**API**: Content Posting API (`/v2/post/publish/video/init/`) con `FILE_UPLOAD` y consulta de
estado (`/v2/post/publish/status/fetch/`).

**Scopes**:
- `user.info.basic`
- `video.publish`
- `video.upload`

**Configuración**:
1. Crea una app en [TikTok for Developers](https://developers.tiktok.com/).
2. Añade **Login Kit** y **Content Posting API**.
3. Registra la URI de redirección `http://127.0.0.1:17423/callback`.
4. Copia el **Client key** y **Client secret** en ParCut.
5. Conecta la cuenta y autoriza los permisos.

**Subida**:
- `POST /v2/post/publish/video/init/` con `post_info` y `source_info` (bloques de hasta 10 MB).
- `PUT` de cada bloque a `upload_url` con `Content-Range`.
- Sondeo de estado hasta `PUBLISH_COMPLETE` (o `FAILED` con motivo).
- Antes de reintentar, ParCut consulta el `publish_id`; si ya se completó, no duplica.
- El texto se limita a 2200 caracteres e incluye título, descripción y hashtags.
- Privacidad: por defecto **`SELF_ONLY`** («Solo yo»), obligatoria para apps sin auditar. Tras la
  auditoría de TikTok puede seleccionarse `PUBLIC_TO_EVERYONE`.

**Requisitos externos**: revisión de la app para publicar públicamente. Los clientes no auditados
solo pueden publicar en el inbox/borradores del usuario. La API no devuelve una URL pública
inmediata; ParCut guarda el `publish_id` como identificador real.

**Importante**: no se usa `PULL_FROM_URL` (requiere verificar el dominio de origen). ParCut sube
el archivo directamente (`FILE_UPLOAD`).

---

## Límites y adaptación de textos

| Campo | YouTube | Facebook | TikTok |
|---|---|---|---|
| Título | 100 | 255 | — (se integra en el pie) |
| Descripción | 5000 | 63 206 | 2200 (total) |
| Etiquetas | ~450 caracteres | Hashtags en el texto | Hashtags en el texto |

El backend trunca y devuelve advertencias; los hashtags se normalizan (sin `#`, sin duplicados,
máximo 30).

El título por defecto es `NOMBRE DEL VIDEO - PARTE N`, donde `N` es el número real del clip.

---

## Diagnóstico de errores frecuentes

| Mensaje | Causa probable | Solución |
|---|---|---|
| «Falta el client secret…» | Credenciales no guardadas | Guarda client ID y secret en la tarjeta. |
| «No hay una cuenta de … conectada» | OAuth no realizado | Conecta la cuenta o revisa la revisión de la app. |
| «redirect_uri no coincide» | URI mal registrada | Copia la URI exacta de ParCut en el panel. |
| «La sesión caducó» | Refresh token revocado | Desconecta y vuelve a conectar. |
| «TikTok rechazó la publicación» | App sin auditar o privacidad no permitida | Usa `SELF_ONLY` o completa la auditoría. |
| «La cuenta no administra ninguna Página» | Perfil personal | Crea una Página y dale acceso. |
| HTTP 401/403 | Token inválido o permiso ausente | Revisa scopes, revisión y reconecta. |

ParCut nunca marca una publicación como exitosa sin un identificador devuelto por la plataforma.
