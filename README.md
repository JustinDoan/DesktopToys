# ScreenOverlayPhysics Native

Transparent native Rust desktop overlay with physics-driven screen toys, Direct3D/DirectComposition on Windows, and a WGPU fallback on other platforms.

## Requirements

- Windows
- Rust toolchain

## Run

```powershell
cd native
cargo run -p app
```

## Overlay UI

The overlay UI lives in `control-ui/`. It is a Tauri 2 + Preact/Vite transparent always-on-top webview intended for HUD/widgets that live above the desktop like the 3D objects. The native renderer still owns physics, input passthrough, and 3D rendering. The control UI talks to the renderer over local IPC for spawn variants, runtime settings, pause/reset, tools, and tray-launched window wakeup.

```powershell
cd control-ui
npm install
npm run dev
```

To run the Tauri shell:

```powershell
cd control-ui
npm run tauri:dev
```

## Twitch EventSub

The control app can connect directly to Twitch from the streamer's computer. It receives chat and Bits over an outbound EventSub WebSocket, then forwards compact events to the native renderer over the existing local IPC connection.

The packaged app includes the project's public Twitch Client ID. A Client Secret is not used or shipped. Local development can override the embedded ID before starting the control app:

```powershell
$env:SCREEN_OVERLAY_TWITCH_CLIENT_ID="your-client-id"
cd control-ui
npm run tauri:dev
```

At runtime, `SCREEN_OVERLAY_TWITCH_CLIENT_ID` takes precedence over the embedded default.

Choose **Twitch** in the control pod and press **Connect**. The app opens Twitch's device authorization page and displays the short code in the pod. The access and refresh tokens remain in Windows Credential Manager. The Twitch page provides independent Chat and Bits switches; disabling one removes that EventSub subscription without disconnecting the other.

Twitch connectivity belongs to the Tauri control process. Running only `cargo run -p app` starts the renderer without Twitch; use `npm run tauri:dev` or the packaged application for the complete integration.

## Local case WebSocket

Local services can open a case and wait for the full animation to finish over
`ws://127.0.0.1:47734`. Send one JSON text message:

```json
{"type":"open_case","requestId":"job-123","viewer":"Alice","tier":"covert","reward":"Prize","seed":12345}
```

Only `type` is required. The other fields are optional; `requestId` is echoed
in every response. A successful request first receives:

```json
{"type":"accepted","requestId":"job-123"}
```

After the celebration and outro have completely finished, the same connection
receives:

```json
{"type":"completed","requestId":"job-123","result":{"viewer":"Alice","tierId":"covert","tierName":"Covert","rewardName":"Prize","wheelPrize":null}}
```

One opening may be in flight per connection. After `completed`, the connection
can submit another. Cancellation produces `cancelled`; invalid requests or a
full queue produce `error`. The API has no authentication and only accepts
loopback connections. `SCREEN_OVERLAY_CASE_WS_ADDR` can change the listen
address, but it must still be a loopback address.

## OBS output (Windows)

The Windows renderer can publish its transparent Direct3D 11 backbuffer as a zero-copy Spout2 sender named `ScreenOverlayPhysics`. The desktop DirectComposition overlay remains active at the same time.

1. Install the [Spout2 plugin for OBS Studio](https://github.com/Off-World-Live/obs-spout2-plugin/releases) and restart OBS.
2. In the control pod, switch **OBS On**.
3. In OBS, add a **Spout2 Capture** source and select `ScreenOverlayPhysics`.
4. Set the source's composite mode to **Premultiplied Alpha**.

OBS and ScreenOverlayPhysics must run on the same GPU on multi-GPU systems. Display Capture remains the no-plugin fallback.

## Packaging

The release build packages the native renderer/tray app as a Tauri sidecar. The installed Tauri app launches the renderer, and the renderer tray can bring the control UI window forward.

Build the Windows installer:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

The NSIS installer is written under `control-ui/src-tauri/target/release/bundle/nsis/`.

For one portable runtime `.exe`, the renderer and Tauri shell still need to be merged into one process or one binary must embed/extract the other at startup.

## Controls

- `F1`: toggle debug overlay
- `F2`: spawn the next catalog object
- `F3`: reset scene
- `F4`: open runtime settings
- `F5`: force interactive overlay mode
- `F6`: import a 3D model
- `F7`: spawn a crystal
- `F8`: spawn a DVD logo
- `F9`: spawn a batch of boxes
- `F10`: toggle slingshot game
- `F11`: spawn robot buddy
- `F12`: toggle basketball
- `B`: equip the shatter gun; click to shoot textured screen shards
- `Esc`: quit

## Window terrariums (prototype)

On Windows, hold `Shift` while dragging a physics object over the client area of another application. A cyan guide marks the window that will capture it. Release the object while still holding `Shift` to bind it to that window. Ordinary dragging never captures a window object:

- the object bounces inside the window's client bounds;
- moving the window carries the object with it;
- resizing the window pushes the object back inside;
- minimizing or hiding the window hides and pauses its objects until it returns;
- closing the window releases its objects back to the desktop; and
- dragging the object onto the desktop releases it, while dropping it over another window transfers it.

This first prototype constrains objects to real window geometry but does not yet clip rendering behind overlapping windows or title bars.

## Project map

- `native/app/src/main.rs`: native app shell, hotkeys, tray actions, HUD/game state
- `native/scene_logic/src/lib.rs`: object spawning, drag logic, scene reset flow
- `native/physics_core/src/lib.rs`: physics world integration and Box3D bridge
- `native/renderer/src/lib.rs`: mesh generation and overlay rendering
- `native/native_shell/src/lib.rs`: platform window/input/tray helpers
- `native/core_types/src/lib.rs`: shared object, body, color, and config types
- `Assets/`: runtime and compile-time visual assets used by the native crates
