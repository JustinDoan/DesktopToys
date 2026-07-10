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
  Minus,
  RotateCcw,
  Sparkles,
  Target,
  Triangle,
  Workflow,
  X,
} from "lucide-preact";
import {
  dispatchEngineCommand,
  EngineCommandPayload,
  EngineSnapshot,
  hideOverlay,
  minimizeOverlay,
  RuntimeSettings,
  getEngineSnapshot,
} from "./bridge";

type Tone = "steel" | "cyan" | "amber" | "violet" | "green";
type SpawnVariant = {
  label: string;
  kind?: string;
  command?: string;
  icon: typeof Box;
  tone: Tone;
  shader?: boolean;
};

type ActiveTab = "objects" | "labs";
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
    id: "cheers",
    label: "Cheers",
    actions: [
      { label: "Cheer 10", shortLabel: "10 Bits", command: "simulate_twitch_cheer", payload: { bits: 10, donor: "PixelGoblin", message: "nice!" }, icon: Gem, tone: "violet" },
      { label: "Cheer 100", shortLabel: "100 Bits", command: "simulate_twitch_cheer", payload: { bits: 100, donor: "GoblinFan42", message: "LET'S GO!!!" }, icon: Gem, tone: "cyan" },
      { label: "Cheer 1000", shortLabel: "1K Bits", command: "simulate_twitch_cheer", payload: { bits: 1000, donor: "CrystalWhale", message: "BIG DROP!!!!!" }, icon: Sparkles, tone: "violet" },
      { label: "Cheer 5000", shortLabel: "5K Bits", command: "simulate_twitch_cheer", payload: { bits: 5000, donor: "MeteorPatron", message: "CHAOS!!!!!!!!" }, icon: Sparkles, tone: "amber" },
      { label: "Anonymous 500", shortLabel: "Anon", command: "simulate_twitch_cheer", payload: { bits: 500, anonymous: true, message: "???" }, icon: Gem, tone: "steel" },
    ],
  },
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
const objectBars = [22, 38, 52, 72, 88, 66, 58, 42, 34, 29, 38, 44, 31, 27, 25, 23, 22, 21];

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
  fps: 0,
  objectCount: 7,
  queuedCommands: 0,
  lastCommand: null,
  commandLog: [],
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

  useEffect(() => {
    getEngineSnapshot().then(applySnapshot).catch(() => applySnapshot(defaultSnapshot));
  }, []);

  useEffect(() => {
    if (!settingsOpen) {
      setSettingsDraft(snapshot.settings);
    }
  }, [settingsOpen, snapshot.settings]);

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
  const transportText = formatTransport(snapshot.transport);

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
    });
  }

  function runLabAction(action: LabAction = selectedLabAction) {
    return send(action.command, action.payload ?? {});
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
          <div className="object-bars" aria-hidden="true">
            {objectBars.map((height, index) => (
              <span key={index} style={{ height: `${height}%` }} />
            ))}
          </div>
          <span className="transport-chip">{transportText}</span>
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
            </div>
          </FrameSection>
        ) : (
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
        )}

        <section className="control-strip" aria-label="Quick controls">
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

function formatTransport(transport: string) {
  if (transport.toLowerCase().includes("browser")) {
    return "Preview";
  }

  if (transport.toLowerCase().includes("loading")) {
    return "Loading";
  }

  return transport.split("_").join(" ");
}
