# Ritmo · Tauri 2.0

App de barra de menú (macOS, Windows, Linux) con agenda, tareas, notas, Pomodoro, Gemini y Google Chat. Contexto completo en `../APP_CONTEXT.md`.

## Estado
La bandeja, el panel y el atajo siguen igual. La agenda, las tareas, las notas, el Pomodoro, Gemini y Chat salen de Rust cuando hay sesión. Sin `google.client.json` la app abre igual y pide ese archivo al iniciar sesión. El detalle está en `DEVELOPMENT.md`.

## Requisitos
- Node 18+ y Rust estable (`curl https://sh.rustup.rs -sSf | sh`).
- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Windows: Microsoft C++ Build Tools y WebView2 (incluido en Windows 11).

## Primer arranque
```bash
cd ritmo-tauri
npm install
npm run icons      # genera icon.icns / icon.ico y demás tamaños desde app-icon.png
npm run dev        # abre Ritmo en la barra de menú
```

## Compilar instalador
```bash
npm run build      # .dmg/.app en macOS, .msi/.exe en Windows
```
Para distribuir al equipo: firma con Developer ID y notariza en macOS; firma el instalador en Windows.

## Actualizar la interfaz
La UI vive en `ui/`. Al cambiar el diseño, copia de nuevo:
- `Menu Bar Productividad v3.dc.html` → `ui/index.html`
- `support.js` → `ui/support.js`
- `assets/` → `ui/assets/`

Después deja `./bridge.js` antes de `./support.js`, y el script de diseño en una sola línea: `const Component = window.RitmoDefine(DCLogic);`. No edites `support.js` a mano. `DEVELOPMENT.md` explica por qué.

## Estructura
```
ritmo-tauri/
  app-icon.png
  ui/index.html             prototipo visual
  ui/bridge.js              vista contra los comandos
  src-tauri/src/            dominio, casos de uso, Google, llavero
  src-tauri/google.client.example.json
```
