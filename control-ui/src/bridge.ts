import { invoke } from "@tauri-apps/api/core";

export type EngineSnapshot = {
  connected: boolean;
  transport: string;
  rendererBackend: string;
  overlayMode: string;
  activePreset: string;
  sceneMode: string;
  settings: RuntimeSettings;
  physicsPaused: boolean;
  shatterGunEquipped: boolean;
  clickThrough: boolean;
  overlayInteractive: boolean;
  fps: number;
  objectCount: number;
  queuedCommands: number;
  lastCommand: string | null;
  commandLog: CommandEntry[];
};

export type RuntimeSettings = {
  gravityY: number;
  throwSensitivity: number;
  maxThrowSpeed: number;
  restitution: number;
  linearDamping: number;
  sleepThreshold: number;
  floorSnapThreshold: number;
  interactionDebounceMs: number;
  startInPassThrough: boolean;
};

export type CommandEntry = {
  id: number;
  label: string;
  payload: string;
};

export type EngineCommandPayload = Record<string, string | number | boolean>;

const previewSnapshot: EngineSnapshot = {
  connected: false,
  transport: "browser preview",
  rendererBackend: "native overlay",
  overlayMode: "pass-through ready",
  activePreset: "Desk orbit",
  sceneMode: "Play",
  settings: {
    gravityY: 1800,
    throwSensitivity: 1.1,
    maxThrowSpeed: 2600,
    restitution: 0.75,
    linearDamping: 0.992,
    sleepThreshold: 24,
    floorSnapThreshold: 3,
    interactionDebounceMs: 80,
    startInPassThrough: true,
  },
  physicsPaused: false,
  shatterGunEquipped: false,
  clickThrough: true,
  overlayInteractive: true,
  fps: 0,
  objectCount: 7,
  queuedCommands: 0,
  lastCommand: null,
  commandLog: [],
};

let previewCommands: CommandEntry[] = [];
let previewPreset = "Desk orbit";
let previewMode = "Arrange";
let previewPaused = false;
let previewShatterGunEquipped = false;
let previewClickThrough = true;
let previewOverlayInteractive = true;
let previewObjectCount = 7;
let previewSettings: RuntimeSettings = previewSnapshot.settings;

function hasTauriRuntime() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

export async function getEngineSnapshot(): Promise<EngineSnapshot> {
  if (!hasTauriRuntime()) {
    return {
      ...previewSnapshot,
      activePreset: previewPreset,
      sceneMode: previewMode,
      settings: previewSettings,
      physicsPaused: previewPaused,
      shatterGunEquipped: previewShatterGunEquipped,
      clickThrough: previewClickThrough,
      overlayInteractive: previewOverlayInteractive,
      objectCount: previewObjectCount,
      queuedCommands: previewCommands.length,
      lastCommand: previewCommands[0]?.label ?? null,
      commandLog: previewCommands,
    };
  }

  return invoke<EngineSnapshot>("engine_snapshot");
}

export async function dispatchEngineCommand(
  command: string,
  payload: EngineCommandPayload = {},
): Promise<EngineSnapshot> {
  if (!hasTauriRuntime()) {
    if (command === "clear_command_log") {
      previewCommands = [];
      return getEngineSnapshot();
    }

    if (command === "pause_physics") {
      previewPaused = true;
    } else if (command === "resume_physics") {
      previewPaused = false;
    } else if (command === "toggle_click_through") {
      previewClickThrough = Boolean(payload.enabled);
    } else if (command === "apply_runtime_settings") {
      previewSettings = settingsFromPayload(payload, previewSettings);
      if (typeof payload.clickThrough === "boolean") {
        previewClickThrough = payload.clickThrough;
      }
    } else if (command === "apply_preset" && typeof payload.preset === "string") {
      previewPreset = payload.preset;
    } else if (command === "set_scene_mode" && typeof payload.mode === "string") {
      previewMode = payload.mode;
    } else if (command === "toggle_shatter_gun") {
      previewShatterGunEquipped = !previewShatterGunEquipped;
    } else if (command === "shatter_screen") {
      previewObjectCount = 118;
      previewShatterGunEquipped = true;
    } else if (command === "spawn_variant" || command.startsWith("spawn_")) {
      previewObjectCount += command === "spawn_stress_batch" ? 25 : 1;
    } else if (command === "reset_scene") {
      previewObjectCount = 7;
      previewShatterGunEquipped = false;
    }

    previewCommands = [
      {
        id: Date.now(),
        label: command,
        payload: JSON.stringify(payload),
      },
      ...previewCommands,
    ].slice(0, 8);

    return getEngineSnapshot();
  }

  return invoke<EngineSnapshot>("dispatch_engine_command", { command, payload });
}

export async function setOverlayInteractive(interactive: boolean): Promise<EngineSnapshot> {
  if (!hasTauriRuntime()) {
    previewOverlayInteractive = interactive;
    previewCommands = [
      {
        id: Date.now(),
        label: "set_overlay_interactive",
        payload: JSON.stringify({ interactive }),
      },
      ...previewCommands,
    ].slice(0, 8);

    return getEngineSnapshot();
  }

  return invoke<EngineSnapshot>("set_overlay_interactive", { interactive });
}

export async function minimizeOverlay(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  await invoke("minimize_overlay");
}

export async function hideOverlay(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  await invoke("hide_overlay");
}

export async function exitOverlay(): Promise<void> {
  await hideOverlay();
}

function settingsFromPayload(payload: EngineCommandPayload, fallback: RuntimeSettings): RuntimeSettings {
  return {
    gravityY: numericPayload(payload.gravityY, fallback.gravityY),
    throwSensitivity: numericPayload(payload.throwSensitivity, fallback.throwSensitivity),
    maxThrowSpeed: numericPayload(payload.maxThrowSpeed, fallback.maxThrowSpeed),
    restitution: numericPayload(payload.restitution, fallback.restitution),
    linearDamping: numericPayload(payload.linearDamping, fallback.linearDamping),
    sleepThreshold: numericPayload(payload.sleepThreshold, fallback.sleepThreshold),
    floorSnapThreshold: numericPayload(payload.floorSnapThreshold, fallback.floorSnapThreshold),
    interactionDebounceMs: numericPayload(payload.interactionDebounceMs, fallback.interactionDebounceMs),
    startInPassThrough:
      typeof payload.startInPassThrough === "boolean" ? payload.startInPassThrough : fallback.startInPassThrough,
  };
}

function numericPayload(value: EngineCommandPayload[string], fallback: number) {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}
