import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
  obsOutputEnabled: boolean;
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

export type DisplayInfo = {
  id: string;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
  primary: boolean;
};

export type TwitchStatus =
  | "notConfigured"
  | "disconnected"
  | "authorizing"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

export type TwitchSnapshot = {
  status: TwitchStatus;
  clientConfigured: boolean;
  broadcasterDisplayName: string | null;
  userCode: string | null;
  verificationUri: string | null;
  chatEnabled: boolean;
  bitsEnabled: boolean;
  lastError: string | null;
  chatMessagesReceived: number;
  bitsEventsReceived: number;
};

export type CaseTier = {
  id: string;
  name: string;
  odds: number;
  /// Hex colour from the case definition, used to tint the tier's button.
  color: string;
  celebration: string;
  rewards: string[];
};

export type CaseActive = {
  viewer: string;
  phase: string;
  progress: number;
  tierName: string;
  tierColor: string;
  rewardName: string;
  celebration: string;
  wheelPrize: string | null;
};

export type CaseStatus = {
  configLabel: string;
  configError: string | null;
  caseName: string;
  currencyName: string;
  chatCommand: string;
  rollCost: number;
  wheelEnabled: boolean;
  queued: number;
  opened: number;
  refused: number;
  tiers: CaseTier[];
  wheelPrizes: string[];
  active: CaseActive | null;
};

export type RoomStatus =
  | "idle"
  | "creating"
  | "joining"
  | "syncing"
  | "live"
  | "degraded"
  | "reconnecting"
  | "error";

export type RoomRole = "host" | "guest";

export type RoomFeatureFilters = {
  objects: boolean;
  twitch: boolean;
  worldEffects: boolean;
  gamesAndPortals: boolean;
  importedAssets: boolean;
};

export type RoomParticipant = {
  id: string;
  displayName: string;
  role: RoomRole;
  canInteract: boolean;
  status: "live" | "reconnecting" | "offline";
  latencyMs: number | null;
};

export type RoomSnapshot = {
  status: RoomStatus;
  role: RoomRole | null;
  roomId: string | null;
  inviteUrl: string | null;
  localDisplayName: string;
  targetDisplayId: string | null;
  allowGuestInteraction: boolean;
  featureFilters: RoomFeatureFilters;
  participants: RoomParticipant[];
  relayRegion: string | null;
  latencyMs: number | null;
  lastPacketAgeMs: number | null;
  retryInMs: number | null;
  lastError: string | null;
  localSimulation: boolean;
};

export type HostRoomOptions = {
  displayName: string;
  targetDisplayId: string | null;
  allowGuestInteraction: boolean;
  featureFilters: RoomFeatureFilters;
};

export type JoinRoomOptions = {
  inviteUrl: string;
  displayName: string;
  targetDisplayId: string | null;
};

export const defaultRoomFeatureFilters: RoomFeatureFilters = {
  objects: true,
  twitch: true,
  worldEffects: true,
  gamesAndPortals: true,
  importedAssets: false,
};

export const defaultRoomSnapshot: RoomSnapshot = {
  status: "idle",
  role: null,
  roomId: null,
  inviteUrl: null,
  localDisplayName: "Desktop friend",
  targetDisplayId: null,
  allowGuestInteraction: true,
  featureFilters: defaultRoomFeatureFilters,
  participants: [],
  relayRegion: null,
  latencyMs: null,
  lastPacketAgeMs: null,
  retryInMs: null,
  lastError: null,
  localSimulation: true,
};

export function notifyFrontendReady(): Promise<void> {
  if (!hasTauriRuntime()) {
    return Promise.resolve();
  }
  return invoke<void>("frontend_ready");
}

const previewDisplays: DisplayInfo[] = [
  { id: "0:0:1920:1080", label: "Display 1", x: 0, y: 0, width: 1920, height: 1080, primary: true },
  { id: "1920:0:1920:1080", label: "Display 2", x: 1920, y: 0, width: 1920, height: 1080, primary: false },
];

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
  obsOutputEnabled: false,
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
let previewObsOutputEnabled = false;
let previewObjectCount = 7;
let previewSettings: RuntimeSettings = previewSnapshot.settings;
let previewTwitch: TwitchSnapshot = {
  status: "disconnected",
  clientConfigured: true,
  broadcasterDisplayName: null,
  userCode: null,
  verificationUri: null,
  chatEnabled: true,
  bitsEnabled: true,
  lastError: null,
  chatMessagesReceived: 0,
  bitsEventsReceived: 0,
};

/// Mirrors the built-in case definition so the page is populated in a browser
/// preview, where there is no engine to poll.
const previewCaseStatus: CaseStatus = {
  configLabel: "Assets/case/rewards.json",
  configError: null,
  caseName: "VIEWER REWARD CASE",
  currencyName: "channel points",
  chatCommand: "!roll",
  rollCost: 1000,
  wheelEnabled: true,
  queued: 0,
  opened: 0,
  refused: 0,
  tiers: [
    { id: "mil_spec", name: "Mil-Spec", odds: 79.92, color: "#4b69ff", celebration: "clean", rewards: ["25 Tokens", "50 Tokens", "Free Spin", "Reroll", "Shoutout"] },
    { id: "restricted", name: "Restricted", odds: 15.98, color: "#8847ff", celebration: "plume", rewards: ["250 Tokens", "VIP Time", "Song Request", "Loadout Pick"] },
    { id: "classified", name: "Classified", odds: 3.2, color: "#d32ce6", celebration: "streamers", rewards: ["Modifier", "Challenge", "Name A Thing"] },
    { id: "covert", name: "Covert", odds: 0.64, color: "#eb4b4b", celebration: "shatter", rewards: ["Play With Streamer", "Scene Takeover"] },
    { id: "rare_special", name: "Rare Special", odds: 0.26, color: "#ffd700", celebration: "jackpot", rewards: ["Legendary Drop"] },
  ],
  wheelPrizes: ["Golden VIP Day", "1000 Tokens", "Play With Streamer", "Double Legendary", "Stream Takeover"],
  active: null,
};

const roomPreviewStorageKey = "overlay.previewRoom";
const roomPreviewChannelName = "screen-overlay-room-preview";
const previewRoomListeners = new Set<(snapshot: RoomSnapshot) => void>();
let previewRoom = loadPreviewRoom();
const previewRoomChannel = typeof BroadcastChannel === "undefined"
  ? null
  : new BroadcastChannel(roomPreviewChannelName);

if (previewRoomChannel) {
  previewRoomChannel.onmessage = (event: MessageEvent<RoomSnapshot>) => {
    previewRoom = cloneRoomSnapshot(event.data);
    notifyPreviewRoomListeners();
  };
}

if (typeof window !== "undefined") {
  window.addEventListener("storage", (event) => {
    if (event.key !== roomPreviewStorageKey || !event.newValue) {
      return;
    }
    try {
      previewRoom = normalizeRoomSnapshot(JSON.parse(event.newValue) as Partial<RoomSnapshot>);
      notifyPreviewRoomListeners();
    } catch {
      // Ignore malformed preview state written by older development builds.
    }
  });
}

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
      obsOutputEnabled: previewObsOutputEnabled,
      objectCount: previewObjectCount,
      queuedCommands: previewCommands.length,
      lastCommand: previewCommands[0]?.label ?? null,
      commandLog: previewCommands,
    };
  }

  return invoke<EngineSnapshot>("engine_snapshot");
}

/// Null when the engine is not reachable, which the page reads as offline.
export async function getCaseStatus(): Promise<CaseStatus | null> {
  if (!hasTauriRuntime()) {
    return previewCaseStatus;
  }

  try {
    return await invoke<CaseStatus | null>("get_case_status");
  } catch {
    return null;
  }
}

export async function getDisplayLayout(): Promise<DisplayInfo[]> {
  if (!hasTauriRuntime()) {
    return previewDisplays;
  }
  return invoke<DisplayInfo[]>("display_layout");
}

export async function getTwitchSnapshot(): Promise<TwitchSnapshot> {
  if (!hasTauriRuntime()) {
    return { ...previewTwitch };
  }

  return invoke<TwitchSnapshot>("twitch_snapshot");
}

export async function connectTwitch(): Promise<TwitchSnapshot> {
  if (!hasTauriRuntime()) {
    previewTwitch = {
      ...previewTwitch,
      status: "connected",
      broadcasterDisplayName: "PreviewStreamer",
      userCode: null,
      verificationUri: null,
      lastError: null,
    };
    return { ...previewTwitch };
  }

  return invoke<TwitchSnapshot>("twitch_connect");
}

export async function disconnectTwitch(): Promise<TwitchSnapshot> {
  if (!hasTauriRuntime()) {
    previewTwitch = {
      ...previewTwitch,
      status: "disconnected",
      broadcasterDisplayName: null,
      userCode: null,
      verificationUri: null,
      lastError: null,
    };
    return { ...previewTwitch };
  }

  return invoke<TwitchSnapshot>("twitch_disconnect");
}

export async function setTwitchFeatures(
  chatEnabled: boolean,
  bitsEnabled: boolean,
): Promise<TwitchSnapshot> {
  if (!hasTauriRuntime()) {
    previewTwitch = { ...previewTwitch, chatEnabled, bitsEnabled };
    return { ...previewTwitch };
  }

  return invoke<TwitchSnapshot>("twitch_set_features", { chatEnabled, bitsEnabled });
}

export async function getRoomSnapshot(): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    previewRoom = loadPreviewRoom();
    return cloneRoomSnapshot(previewRoom);
  }
  return invoke<RoomSnapshot>("room_snapshot");
}

export async function subscribeRoomSnapshot(
  callback: (snapshot: RoomSnapshot) => void,
): Promise<UnlistenFn> {
  if (!hasTauriRuntime()) {
    previewRoomListeners.add(callback);
    return () => previewRoomListeners.delete(callback);
  }
  return listen<RoomSnapshot>("room://snapshot", (event) => callback(event.payload));
}

export async function hostRoom(options: HostRoomOptions): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    const roomId = generatedPreviewId("room");
    const displayName = normalizedPreviewName(options.displayName);
    return commitPreviewRoom({
      ...defaultRoomSnapshot,
      status: "live",
      role: "host",
      roomId,
      inviteUrl: `https://rooms.screenoverlayphysics.local/join/${roomId}?key=${generatedPreviewId("invite")}`,
      localDisplayName: displayName,
      targetDisplayId: options.targetDisplayId,
      allowGuestInteraction: options.allowGuestInteraction,
      featureFilters: { ...options.featureFilters },
      participants: [{
        id: "local",
        displayName,
        role: "host",
        canInteract: true,
        status: "live",
        latencyMs: 0,
      }],
      relayRegion: "Browser simulation",
      latencyMs: 0,
      lastPacketAgeMs: 0,
    });
  }
  return invoke<RoomSnapshot>("room_host", options);
}

export async function joinRoom(options: JoinRoomOptions): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    const roomId = previewRoomIdFromInvite(options.inviteUrl);
    const displayName = normalizedPreviewName(options.displayName);
    return commitPreviewRoom({
      ...defaultRoomSnapshot,
      status: "live",
      role: "guest",
      roomId,
      localDisplayName: displayName,
      targetDisplayId: options.targetDisplayId,
      participants: [
        {
          id: "simulated-host",
          displayName: "Room host",
          role: "host",
          canInteract: true,
          status: "live",
          latencyMs: 18,
        },
        {
          id: "local",
          displayName,
          role: "guest",
          canInteract: true,
          status: "live",
          latencyMs: 0,
        },
      ],
      relayRegion: "Browser simulation",
      latencyMs: 18,
      lastPacketAgeMs: 4,
    });
  }
  return invoke<RoomSnapshot>("room_join", options);
}

export async function leaveRoom(): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    return commitPreviewRoom(resetPreviewRoom());
  }
  return invoke<RoomSnapshot>("room_leave");
}

export async function endRoom(): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    return commitPreviewRoom(resetPreviewRoom());
  }
  return invoke<RoomSnapshot>("room_end");
}

export async function setRoomInteraction(enabled: boolean): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    ensurePreviewHost();
    return commitPreviewRoom({
      ...previewRoom,
      allowGuestInteraction: enabled,
      participants: previewRoom.participants.map((participant) => (
        participant.role === "guest" ? { ...participant, canInteract: enabled } : participant
      )),
    });
  }
  return invoke<RoomSnapshot>("room_set_interaction", { enabled });
}

export async function setRoomParticipantInteraction(
  participantId: string,
  enabled: boolean,
): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    ensurePreviewHost();
    return commitPreviewRoom({
      ...previewRoom,
      participants: previewRoom.participants.map((participant) => (
        participant.id === participantId ? { ...participant, canInteract: enabled } : participant
      )),
    });
  }
  return invoke<RoomSnapshot>("room_set_participant_interaction", { participantId, enabled });
}

export async function removeRoomParticipant(participantId: string): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    ensurePreviewHost();
    return commitPreviewRoom({
      ...previewRoom,
      participants: previewRoom.participants.filter((participant) => participant.id === "local" || participant.id !== participantId),
    });
  }
  return invoke<RoomSnapshot>("room_remove_participant", { participantId });
}

export async function setRoomFeatures(featureFilters: RoomFeatureFilters): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    ensurePreviewHost();
    return commitPreviewRoom({ ...previewRoom, featureFilters: { ...featureFilters } });
  }
  return invoke<RoomSnapshot>("room_set_features", { featureFilters });
}

export async function regenerateRoomInvite(): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    ensurePreviewHost();
    return commitPreviewRoom({
      ...previewRoom,
      inviteUrl: previewRoom.roomId
        ? `https://rooms.screenoverlayphysics.local/join/${previewRoom.roomId}?key=${generatedPreviewId("invite")}`
        : null,
    });
  }
  return invoke<RoomSnapshot>("room_regenerate_invite");
}

export async function reconnectRoom(): Promise<RoomSnapshot> {
  if (!hasTauriRuntime()) {
    if (!previewRoom.roomId) {
      throw new Error("No room is available to reconnect.");
    }
    return commitPreviewRoom({
      ...previewRoom,
      status: "live",
      retryInMs: null,
      lastError: null,
      lastPacketAgeMs: 0,
    });
  }
  return invoke<RoomSnapshot>("room_reconnect");
}

export async function openRoomWindow(): Promise<void> {
  if (!hasTauriRuntime()) {
    const url = new URL(window.location.href);
    url.searchParams.set("surface", "room");
    window.open(url, "screen-overlay-room", "popup=yes,width=480,height=640");
    return;
  }
  return invoke<void>("open_room_window");
}

export async function hideRoomWindow(): Promise<void> {
  if (!hasTauriRuntime()) {
    window.close();
    return;
  }
  return invoke<void>("hide_room_window");
}

export async function minimizeRoomWindow(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }
  return invoke<void>("minimize_room_window");
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
    } else if (command === "set_obs_output") {
      previewObsOutputEnabled = Boolean(payload.enabled);
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

function cloneRoomSnapshot(snapshot: RoomSnapshot): RoomSnapshot {
  return {
    ...snapshot,
    featureFilters: { ...snapshot.featureFilters },
    participants: snapshot.participants.map((participant) => ({ ...participant })),
  };
}

function normalizeRoomSnapshot(snapshot: Partial<RoomSnapshot>): RoomSnapshot {
  return cloneRoomSnapshot({
    ...defaultRoomSnapshot,
    ...snapshot,
    featureFilters: {
      ...defaultRoomFeatureFilters,
      ...snapshot.featureFilters,
    },
    participants: Array.isArray(snapshot.participants) ? snapshot.participants : [],
  });
}

function loadPreviewRoom(): RoomSnapshot {
  if (typeof window === "undefined") {
    return cloneRoomSnapshot(defaultRoomSnapshot);
  }
  try {
    const saved = window.localStorage.getItem(roomPreviewStorageKey);
    return saved ? normalizeRoomSnapshot(JSON.parse(saved) as Partial<RoomSnapshot>) : cloneRoomSnapshot(defaultRoomSnapshot);
  } catch {
    return cloneRoomSnapshot(defaultRoomSnapshot);
  }
}

function commitPreviewRoom(snapshot: RoomSnapshot): RoomSnapshot {
  previewRoom = cloneRoomSnapshot(snapshot);
  if (typeof window !== "undefined") {
    window.localStorage.setItem(roomPreviewStorageKey, JSON.stringify(previewRoom));
  }
  previewRoomChannel?.postMessage(previewRoom);
  notifyPreviewRoomListeners();
  return cloneRoomSnapshot(previewRoom);
}

function notifyPreviewRoomListeners() {
  for (const listener of previewRoomListeners) {
    listener(cloneRoomSnapshot(previewRoom));
  }
}

function resetPreviewRoom(): RoomSnapshot {
  return {
    ...defaultRoomSnapshot,
    localDisplayName: previewRoom.localDisplayName,
    targetDisplayId: previewRoom.targetDisplayId,
    allowGuestInteraction: previewRoom.allowGuestInteraction,
    featureFilters: { ...previewRoom.featureFilters },
  };
}

function ensurePreviewHost() {
  if (previewRoom.role !== "host") {
    throw new Error("Only the room host can change this setting.");
  }
}

function generatedPreviewId(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}`;
}

function normalizedPreviewName(displayName: string) {
  return displayName.trim().slice(0, 32) || "Desktop friend";
}

function previewRoomIdFromInvite(inviteUrl: string) {
  let parsed: URL;
  try {
    parsed = new URL(inviteUrl.trim());
  } catch {
    throw new Error("Paste a complete room invite URL.");
  }
  const parts = parsed.pathname.split("/").filter(Boolean);
  const joinIndex = parts.indexOf("join");
  const roomId = joinIndex >= 0 ? parts[joinIndex + 1] : null;
  if (!roomId) {
    throw new Error("This does not look like a room invite URL.");
  }
  return roomId;
}
