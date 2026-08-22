import type { ComponentChildren } from "preact";
import { useEffect, useMemo, useState } from "preact/hooks";
import {
  Box,
  Circle,
  Cog,
  Crosshair,
  Disc,
  Gem,
  Grid2X2,
  Link,
  MessageCircle,
  Minus,
  PackageOpen,
  Radio,
  RotateCcw,
  Sparkles,
  Target,
  Triangle,
  UsersRound,
  Workflow,
  X,
} from "lucide-preact";
import {
  dispatchEngineCommand,
  connectTwitch,
  disconnectTwitch,
  CaseActive,
  CaseStatus,
  CaseTier,
  DisplayInfo,
  EngineCommandPayload,
  EngineSnapshot,
  hideOverlay,
  minimizeOverlay,
  notifyFrontendReady,
  RuntimeSettings,
  setTwitchFeatures,
  TwitchSnapshot,
  getCaseStatus,
  getEngineSnapshot,
  getDisplayLayout,
  getTwitchSnapshot,
} from "./bridge";
import { CompactRoomPanel, useRoomState } from "./room";

type Tone = "steel" | "cyan" | "amber" | "violet" | "green";
type SpawnVariant = {
  label: string;
  kind?: string;
  command?: string;
  icon: typeof Box;
  tone: Tone;
  shader?: boolean;
  textInput?: boolean;
};

type ActiveTab = "objects" | "labs" | "case" | "twitch" | "room";
type LabAction = {
  label: string;
  shortLabel?: string;
  command: string;
  payload?: EngineCommandPayload;
  icon: typeof Box;
  tone: Tone;
};

const defaultRuntimeSettings: RuntimeSettings = {
  gravityY: 1800,
  throwSensitivity: 1.1,
  maxThrowSpeed: 2600,
  restitution: 0.75,
  linearDamping: 0.992,
  sleepThreshold: 24,
  floorSnapThreshold: 3,
  interactionDebounceMs: 80,
  startInPassThrough: true,
};

const spawnGroups: Array<{ id: string; label: string; variants: SpawnVariant[] }> = [
  {
    id: "core",
    label: "Core",
    variants: [
      { label: "Cube", kind: "Cube", icon: Box, tone: "steel" },
      { label: "Ball", kind: "Ball", icon: Circle, tone: "cyan" },
      { label: "Dice", kind: "Dice", icon: Grid2X2, tone: "steel" },
      { label: "Pyramid", kind: "Pyramid", icon: Triangle, tone: "amber" },
      { label: "Ring", kind: "Ring", icon: Circle, tone: "violet" },
      { label: "Star", kind: "Star", icon: Sparkles, tone: "amber" },
      { label: "Text", kind: "Text", icon: MessageCircle, tone: "violet", textInput: true },
    ],
  },
  {
    id: "shader",
    label: "Shader",
    variants: [
      { label: "Glass", kind: "GlassMarble", icon: Circle, tone: "cyan", shader: true },
      { label: "Plasma", kind: "PlasmaOrb", icon: Sparkles, tone: "violet", shader: true },
      { label: "Portal", kind: "PortalOrb", icon: Circle, tone: "cyan", shader: true },
      { label: "Bubble", kind: "SoapBubble", icon: Circle, tone: "steel", shader: true },
      { label: "Shield", kind: "ForcefieldOrb", icon: Circle, tone: "green", shader: true },
      { label: "Ray Cube", kind: "RaymarchCube", icon: Box, tone: "violet", shader: true },
    ],
  },
  {
    id: "special",
    label: "Special",
    variants: [
      { label: "Crystal", kind: "Crystal", icon: Gem, tone: "cyan" },
      { label: "DVD", kind: "DvdLogo", icon: Disc, tone: "violet" },
      { label: "Target", kind: "GameTarget", icon: Target, tone: "amber" },
      { label: "Robot", kind: "RobotBuddy", icon: Workflow, tone: "cyan" },
      { label: "Fan", kind: "Fan", icon: Sparkles, tone: "cyan" },
      { label: "Drone", kind: "QuadDrone", icon: Workflow, tone: "steel" },
      { label: "Model", command: "import_model", icon: Workflow, tone: "steel" },
      { label: "Batch", command: "spawn_stress_batch", icon: Grid2X2, tone: "amber" },
    ],
  },
];

const labGroups: Array<{ id: string; label: string; actions: LabAction[] }> = [
  {
    id: "tools",
    label: "Tools",
    actions: [
      { label: "Debug HUD", shortLabel: "Debug", command: "toggle_debug", icon: Sparkles, tone: "green" },
      { label: "Measure", command: "toggle_measure_tool", icon: Target, tone: "cyan" },
      { label: "Spotlight", command: "toggle_spotlight", icon: Circle, tone: "amber" },
      { label: "Rope Lasso", shortLabel: "Lasso", command: "toggle_lasso_tool", icon: Workflow, tone: "cyan" },
      { label: "Release Lasso", shortLabel: "Release", command: "release_lasso", icon: Workflow, tone: "steel" },
      { label: "Portal Pair", shortLabel: "Portals", command: "toggle_portal_pair_tool", icon: Workflow, tone: "cyan" },
      { label: "Shatter Gun", shortLabel: "Gun", command: "toggle_shatter_gun", icon: Crosshair, tone: "amber" },
      { label: "Shatter Screen", shortLabel: "Shatter", command: "shatter_screen", icon: Crosshair, tone: "violet" },
    ],
  },
  {
    id: "world",
    label: "World",
    actions: [
      { label: "Rain", command: "toggle_weather", icon: Sparkles, tone: "cyan" },
      { label: "Sand", command: "toggle_sand", icon: Circle, tone: "amber" },
      { label: "Stress Batch", shortLabel: "Batch", command: "spawn_stress_batch", icon: Grid2X2, tone: "amber" },
    ],
  },
  {
    id: "games",
    label: "Games",
    actions: [
      { label: "Slingshot", command: "toggle_slingshot_game", icon: Target, tone: "amber" },
      { label: "Basketball", shortLabel: "Hoops", command: "toggle_basketball_game", icon: Circle, tone: "amber" },
      { label: "Robot Buddy", shortLabel: "Robot", command: "spawn_robot_buddy", icon: Workflow, tone: "cyan" },
    ],
  },
  {
    id: "spawns",
    label: "Spawns",
    actions: [
      { label: "Fox Buddy", shortLabel: "Fox", command: "spawn_visual_kind", payload: { kind: "FoxBuddy", label: "Fox Buddy" }, icon: Sparkles, tone: "amber" },
      { label: "Mouse Snail", shortLabel: "Snail", command: "spawn_visual_kind", payload: { kind: "Snail", label: "Mouse Snail" }, icon: Circle, tone: "green" },
      { label: "Fan", command: "spawn_visual_kind", payload: { kind: "Fan", label: "Fan" }, icon: Sparkles, tone: "cyan" },
      { label: "Quadcopter", shortLabel: "Drone", command: "spawn_visual_kind", payload: { kind: "QuadDrone", label: "Quadcopter" }, icon: Workflow, tone: "steel" },
      { label: "Satellite", command: "spawn_visual_kind", payload: { kind: "Satellite", label: "Satellite" }, icon: Workflow, tone: "cyan" },
      { label: "Barrel", command: "spawn_visual_kind", payload: { kind: "Barrel", label: "Barrel" }, icon: Circle, tone: "green" },
      { label: "Game Plank", shortLabel: "Plank", command: "spawn_visual_kind", payload: { kind: "GamePlank", label: "Game Plank" }, icon: Box, tone: "amber" },
      { label: "Basketball", shortLabel: "Ball", command: "spawn_visual_kind", payload: { kind: "Basketball", label: "Basketball" }, icon: Circle, tone: "amber" },
      { label: "Hoop", command: "spawn_visual_kind", payload: { kind: "BasketballHoop", label: "Hoop" }, icon: Circle, tone: "steel" },
      { label: "Soft Ball", command: "spawn_visual_kind", payload: { kind: "SoftBall", label: "Soft Ball" }, icon: Circle, tone: "cyan" },
      { label: "Next Catalog", shortLabel: "Catalog", command: "spawn_next_catalog", icon: Grid2X2, tone: "violet" },
      { label: "Import Model", shortLabel: "Model", command: "import_model", icon: Workflow, tone: "steel" },
    ],
  },
];

const quickLabActionKeys = new Set(["toggle_shatter_gun", "shatter_screen", "toggle_weather", "toggle_sand"]);
const defaultSnapshot: EngineSnapshot = {
  connected: false,
  transport: "loading",
  rendererBackend: "native overlay",
  overlayMode: "compact pod",
  activePreset: "Desk orbit",
  sceneMode: "Arrange",
  settings: defaultRuntimeSettings,
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

const defaultTwitchSnapshot: TwitchSnapshot = {
  status: "disconnected",
  clientConfigured: false,
  broadcasterDisplayName: null,
  userCode: null,
  verificationUri: null,
  chatEnabled: true,
  bitsEnabled: true,
  lastError: null,
  chatMessagesReceived: 0,
  bitsEventsReceived: 0,
};

export function App() {
  const [snapshot, setSnapshot] = useState<EngineSnapshot>(defaultSnapshot);
  const [activeTab, setActiveTab] = useState<ActiveTab>("objects");
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);
  const [selectedGroupId, setSelectedGroupId] = useState("shader");
  const [selectedVariantKey, setSelectedVariantKey] = useState("PlasmaOrb");
  const [selectedLabGroupId, setSelectedLabGroupId] = useState("tools");
  const [selectedLabActionKey, setSelectedLabActionKey] = useState("toggle_debug");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<RuntimeSettings>(defaultRuntimeSettings);
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [spawnDisplayId, setSpawnDisplayId] = useState("");
  const [textObjectValue, setTextObjectValue] = useState("Hello!");
  const [textObjectSize, setTextObjectSize] = useState("medium");
  const [caseStatus, setCaseStatus] = useState<CaseStatus | null>(null);
  const [caseLoaded, setCaseLoaded] = useState(false);
  const [caseViewer, setCaseViewer] = useState("STREAMER");
  const [caseTierId, setCaseTierId] = useState("");
  const [caseRewardName, setCaseRewardName] = useState("");
  const [caseSeed, setCaseSeed] = useState("");
  const [twitch, setTwitch] = useState<TwitchSnapshot>(defaultTwitchSnapshot);
  const [twitchLoaded, setTwitchLoaded] = useState(false);
  const [twitchActionError, setTwitchActionError] = useState<string | null>(null);
  const room = useRoomState();

  useEffect(() => {
    void notifyFrontendReady();
    getEngineSnapshot().then(applySnapshot).catch(() => applySnapshot(defaultSnapshot));
    getDisplayLayout().then((layout) => {
      setDisplays(layout);
      const savedDisplayId = window.localStorage.getItem("overlay.spawnDisplayId");
      const preferred = layout.find((display) => display.id === savedDisplayId)
        ?? layout.find((display) => display.primary)
        ?? layout[0];
      if (preferred) {
        setSpawnDisplayId(preferred.id);
        void dispatchEngineCommand("set_spawn_monitor", displayPayload(preferred)).then(applySnapshot);
      }
    }).catch(() => setDisplays([]));
  }, []);

  useEffect(() => {
    if (!settingsOpen) {
      setSettingsDraft(snapshot.settings);
    }
  }, [settingsOpen, snapshot.settings]);

  useEffect(() => {
    if (activeTab !== "twitch") {
      return;
    }

    let cancelled = false;
    const refresh = () => {
      void getTwitchSnapshot()
        .then((next) => {
          if (!cancelled) {
            setTwitch(next);
            setTwitchLoaded(true);
          }
        })
        .catch((error) => {
          if (!cancelled) {
            setTwitchActionError(errorMessage(error));
            setTwitchLoaded(true);
          }
        });
    };

    refresh();
    const poll = window.setInterval(refresh, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(poll);
    };
  }, [activeTab]);

  useEffect(() => {
    if (activeTab !== "case") {
      return;
    }

    let cancelled = false;
    const refresh = () => {
      void getCaseStatus().then((next) => {
        if (!cancelled) {
          setCaseStatus(next);
          setCaseLoaded(true);
        }
      });
    };

    refresh();
    // Fast enough to watch a sequence play out, slow enough to stay cheap.
    const poll = window.setInterval(refresh, 400);
    return () => {
      cancelled = true;
      window.clearInterval(poll);
    };
  }, [activeTab]);

  const activeObjects = useMemo(() => Math.max(snapshot.objectCount, 1), [snapshot.objectCount]);
  const displayObjects = snapshot.connected ? activeObjects : 124;
  const selectedGroup = spawnGroups.find((group) => group.id === selectedGroupId) ?? spawnGroups[1];
  const selectedVariant =
    selectedGroup.variants.find((variant) => variantKey(variant) === selectedVariantKey) ?? selectedGroup.variants[0];
  const selectedLabGroup = labGroups.find((group) => group.id === selectedLabGroupId) ?? labGroups[0];
  const selectedLabAction =
    selectedLabGroup.actions.find((action) => labActionKey(action) === selectedLabActionKey) ?? selectedLabGroup.actions[0];
  const quickLabActions = labGroups.flatMap((group) => group.actions).filter((action) => quickLabActionKeys.has(action.command));
  const SelectedSpawnIcon = selectedVariant.icon;
  const SelectedLabIcon = selectedLabAction.icon;
  const commandText = formatCommand(snapshot.lastCommand);

  async function send(command: string, payload: EngineCommandPayload = {}) {
    setPendingCommand(command);
    try {
      const next = await dispatchEngineCommand(command, payload);
      applySnapshot(next);
    } finally {
      window.setTimeout(() => setPendingCommand(null), 260);
    }
  }

  function applySnapshot(next: EngineSnapshot) {
    setSnapshot(next);
  }

  function chooseGroup(groupId: string) {
    const nextGroup = spawnGroups.find((group) => group.id === groupId) ?? spawnGroups[0];
    setSelectedGroupId(nextGroup.id);
    setSelectedVariantKey(variantKey(nextGroup.variants[0]));
  }

  function chooseLabGroup(groupId: string) {
    const nextGroup = labGroups.find((group) => group.id === groupId) ?? labGroups[0];
    setSelectedLabGroupId(nextGroup.id);
    setSelectedLabActionKey(labActionKey(nextGroup.actions[0]));
  }

  function spawnSelected() {
    if (selectedVariant.command) {
      return send(selectedVariant.command);
    }

    return send("spawn_visual_kind", {
      kind: selectedVariant.kind ?? "Cube",
      label: selectedVariant.label,
      shader: Boolean(selectedVariant.shader),
      ...(selectedVariant.textInput ? { text: textObjectValue.trim() || "Text" } : {}),
      ...(selectedVariant.textInput ? { textSize: textObjectSize } : {}),
    });
  }

  function runLabAction(action: LabAction = selectedLabAction) {
    return send(action.command, action.payload ?? {});
  }

  /// Opens a case, optionally forcing a tier and an exact prize. Anything left
  /// blank is left to the roll.
  function openCase(options: { tierId?: string; rewardName?: string } = {}) {
    const payload: EngineCommandPayload = {
      user: caseViewer.trim() || "STREAMER",
    };
    const tierId = options.tierId ?? caseTierId;
    if (tierId) {
      payload.rarity = tierId;
    }
    const rewardName = options.rewardName ?? (options.tierId ? "" : caseRewardName);
    if (rewardName) {
      payload.reward = rewardName;
    }
    const seed = Number.parseInt(caseSeed, 10);
    if (Number.isFinite(seed) && seed >= 0) {
      payload.seed = seed;
    }
    return send("open_case", payload);
  }

  function chooseCaseTier(tierId: string) {
    setCaseTierId(tierId);
    // A prize from the previous tier cannot be forced alongside a new one.
    setCaseRewardName("");
  }

  function openSettings() {
    setSettingsDraft(snapshot.settings);
    setSettingsOpen(true);
  }

  function applySettings() {
    void send("apply_runtime_settings", {
      ...settingsDraft,
      clickThrough: settingsDraft.startInPassThrough,
    });
    setSettingsOpen(false);
  }

  async function runTwitchAction(
    command: "twitch_connect" | "twitch_disconnect" | "twitch_set_features",
    action: () => Promise<TwitchSnapshot>,
  ) {
    setPendingCommand(command);
    setTwitchActionError(null);
    try {
      const next = await action();
      setTwitch(next);
      setTwitchLoaded(true);
    } catch (error) {
      setTwitchActionError(errorMessage(error));
    } finally {
      setPendingCommand(null);
    }
  }

  function toggleTwitchFeature(feature: "chat" | "bits", enabled: boolean) {
    const chatEnabled = feature === "chat" ? enabled : twitch.chatEnabled;
    const bitsEnabled = feature === "bits" ? enabled : twitch.bitsEnabled;
    return runTwitchAction("twitch_set_features", () => setTwitchFeatures(chatEnabled, bitsEnabled));
  }

  function chooseSpawnDisplay(displayId: string) {
    setSpawnDisplayId(displayId);
    window.localStorage.setItem("overlay.spawnDisplayId", displayId);
    const display = displays.find((candidate) => candidate.id === displayId);
    if (display) {
      void send("set_spawn_monitor", displayPayload(display));
    }
  }

  return (
    <main className="pod-shell" data-pending={pendingCommand ?? ""}>
      <section className="command-pod" aria-label="Screen overlay command pod">
        <span className="corner-screw top-left" aria-hidden="true" />
        <span className="corner-screw top-right" aria-hidden="true" />
        <span className="corner-screw bottom-left" aria-hidden="true" />
        <span className="corner-screw bottom-right" aria-hidden="true" />

        <header className="pod-header">
          <div className="brand-lockup">
            <span className="brand-mark" aria-hidden="true" />
            <strong>Overlay</strong>
          </div>
          <div className="window-controls">
            <button
              className="window-button bevel-button"
              type="button"
              aria-label="Minimize overlay UI"
              onClick={() => void minimizeOverlay()}
            >
              <Minus size={14} strokeWidth={2.6} />
            </button>
            <button
              className="window-button bevel-button"
              type="button"
              aria-label="Hide overlay UI"
              onClick={() => void hideOverlay()}
            >
              <X size={15} strokeWidth={2.4} />
            </button>
          </div>
        </header>

        <section className="tab-header" role="tablist" aria-label="Overlay panel sections">
          <button
            className={activeTab === "objects" ? "panel-tab selected bevel-button" : "panel-tab bevel-button"}
            type="button"
            role="tab"
            aria-selected={activeTab === "objects"}
            aria-controls="objects-panel"
            onClick={() => setActiveTab("objects")}
          >
            <span className="status-led" />
            <span>Objects</span>
            <strong>{displayObjects}</strong>
          </button>
          <button
            className={activeTab === "labs" ? "panel-tab selected bevel-button" : "panel-tab bevel-button"}
            type="button"
            role="tab"
            aria-selected={activeTab === "labs"}
            aria-controls="labs-panel"
            onClick={() => setActiveTab("labs")}
          >
            <Crosshair size={12} strokeWidth={2.2} />
            <span>Labs</span>
          </button>
          <button
            className={activeTab === "case" ? "panel-tab selected bevel-button" : "panel-tab bevel-button"}
            type="button"
            role="tab"
            aria-selected={activeTab === "case"}
            aria-controls="case-panel"
            onClick={() => setActiveTab("case")}
          >
            <PackageOpen size={12} strokeWidth={2.2} />
            <span>Case</span>
          </button>
          <button
            className={activeTab === "twitch" ? "panel-tab selected bevel-button" : "panel-tab bevel-button"}
            type="button"
            role="tab"
            aria-selected={activeTab === "twitch"}
            aria-controls="twitch-panel"
            onClick={() => setActiveTab("twitch")}
          >
            <Radio size={12} strokeWidth={2.2} />
            <span>Twitch</span>
          </button>
          <button
            className={activeTab === "room" ? "panel-tab room-tab selected bevel-button" : "panel-tab room-tab bevel-button"}
            type="button"
            role="tab"
            aria-selected={activeTab === "room"}
            aria-controls="room-panel"
            onClick={() => setActiveTab("room")}
          >
            {room.snapshot.status === "live" ? (
              <span className="status-led room-led" aria-hidden="true" />
            ) : (
              <UsersRound size={12} strokeWidth={2.2} />
            )}
            <span>Room</span>
            {room.snapshot.participants.length > 0 ? <strong>{room.snapshot.participants.length}</strong> : null}
          </button>
        </section>

        {activeTab === "objects" ? (
          <FrameSection label="Spawn" className="spawn-section" id="objects-panel">
            <div className="spawn-selector">
              <div className="spawn-row">
                <span className={`spawn-icon tone-${selectedVariant.tone}`} aria-hidden="true">
                  <SelectedSpawnIcon size={17} strokeWidth={2} />
                </span>
                <select aria-label="Spawn family" value={selectedGroupId} onChange={(event) => chooseGroup(event.currentTarget.value)}>
                  {spawnGroups.map((group) => (
                    <option value={group.id} key={group.id}>
                      {group.label}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="Spawn variant"
                  value={variantKey(selectedVariant)}
                  onChange={(event) => setSelectedVariantKey(event.currentTarget.value)}
                >
                  {selectedGroup.variants.map((variant) => (
                    <option value={variantKey(variant)} key={variantKey(variant)}>
                      {variant.label}
                    </option>
                  ))}
                </select>
                <button
                  className={`spawn-now bevel-button ${pendingCommand === "spawn_visual_kind" || pendingCommand === selectedVariant.command ? "is-pending" : ""}`}
                  type="button"
                  onClick={() => void spawnSelected()}
                >
                  Spawn
                </button>
              </div>
              {selectedVariant.textInput ? (
                <div className="spawn-text-controls">
                  <input
                    className="spawn-text-input"
                    aria-label="Text object content"
                    value={textObjectValue}
                    maxLength={120}
                    placeholder="Enter text"
                    onInput={(event) => setTextObjectValue(event.currentTarget.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        void spawnSelected();
                      }
                    }}
                  />
                  <select
                    className="spawn-text-size"
                    aria-label="Text object size"
                    value={textObjectSize}
                    onChange={(event) => setTextObjectSize(event.currentTarget.value)}
                  >
                    <option value="small">Small</option>
                    <option value="medium">Medium</option>
                    <option value="large">Large</option>
                    <option value="xlarge">X-Large</option>
                  </select>
                </div>
              ) : null}
            </div>
          </FrameSection>
        ) : activeTab === "labs" ? (
          <FrameSection label="Experimental" className="labs-section" id="labs-panel">
            <div className="lab-selector">
              <div className="lab-row">
                <span className={`spawn-icon tone-${selectedLabAction.tone}`} aria-hidden="true">
                  <SelectedLabIcon size={17} strokeWidth={2} />
                </span>
                <select aria-label="Lab family" value={selectedLabGroupId} onChange={(event) => chooseLabGroup(event.currentTarget.value)}>
                  {labGroups.map((group) => (
                    <option value={group.id} key={group.id}>
                      {group.label}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="Lab action"
                  value={labActionKey(selectedLabAction)}
                  onChange={(event) => setSelectedLabActionKey(event.currentTarget.value)}
                >
                  {selectedLabGroup.actions.map((action) => (
                    <option value={labActionKey(action)} key={labActionKey(action)}>
                      {action.label}
                    </option>
                  ))}
                </select>
                <button
                  className={`spawn-now bevel-button ${pendingCommand === selectedLabAction.command ? "is-pending" : ""}`}
                  type="button"
                  onClick={() => void runLabAction()}
                >
                  Run
                </button>
              </div>
              <div className="lab-quick-grid" aria-label="Quick experimental actions">
                {quickLabActions.map((action) => {
                  const Icon = action.icon;
                  const selected = action.command === "toggle_shatter_gun" && snapshot.shatterGunEquipped;
                  return (
                    <button
                      className={`lab-quick bevel-button ${selected ? "selected" : ""} ${pendingCommand === action.command ? "is-pending" : ""}`}
                      type="button"
                      key={labActionKey(action)}
                      onClick={() => void runLabAction(action)}
                      aria-pressed={action.command === "toggle_shatter_gun" ? snapshot.shatterGunEquipped : undefined}
                    >
                      <Icon size={12} strokeWidth={2.1} />
                      <strong>{action.command === "toggle_shatter_gun" && snapshot.shatterGunEquipped ? "Armed" : action.shortLabel ?? action.label}</strong>
                    </button>
                  );
                })}
              </div>
            </div>
          </FrameSection>
        ) : activeTab === "case" ? (
          <FrameSection label="Case Opening" className="case-section" id="case-panel">
            <CasePanel
              status={caseStatus}
              loaded={caseLoaded}
              pendingCommand={pendingCommand}
              viewer={caseViewer}
              tierId={caseTierId}
              rewardName={caseRewardName}
              seed={caseSeed}
              onViewerChange={setCaseViewer}
              onTierChange={chooseCaseTier}
              onRewardChange={setCaseRewardName}
              onSeedChange={setCaseSeed}
              onOpen={(options) => void openCase(options)}
              onCancel={() => void send("cancel_case")}
            />
          </FrameSection>
        ) : activeTab === "twitch" ? (
          <FrameSection label="Twitch EventSub" className="twitch-section" id="twitch-panel">
            <TwitchPanel
              snapshot={twitch}
              loaded={twitchLoaded}
              pendingCommand={pendingCommand}
              actionError={twitchActionError}
              onConnect={() => void runTwitchAction("twitch_connect", connectTwitch)}
              onDisconnect={() => void runTwitchAction("twitch_disconnect", disconnectTwitch)}
              onFeatureChange={(feature, enabled) => void toggleTwitchFeature(feature, enabled)}
            />
          </FrameSection>
        ) : (
          <FrameSection label="Shared Room" className="room-section" id="room-panel">
            <CompactRoomPanel {...room} />
          </FrameSection>
        )}

        <section className="control-strip" aria-label="Quick controls">
          <button
            className={`utility-control obs-control bevel-button ${snapshot.obsOutputEnabled ? "selected" : ""}`}
            type="button"
            role="switch"
            aria-checked={snapshot.obsOutputEnabled}
            onClick={() => void send("set_obs_output", { enabled: !snapshot.obsOutputEnabled })}
            aria-label={`${snapshot.obsOutputEnabled ? "Disable" : "Enable"} OBS Spout output`}
          >
            <Radio size={15} strokeWidth={2} />
            <strong>OBS {snapshot.obsOutputEnabled ? "On" : "Off"}</strong>
          </button>
          <button className="utility-control bevel-button" type="button" onClick={() => void send("reset_scene")} aria-label="Reset scene">
            <RotateCcw size={15} strokeWidth={2} />
            <strong>Reset</strong>
          </button>
          <button
            className={`utility-control bevel-button ${settingsOpen ? "selected" : ""}`}
            type="button"
            onClick={settingsOpen ? () => setSettingsOpen(false) : openSettings}
            aria-label="Overlay settings"
          >
            <Cog size={15} strokeWidth={2} />
            <strong>Set</strong>
          </button>
        </section>

        <footer className="event-panel" aria-label="Recent command">
          <span>Last</span>
          <strong>{commandText.label}</strong>
        </footer>

        {settingsOpen ? (
          <section className="settings-panel" aria-label="Overlay settings">
            <header>
              <div>
                <Cog size={14} strokeWidth={2.2} />
                <strong>Settings</strong>
              </div>
              <button className="window-button bevel-button" type="button" aria-label="Close settings" onClick={() => setSettingsOpen(false)}>
                <X size={14} strokeWidth={2.4} />
              </button>
            </header>
            <div className="settings-scroll">
              <label className="display-setting">
                <span>Primary spawn display</span>
                <select
                  aria-label="Primary spawn display"
                  value={spawnDisplayId}
                  disabled={displays.length === 0}
                  onChange={(event) => chooseSpawnDisplay(event.currentTarget.value)}
                >
                  {displays.length === 0 ? <option value="">Display unavailable</option> : null}
                  {displays.map((display, index) => (
                    <option value={display.id} key={display.id}>
                      {index + 1}. {display.label} ({display.width} x {display.height}){display.primary ? " - System primary" : ""}
                    </option>
                  ))}
                </select>
              </label>
              <ToggleField
                label="Pass-through"
                checked={settingsDraft.startInPassThrough}
                onChange={(startInPassThrough) => setSettingsDraft((settings) => ({ ...settings, startInPassThrough }))}
              />
              <SettingField
                label="Gravity"
                value={settingsDraft.gravityY}
                min={0}
                max={4000}
                step={50}
                onChange={(gravityY) => setSettingsDraft((settings) => ({ ...settings, gravityY }))}
              />
              <SettingField
                label="Throw"
                value={settingsDraft.throwSensitivity}
                min={0.1}
                max={4}
                step={0.05}
                precision={2}
                onChange={(throwSensitivity) => setSettingsDraft((settings) => ({ ...settings, throwSensitivity }))}
              />
              <SettingField
                label="Max speed"
                value={settingsDraft.maxThrowSpeed}
                min={100}
                max={6000}
                step={100}
                onChange={(maxThrowSpeed) => setSettingsDraft((settings) => ({ ...settings, maxThrowSpeed }))}
              />
              <SettingField
                label="Bounce"
                value={settingsDraft.restitution}
                min={0.05}
                max={1.2}
                step={0.05}
                precision={2}
                onChange={(restitution) => setSettingsDraft((settings) => ({ ...settings, restitution }))}
              />
              <SettingField
                label="Damping"
                value={settingsDraft.linearDamping}
                min={0.9}
                max={0.999}
                step={0.001}
                precision={3}
                onChange={(linearDamping) => setSettingsDraft((settings) => ({ ...settings, linearDamping }))}
              />
              <SettingField
                label="Sleep"
                value={settingsDraft.sleepThreshold}
                min={1}
                max={120}
                step={1}
                onChange={(sleepThreshold) => setSettingsDraft((settings) => ({ ...settings, sleepThreshold }))}
              />
              <SettingField
                label="Debounce"
                value={settingsDraft.interactionDebounceMs}
                min={0}
                max={300}
                step={10}
                onChange={(interactionDebounceMs) => setSettingsDraft((settings) => ({ ...settings, interactionDebounceMs }))}
              />
            </div>
            <footer>
              <button className="utility-control bevel-button" type="button" onClick={() => setSettingsDraft(defaultRuntimeSettings)}>
                Reset
              </button>
              <button className="spawn-now bevel-button" type="button" onClick={applySettings}>
                Apply
              </button>
            </footer>
          </section>
        ) : null}
      </section>
    </main>
  );
}

type FrameSectionProps = {
  label: string;
  className: string;
  id?: string;
  children: ComponentChildren;
};

function FrameSection({ label, className, id, children }: FrameSectionProps) {
  return (
    <section className={`frame-section ${className}`} id={id} role="tabpanel">
      <span className="section-label">{label}</span>
      {children}
    </section>
  );
}

type CasePanelProps = {
  status: CaseStatus | null;
  loaded: boolean;
  pendingCommand: string | null;
  viewer: string;
  tierId: string;
  rewardName: string;
  seed: string;
  onViewerChange: (viewer: string) => void;
  onTierChange: (tierId: string) => void;
  onRewardChange: (rewardName: string) => void;
  onSeedChange: (seed: string) => void;
  onOpen: (options?: { tierId?: string; rewardName?: string }) => void;
  onCancel: () => void;
};

function CasePanel({
  status,
  loaded,
  pendingCommand,
  viewer,
  tierId,
  rewardName,
  seed,
  onViewerChange,
  onTierChange,
  onRewardChange,
  onSeedChange,
  onOpen,
  onCancel,
}: CasePanelProps) {
  if (!loaded) {
    return (
      <div className="case-console case-loading" aria-busy="true" aria-label="Loading case state">
        <div className="case-status-skeleton" />
        <div className="case-tier-skeleton" />
      </div>
    );
  }

  if (!status) {
    return (
      <div className="case-console">
        <div className="case-status status-idle">
          <span className="case-status-copy">
            <strong>Overlay offline</strong>
            <small>Start the overlay to open cases.</small>
          </span>
        </div>
      </div>
    );
  }

  const pending = pendingCommand === "open_case" || pendingCommand === "cancel_case";
  const selectedTier = status.tiers.find((tier) => tier.id === tierId);
  const active = status.active;
  const detail = caseDetailLine(status, active);

  return (
    <div className="case-console">
      <div
        className={`case-status ${active ? "is-active" : ""} ${status.configError ? "has-error" : ""}`}
        style={active ? { borderColor: active.tierColor } : undefined}
        aria-live="polite"
      >
        {active ? (
          <span
            className="case-progress"
            style={{ width: `${Math.round(active.progress * 100)}%`, background: active.tierColor }}
            aria-hidden="true"
          />
        ) : null}
        <span className="case-status-copy">
          <strong style={active ? { color: active.tierColor } : undefined}>
            {active ? `${active.viewer} · ${active.rewardName}` : status.caseName}
          </strong>
          <small title={status.configError ?? status.configLabel}>{detail}</small>
        </span>
        <button
          className="case-open bevel-button"
          type="button"
          disabled={pending}
          onClick={() => onOpen()}
        >
          {pending ? "..." : selectedTier ? "Force" : "Open"}
        </button>
      </div>

      <div className="case-tier-grid" aria-label="Force a rarity">
        {status.tiers.map((tier, index) => (
          <button
            className={`case-tier bevel-button ${tier.id === tierId ? "selected" : ""}`}
            type="button"
            key={tier.id}
            style={{ "--tier-color": tier.color } as Record<string, string>}
            title={caseTierHint(status, tier, index === status.tiers.length - 1)}
            onClick={() => onTierChange(tier.id === tierId ? "" : tier.id)}
            onDblClick={() => onOpen({ tierId: tier.id })}
            aria-pressed={tier.id === tierId}
          >
            <strong>{tier.name}</strong>
            <small>{tier.odds}%</small>
          </button>
        ))}
      </div>

      <div className="case-row">
        <input
          className="case-input"
          aria-label="Viewer name"
          value={viewer}
          maxLength={40}
          placeholder="Viewer"
          onInput={(event) => onViewerChange(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onOpen();
            }
          }}
        />
        <select
          aria-label="Force a prize"
          value={rewardName}
          disabled={!selectedTier}
          onChange={(event) => onRewardChange(event.currentTarget.value)}
        >
          <option value="">{selectedTier ? "Any prize" : "Any rarity"}</option>
          {(selectedTier?.rewards ?? []).map((reward) => (
            <option value={reward} key={reward}>
              {reward}
            </option>
          ))}
        </select>
        <input
          className="case-input case-seed"
          aria-label="Roll seed"
          value={seed}
          maxLength={9}
          placeholder="Seed"
          onInput={(event) => onSeedChange(event.currentTarget.value.replace(/[^0-9]/g, ""))}
        />
        <button className="case-cancel bevel-button" type="button" disabled={pending} onClick={onCancel}>
          Stop
        </button>
      </div>
    </div>
  );
}

/// The one line under the case name: what is playing, or what an opening costs.
function caseDetailLine(status: CaseStatus, active: CaseActive | null) {
  if (status.configError) {
    return `Config error: ${status.configError}`;
  }
  if (active) {
    const wheel = active.wheelPrize ? ` · wheel ${active.wheelPrize}` : "";
    return `${active.phase} ${Math.round(active.progress * 100)}% · ${active.tierName} · ${active.celebration}${wheel}`;
  }
  const queue = status.queued > 0 ? `${status.queued} waiting · ` : "";
  const refused = status.refused > 0 ? ` · ${status.refused} refused` : "";
  return `${queue}${status.chatCommand} costs ${status.rollCost} ${status.currencyName}${refused}`;
}

/// Hover text for a rarity chip. The top tier also lists what its wheel holds,
/// since that is the prize the tier actually pays out.
function caseTierHint(status: CaseStatus, tier: CaseTier, isTopTier: boolean) {
  const base = `${tier.name} · ${tier.odds}% · ${tier.celebration} celebration. Click to arm, double-click to open now.`;
  if (!isTopTier || !status.wheelEnabled || status.wheelPrizes.length === 0) {
    return base;
  }
  return `${base}\nWheel: ${status.wheelPrizes.join(", ")}`;
}

type TwitchPanelProps = {
  snapshot: TwitchSnapshot;
  loaded: boolean;
  pendingCommand: string | null;
  actionError: string | null;
  onConnect: () => void;
  onDisconnect: () => void;
  onFeatureChange: (feature: "chat" | "bits", enabled: boolean) => void;
};

function TwitchPanel({
  snapshot,
  loaded,
  pendingCommand,
  actionError,
  onConnect,
  onDisconnect,
  onFeatureChange,
}: TwitchPanelProps) {
  if (!loaded) {
    return (
      <div className="twitch-console twitch-loading" aria-busy="true" aria-label="Loading Twitch connection">
        <div className="twitch-status-skeleton" />
        <div className="twitch-feature-grid">
          <div className="twitch-feature-skeleton" />
          <div className="twitch-feature-skeleton" />
        </div>
      </div>
    );
  }

  const pending = pendingCommand?.startsWith("twitch_") ?? false;
  const featurePending = pendingCommand === "twitch_set_features";
  const visibleError = actionError ?? snapshot.lastError;
  const status = twitchStatusView(snapshot, visibleError);
  const showDisconnect = snapshot.status === "connected" || snapshot.status === "reconnecting";
  const showCancel = snapshot.status === "authorizing" || snapshot.status === "connecting";
  const featureDisabled = !snapshot.clientConfigured || featurePending;

  return (
    <div className="twitch-console">
      <div className={`twitch-status status-${status.tone}`} aria-live="polite">
        <span className="twitch-status-icon" aria-hidden="true">
          {snapshot.status === "authorizing" ? <Link size={15} strokeWidth={2.1} /> : <Radio size={15} strokeWidth={2.1} />}
        </span>
        <span className="twitch-status-copy">
          <strong>{status.title}</strong>
          <small title={status.detail}>{status.detail}</small>
        </span>
        <button
          className="twitch-connect bevel-button"
          type="button"
          disabled={status.buttonDisabled || pending}
          onClick={showDisconnect || showCancel ? onDisconnect : onConnect}
        >
          {pending ? "Working" : showCancel ? "Cancel" : showDisconnect ? "Disconnect" : status.buttonLabel}
        </button>
      </div>

      <div className="twitch-feature-grid" aria-label="Twitch event controls">
        <TwitchFeature
          label="Chat"
          detail={snapshot.status === "connected" ? `${snapshot.chatMessagesReceived} received` : "Falling messages"}
          icon={MessageCircle}
          checked={snapshot.chatEnabled}
          disabled={featureDisabled}
          onChange={(enabled) => onFeatureChange("chat", enabled)}
        />
        <TwitchFeature
          label="Bits"
          detail={snapshot.status === "connected" ? `${snapshot.bitsEventsReceived} received` : "Crystal drops"}
          icon={Gem}
          checked={snapshot.bitsEnabled}
          disabled={featureDisabled}
          onChange={(enabled) => onFeatureChange("bits", enabled)}
        />
      </div>
    </div>
  );
}

type TwitchFeatureProps = {
  label: string;
  detail: string;
  icon: typeof Box;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
};

function TwitchFeature({ label, detail, icon: Icon, checked, disabled, onChange }: TwitchFeatureProps) {
  return (
    <label className={`twitch-feature ${checked ? "enabled" : ""} ${disabled ? "disabled" : ""}`}>
      <Icon size={15} strokeWidth={2.1} aria-hidden="true" />
      <span>
        <strong>{label}</strong>
        <small>{detail}</small>
      </span>
      <input
        type="checkbox"
        role="switch"
        aria-label={`Enable Twitch ${label}`}
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
      <i aria-hidden="true" />
    </label>
  );
}

type SettingFieldProps = {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  precision?: number;
  onChange: (value: number) => void;
};

function SettingField({ label, value, min, max, step, precision = 0, onChange }: SettingFieldProps) {
  return (
    <label className="setting-field">
      <span>{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onInput={(event) => onChange(Number(event.currentTarget.value))}
      />
      <strong>{precision > 0 ? value.toFixed(precision) : Math.round(value)}</strong>
    </label>
  );
}

type ToggleFieldProps = {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
};

function ToggleField({ label, checked, onChange }: ToggleFieldProps) {
  return (
    <label className="setting-toggle">
      <span>{label}</span>
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.currentTarget.checked)} />
      <i aria-hidden="true" />
    </label>
  );
}

function variantKey(variant: SpawnVariant) {
  return variant.kind ?? variant.command ?? variant.label;
}

function labActionKey(action: LabAction) {
  return action.payload?.kind?.toString() ?? action.command;
}

function formatCommand(lastCommand: string | null) {
  if (!lastCommand) {
    return {
      label: "SPAWN PLASMA",
    };
  }

  return {
    label: lastCommand.split("_").join(" ").toUpperCase(),
  };
}

function twitchStatusView(snapshot: TwitchSnapshot, visibleError: string | null) {
  if (visibleError) {
    return {
      tone: "error",
      title: "Twitch action failed",
      detail: visibleError,
      buttonLabel: "Retry",
      buttonDisabled: false,
    };
  }

  if (!snapshot.clientConfigured) {
    return {
      tone: "warning",
      title: "Twitch setup needed",
      detail: "Add a Twitch Client ID, then restart.",
      buttonLabel: "Setup needed",
      buttonDisabled: true,
    };
  }

  switch (snapshot.status) {
    case "notConfigured":
      return {
        tone: "warning",
        title: "Twitch setup needed",
        detail: "Add a Twitch Client ID, then restart.",
        buttonLabel: "Setup needed",
        buttonDisabled: true,
      };
    case "authorizing":
      return {
        tone: "working",
        title: snapshot.userCode ? `Enter code ${snapshot.userCode}` : "Authorize in Twitch",
        detail: snapshot.verificationUri ?? "Finish authorization in your browser.",
        buttonLabel: "Waiting",
        buttonDisabled: false,
      };
    case "connecting":
      return {
        tone: "working",
        title: "Connecting to Twitch",
        detail: "Starting the EventSub session.",
        buttonLabel: "Connecting",
        buttonDisabled: false,
      };
    case "connected":
      return {
        tone: "connected",
        title: snapshot.broadcasterDisplayName ? `Connected as ${snapshot.broadcasterDisplayName}` : "Twitch connected",
        detail: "EventSub is listening on this computer.",
        buttonLabel: "Disconnect",
        buttonDisabled: false,
      };
    case "reconnecting":
      return {
        tone: "working",
        title: "Reconnecting to Twitch",
        detail: "Your event settings will stay active.",
        buttonLabel: "Disconnect",
        buttonDisabled: false,
      };
    case "error":
      return {
        tone: "error",
        title: "Twitch connection failed",
        detail: visibleError ?? "Check your connection and try again.",
        buttonLabel: "Retry",
        buttonDisabled: false,
      };
    default:
      return {
        tone: "idle",
        title: "Twitch disconnected",
        detail: "Connect your channel to receive live events.",
        buttonLabel: "Connect",
        buttonDisabled: !snapshot.clientConfigured,
      };
  }
}

function errorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === "string" ? error : "Twitch action failed.";
}

function displayPayload(display: DisplayInfo): EngineCommandPayload {
  return {
    x: display.x,
    y: display.y,
    width: display.width,
    height: display.height,
  };
}
