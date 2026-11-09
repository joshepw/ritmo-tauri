# Desarrollo

La vista sigue siendo el HTML de diseño. El día, las tareas, las notas, el Pomodoro que sobrevive al reinicio, Gemini y Chat viven en Rust. `ui/bridge.js` es el único archivo de interfaz escrito a mano: pinta los datos que llegan y delega cada clic. `ui/support.js` no se edita.

## Por qué está separado así

`ui/index.html` se vuelve a copiar cuando cambia el diseño. Si la agenda o la sesión vivieran ahí, el siguiente prototipo las borraría. Por eso el script del diseño queda en una línea:

```html
<script src="./bridge.js"></script>
<script src="./support.js"></script>
```

y, al final del documento:

```html
<script type="text/x-dc" data-dc-script>
const Component = window.RitmoDefine(DCLogic);
</script>
```

`bridge.js` tiene que cargarse antes que `support.js`. Después de pegar un prototipo nuevo, restaura esas dos cosas y no toques el runtime generado.

Los identificadores del código están en inglés. El texto que ve la persona está en español.

## Capas

`lib.rs` solo compone: plugins, bandeja, atajo y el registro de comandos. Cada comando traduce un caso de uso y un error. El caso de uso habla con un puerto (`Calendar`, `TaskList`, `Notes`, `Chat`, `Assistant`, `Account`, `Clock`). Google, el llavero, el store y el reloj del sistema son adaptadores. Los tests cubren el armado del día, los filtros, el parser de enlaces y el Pomodoro con adaptadores falsos. La vista no tiene tests de Rust.

```
ui/index.html  →  ui/bridge.js  →  commands  →  application  →  domain ports
                                                              →  Google, llavero, store
```

El reloj de la barra lo actualiza Rust cada segundo, también con el panel cerrado. La UI escucha `ritmo-tick` y `ritmo-snapshot`.

## Entrar con Google

Hace falta un cliente OAuth de tipo Desktop en Google Cloud. Si la cuenta es de Workspace, el consentimiento puede ser interno. El alcance base es `calendar.readonly` más openid, email y profile. Tasks y Chat se piden después, la primera vez que se enciende ese servicio.

Copia `src-tauri/google.client.example.json` a `src-tauri/google.client.json` (ese archivo no se versiona) y pon el client id. El client secret es opcional en un cliente de escritorio; si Google lo entrega, va en el mismo JSON. También valen las variables `RITMO_GOOGLE_CLIENT_ID` y `RITMO_GOOGLE_CLIENT_SECRET`, o el mismo JSON en el directorio de configuración de la app.

El id de cliente no es un secreto. El refresh token sí: queda en el llavero del sistema, servicio `app.ritmo.desktop`, cuenta `google-refresh`. No entra al binario ni al store. El store (`ritmo.json`) guarda preferencias, la sesión del Pomodoro, notas, la cuenta sin tokens, los `syncToken` y la última agenda.

La sesión usa PKCE y un redirect a `127.0.0.1`. Ritmo abre el navegador y espera el código. Una sola cuenta: «Agregar otra cuenta» lo dice y no simula una segunda sesión.

## Qué hace cada servicio

Calendar arma Hoy, Próximos, la tarjeta siguiente y el texto de la bandeja. La cuenta regresiva sale de `starts_at`. Ocultar un calendario, el rango de 3/7/14 días, las rechazadas y «buscar en descripción» filtran datos reales. Cada tipo de enlace (Zoom, Teams, Webex, Meet, Hangouts y el resto que reconoce el detector) elige por separado app nativa, el navegador del sistema o un navegador concreto. En macOS el navegador concreto se abre con `open -a`. Teléfono y salas no usan esa lista.

Google Tasks lista, crea y completa en la lista `@default`. «Atrasada» se calcula con la fecha de vencimiento. Apagar el servicio deja de llamarlo; Google no permite retirar un solo alcance sin cerrar la sesión.

El Pomodoro persiste duraciones, sesión, pomodoros por tarea, pista y volumen. El descanso largo entra después de 4 enfoques. «Abrir al iniciar sesión» usa el plugin de autostart. El aviso (1, 5 o 10 min) y «abrir el enlace al empezar» salen del mismo reloj que la cuenta regresiva. Si empieza una reunión y el enfoque sigue corriendo, se pausa una vez por ese evento.

Las notas son locales. Crear, listar y detectar enlaces usan la misma regla que el calendario.

Gemini lee la agenda, las tareas y, si la preferencia lo permite, las notas ya cargadas. La clave va al llavero (`gemini-api-key`), nunca al binario. Sin clave, el resumen se arma en local con esos datos. El correo no tiene puerto, así que esa fila no pide permiso ni inventa asuntos.

Chat lista mensajes directos y espacios, abre la conversación, envía y marca leído. «Abrir en Google Chat» abre el hilo en el sistema. Si el servicio está apagado, el panel no muestra conversaciones de ejemplo.

## Avisos en macOS

Los avisos usan `UNUserNotificationCenter` (crate `mac-usernotifications`). El plugin de notificaciones de Tauri solo se usa en Windows y Linux: en macOS dice que el permiso está concedido sin preguntar.

macOS solo acepta la petición si el bundle está firmado como `app.ritmo.desktop`. Por eso `tauri.conf.json` firma ad hoc (`signingIdentity: "-"`). Para firmar con el certificado de desarrollo, exporta `APPLE_SIGNING_IDENTITY="Apple Development: …"` antes de `npm run build`. Con la firma que pone el linker, macOS rechaza la petición sin mostrarla y Ritmo no aparece en Ajustes → Notificaciones.

El permiso se pide en el paso «Permisos de macOS» de la bienvenida (`ui/welcome.html`, comando `request_notifications`) o con «Probar aviso». Ritmo no lo pide solo al arrancar. La bienvenida se abre mientras `setupComplete` sea falso en `ritmo.json`.

Con `tauri dev` no hay avisos: el binario no vive dentro de un `.app`.

## Actualizaciones

La app instalada consulta `latest.json` de la release pública en GitHub. Preferencias → Buscar comprueba la versión; Instalar descarga el paquete, verifica la firma y reinicia. Al abrir el panel, la misma consulta corre en silencio y solo avisa si hay una versión nueva.

La clave pública vive en `tauri.conf.json`. La privada está en `~/.tauri/ritmo.key` y la contraseña en `~/.tauri/ritmo.password`. No se versionan. En GitHub son los secrets `TAURI_SIGNING_PRIVATE_KEY` y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Si se pierde la privada, las instalaciones actuales no podrán verificar la siguiente actualización.

Para publicar: sube la versión en `tauri.conf.json`, `Cargo.toml` y `package.json`, y crea el tag `vX.Y.Z`. El workflow `.github/workflows/release.yml` construye macOS arm64 e Intel y sube la release con `latest.json`.

## Qué no entra

El sonido del Pomodoro, la respiración y el shell de la bandeja se quedan en la vista y en `shell.rs`. No hay backend. No hay Vite.
