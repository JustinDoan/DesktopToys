import {
  Copy,
  ExternalLink,
  Gamepad2,
  Image,
  Link2,
  MessageCircle,
  Minus,
  Monitor,
  MousePointer2,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Unplug,
  UserRoundX,
  UsersRound,
  Wifi,
  X,
} from "lucide-preact";
import { useEffect, useMemo, useState } from "preact/hooks";
import {
  defaultRoomFeatureFilters,
  defaultRoomSnapshot,
  DisplayInfo,
  endRoom,
  getDisplayLayout,
  getRoomSnapshot,
  hideRoomWindow,
  hostRoom,
  joinRoom,
  leaveRoom,
  minimizeRoomWindow,
  openRoomWindow,
  reconnectRoom,
  regenerateRoomInvite,
  removeRoomParticipant,
  RoomFeatureFilters,
  RoomParticipant,
  RoomSnapshot,
  setRoomFeatures,
  setRoomInteraction,
  setRoomParticipantInteraction,
  subscribeRoomSnapshot,
} from "./bridge";

type RoomState = {
  snapshot: RoomSnapshot;
  loaded: boolean;
  loadError: string | null;
  setSnapshot: (snapshot: RoomSnapshot) => void;
};

type RoomAction = () => Promise<RoomSnapshot>;
type FeatureKey = keyof RoomFeatureFilters;

const featureOptions: Array<{
  key: FeatureKey;
  label: string;
  detail: string;
  icon: typeof UsersRound;
}> = [
  { key: "objects", label: "Objects", detail: "Spawns, movement, and collisions", icon: UsersRound },
  { key: "twitch", label: "Twitch", detail: "Chat, emotes, and Bits", icon: MessageCircle },
  { key: "worldEffects", label: "World effects", detail: "Weather, tools, and screen effects", icon: Sparkles },
  { key: "gamesAndPortals", label: "Games and portals", detail: "Shared game and portal events", icon: Gamepad2 },
  { key: "importedAssets", label: "Imported assets", detail: "Custom models and transferred files", icon: Image },
];

export function useRoomState(): RoomState {
  const [snapshot, setSnapshot] = useState<RoomSnapshot>(defaultRoomSnapshot);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;

    void subscribeRoomSnapshot((next) => {
      if (active) {
        setSnapshot(next);
        setLoaded(true);
        setLoadError(null);
      }
    }).then((stop) => {
      if (active) {
        unsubscribe = stop;
      } else {
        stop();
      }
    }).catch((error) => {
      if (active) {
        setLoadError(errorMessage(error));
      }
    });

    void getRoomSnapshot()
      .then((next) => {
        if (active) {
          setSnapshot(next);
          setLoaded(true);
          setLoadError(null);
        }
      })
      .catch((error) => {
        if (active) {
          setLoaded(true);
          setLoadError(errorMessage(error));
        }
      });

    return () => {
      active = false;
      unsubscribe?.();
    };
  }, []);

  return { snapshot, loaded, loadError, setSnapshot };
}

type CompactRoomPanelProps = RoomState;

export function CompactRoomPanel({ snapshot, loaded, loadError, setSnapshot }: CompactRoomPanelProps) {
  const [pending, setPending] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [joinOpen, setJoinOpen] = useState(false);
  const [inviteUrl, setInviteUrl] = useState("");
  const status = roomStatusView(snapshot, actionError ?? loadError);

  async function run(key: string, action: RoomAction) {
    setPending(key);
    setActionError(null);
    try {
      setSnapshot(await action());
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setPending(null);
    }
  }

  async function copyInvite() {
    if (!snapshot.inviteUrl) {
      return;
    }
    try {
      await copyText(snapshot.inviteUrl);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch (error) {
      setActionError(errorMessage(error));
    }
  }

  if (!loaded) {
    return <div className="room-compact-loading" aria-label="Loading shared room" aria-busy="true" />;
  }

  if (snapshot.status === "idle") {
    if (joinOpen) {
      return (
        <div className="room-compact room-compact-join">
          <span className="room-compact-copy">
            <strong>Join shared room</strong>
            <small>Paste the host invite link.</small>
          </span>
          <div className="room-inline-join-row">
            <input
              aria-label="Room invite URL"
              value={inviteUrl}
              placeholder="http://.../join/..."
              onInput={(event) => setInviteUrl(event.currentTarget.value)}
              autoFocus
            />
            <button
              className="room-small-button bevel-button"
              type="button"
              disabled={pending !== null || !inviteUrl.trim()}
              onClick={() => void run("join", () => joinRoom({
                inviteUrl,
                displayName: savedDisplayName(),
                targetDisplayId: null,
              }))}
            >
              {pending === "join" ? "Joining" : "Join"}
            </button>
            <button className="room-icon-button bevel-button" type="button" aria-label="Cancel joining a room" onClick={() => setJoinOpen(false)}>
              <X size={13} />
            </button>
          </div>
          {actionError ? <span className="room-compact-error" role="alert">{actionError}</span> : null}
        </div>
      );
    }
    return (
      <div className="room-compact room-compact-idle">
        <span className="room-compact-icon status-idle" aria-hidden="true">
          <UsersRound size={17} strokeWidth={2.1} />
        </span>
        <span className="room-compact-copy">
          <strong>{status.title}</strong>
          <small>{status.detail}</small>
        </span>
        <div className="room-compact-actions">
          <button
            className="room-small-button bevel-button"
            type="button"
            disabled={pending !== null}
            onClick={() => void run("host", () => hostRoom({
              displayName: savedDisplayName(),
              targetDisplayId: null,
              allowGuestInteraction: true,
              featureFilters: defaultRoomFeatureFilters,
            }))}
          >
            Host
          </button>
          <button className="room-small-button bevel-button" type="button" onClick={() => { setActionError(null); setJoinOpen(true); }}>
            Join
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="room-compact">
      <span className={`room-compact-icon status-${status.tone}`} aria-hidden="true">
        {snapshot.status === "live" ? <Wifi size={17} strokeWidth={2.1} /> : <RefreshCw size={17} strokeWidth={2.1} />}
      </span>
      <span className="room-compact-copy" aria-live="polite">
        <strong>{status.title}</strong>
        <small>{compactRoomDetail(snapshot, status.detail)}</small>
      </span>
      <div className="room-compact-actions">
        {snapshot.role === "host" && snapshot.inviteUrl ? (
          <button className="room-icon-button bevel-button" type="button" aria-label="Copy room invite" onClick={() => void copyInvite()}>
            {copied ? <ShieldCheck size={14} /> : <Copy size={14} />}
          </button>
        ) : null}
        {snapshot.status === "reconnecting" || snapshot.status === "error" ? (
          <button className="room-small-button bevel-button" type="button" onClick={() => void run("reconnect", reconnectRoom)}>
            Retry
          </button>
        ) : null}
        <button className="room-icon-button bevel-button" type="button" aria-label="Open room console" onClick={() => void openRoomWindow()}>
          <ExternalLink size={14} />
        </button>
      </div>
      {snapshot.role === "host" ? (
        <label className="room-compact-permission" title="Let guests interact with shared objects">
          <MousePointer2 size={12} aria-hidden="true" />
          <input
            type="checkbox"
            checked={snapshot.allowGuestInteraction}
            disabled={pending !== null}
            onChange={(event) => void run("interaction", () => setRoomInteraction(event.currentTarget.checked))}
          />
          <i aria-hidden="true" />
        </label>
      ) : null}
    </div>
  );
}

export function RoomConsole() {
  const room = useRoomState();
  const { snapshot, loaded, loadError, setSnapshot } = room;
  const [displayName, setDisplayName] = useState(savedDisplayName());
  const [inviteUrl, setInviteUrl] = useState("");
  const [targetDisplayId, setTargetDisplayId] = useState("");
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [hostInteraction, setHostInteraction] = useState(true);
  const [hostFilters, setHostFilters] = useState<RoomFeatureFilters>({ ...defaultRoomFeatureFilters });
  const [pending, setPending] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const visibleError = actionError ?? loadError ?? snapshot.lastError;
  const status = roomStatusView(snapshot, visibleError);
  const active = snapshot.status !== "idle";

  useEffect(() => {
    void getDisplayLayout().then((layout) => {
      setDisplays(layout);
      const preferred = snapshot.targetDisplayId
        ?? window.localStorage.getItem("overlay.roomDisplayId")
        ?? layout.find((display) => display.primary)?.id
        ?? layout[0]?.id
        ?? "";
      setTargetDisplayId(preferred);
    }).catch(() => setDisplays([]));
  }, []);

  useEffect(() => {
    if (snapshot.localDisplayName) {
      setDisplayName(snapshot.localDisplayName);
    }
    if (snapshot.targetDisplayId) {
      setTargetDisplayId(snapshot.targetDisplayId);
    }
  }, [snapshot.localDisplayName, snapshot.targetDisplayId]);

  const enabledFeatureCount = useMemo(
    () => Object.values(snapshot.featureFilters).filter(Boolean).length,
    [snapshot.featureFilters],
  );

  async function run(key: string, action: RoomAction) {
    setPending(key);
    setActionError(null);
    try {
      const next = await action();
      setSnapshot(next);
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setPending(null);
    }
  }

  function rememberIdentity() {
    const normalized = displayName.trim() || "Desktop friend";
    window.localStorage.setItem("overlay.roomDisplayName", normalized);
    if (targetDisplayId) {
      window.localStorage.setItem("overlay.roomDisplayId", targetDisplayId);
    }
    return normalized;
  }

  function createRoom() {
    const normalized = rememberIdentity();
    return run("host", () => hostRoom({
      displayName: normalized,
      targetDisplayId: targetDisplayId || null,
      allowGuestInteraction: hostInteraction,
      featureFilters: hostFilters,
    }));
  }

  function joinInvitedRoom() {
    const normalized = rememberIdentity();
    return run("join", () => joinRoom({
      inviteUrl,
      displayName: normalized,
      targetDisplayId: targetDisplayId || null,
    }));
  }

  async function copyInvite() {
    if (!snapshot.inviteUrl) {
      return;
    }
    try {
      await copyText(snapshot.inviteUrl);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      setActionError(errorMessage(error));
    }
  }

  function updateFeature(feature: FeatureKey, enabled: boolean) {
    const next = { ...snapshot.featureFilters, [feature]: enabled };
    return run(`feature-${feature}`, () => setRoomFeatures(next));
  }

  return (
    <main className="room-shell" data-status={snapshot.status}>
      <section className="room-console" aria-label="Shared room console">
        <header className="room-console-header" data-tauri-drag-region>
          <div className="room-console-brand" data-tauri-drag-region>
            <span className={`room-console-mark status-${status.tone}`} aria-hidden="true">
              <UsersRound size={17} strokeWidth={2.1} />
            </span>
            <span data-tauri-drag-region>
              <strong>Shared Room</strong>
              <small>{active ? status.title : "Connect two desktop overlays"}</small>
            </span>
          </div>
          <div className="window-controls">
            <button className="window-button bevel-button" type="button" aria-label="Minimize room console" onClick={() => void minimizeRoomWindow()}>
              <Minus size={14} strokeWidth={2.6} />
            </button>
            <button className="window-button bevel-button" type="button" aria-label="Hide room console" onClick={() => void hideRoomWindow()}>
              <X size={15} strokeWidth={2.4} />
            </button>
          </div>
        </header>

        {!loaded ? (
          <div className="room-console-loading" aria-label="Loading room controls" aria-busy="true">
            <span />
            <span />
            <span />
          </div>
        ) : !active ? (
          <div className="room-console-scroll room-setup-scroll">
            <section className="room-console-section room-identity-section">
              <SectionHeading icon={Monitor} title="This desktop" detail="Choose how you appear and where shared objects land." />
              <div className="room-form-grid">
                <label className="room-field">
                  <span>Display name</span>
                  <input
                    value={displayName}
                    maxLength={32}
                    placeholder="Desktop friend"
                    onInput={(event) => setDisplayName(event.currentTarget.value)}
                  />
                </label>
                <label className="room-field">
                  <span>Target display</span>
                  <select value={targetDisplayId} onChange={(event) => setTargetDisplayId(event.currentTarget.value)}>
                    {displays.length === 0 ? <option value="">Primary display</option> : null}
                    {displays.map((display, index) => (
                      <option value={display.id} key={display.id}>
                        {index + 1}. {display.label} ({display.width} x {display.height})
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </section>

            <section className="room-console-section room-host-section">
              <SectionHeading icon={ShieldCheck} title="Host a room" detail="Create a private invite and keep this desktop authoritative." />
              <label className="room-console-toggle">
                <span>
                  <strong>Allow guest interaction</strong>
                  <small>Guests can grab and move shared objects.</small>
                </span>
                <input type="checkbox" checked={hostInteraction} onChange={(event) => setHostInteraction(event.currentTarget.checked)} />
                <i aria-hidden="true" />
              </label>
              <div className="room-preset-row" aria-label="Initial sharing preset">
                <button
                  className={allFeaturesEnabled(hostFilters) ? "selected bevel-button" : "bevel-button"}
                  type="button"
                  onClick={() => setHostFilters({ objects: true, twitch: true, worldEffects: true, gamesAndPortals: true, importedAssets: true })}
                >
                  Everything
                </button>
                <button
                  className={objectsOnly(hostFilters) ? "selected bevel-button" : "bevel-button"}
                  type="button"
                  onClick={() => setHostFilters({ objects: true, twitch: false, worldEffects: false, gamesAndPortals: false, importedAssets: false })}
                >
                  Objects only
                </button>
                <button className="room-primary-button bevel-button" type="button" disabled={pending !== null} onClick={() => void createRoom()}>
                  {pending === "host" ? "Creating" : "Create room"}
                </button>
              </div>
            </section>

            <section className="room-console-section room-join-section">
              <SectionHeading icon={Link2} title="Join by invite" detail="Paste the room URL sent by the host." />
              <div className="room-join-row">
                <input
                  aria-label="Room invite URL"
                  value={inviteUrl}
                  placeholder="https://.../join/..."
                  onInput={(event) => setInviteUrl(event.currentTarget.value)}
                />
                <button className="room-primary-button bevel-button" type="button" disabled={pending !== null || !inviteUrl.trim()} onClick={() => void joinInvitedRoom()}>
                  {pending === "join" ? "Joining" : "Join room"}
                </button>
              </div>
            </section>

            {visibleError ? <RoomError message={visibleError} /> : null}
          </div>
        ) : (
          <div className="room-console-scroll">
            <section className={`room-status-banner status-${status.tone}`} aria-live="polite">
              <span className="room-status-symbol" aria-hidden="true">
                {snapshot.status === "live" ? <Wifi size={18} /> : <RefreshCw size={18} />}
              </span>
              <span>
                <strong>{status.title}</strong>
                <small>{status.detail}</small>
              </span>
              <span className="room-status-metric">{healthMetric(snapshot)}</span>
              {snapshot.status === "reconnecting" || snapshot.status === "error" ? (
                <button className="room-small-button bevel-button" type="button" disabled={pending !== null} onClick={() => void run("reconnect", reconnectRoom)}>
                  Reconnect
                </button>
              ) : null}
            </section>

            {snapshot.role === "host" && snapshot.inviteUrl ? (
              <section className="room-console-section">
                <SectionHeading icon={Link2} title="Invite" detail="Anyone with this link can join this prototype room." />
                <div className="room-invite-row">
                  <input aria-label="Room invite URL" readOnly value={snapshot.inviteUrl} onFocus={(event) => event.currentTarget.select()} />
                  <button className="room-icon-button bevel-button" type="button" aria-label="Copy invite URL" onClick={() => void copyInvite()}>
                    {copied ? <ShieldCheck size={15} /> : <Copy size={15} />}
                  </button>
                  <button
                    className="room-icon-button bevel-button"
                    type="button"
                    aria-label="Regenerate invite URL"
                    disabled={pending !== null}
                    onClick={() => void run("regenerate", regenerateRoomInvite)}
                  >
                    <RefreshCw size={15} />
                  </button>
                </div>
              </section>
            ) : null}

            <section className="room-console-section">
              <SectionHeading
                icon={UsersRound}
                title={`Participants (${snapshot.participants.length})`}
                detail={snapshot.role === "host" ? "Control who can interact with the shared scene." : "The host controls room permissions."}
              />
              <div className="room-participant-list">
                {snapshot.participants.map((participant) => (
                  <ParticipantRow
                    key={participant.id}
                    participant={participant}
                    local={participant.id === "local"}
                    hostControls={snapshot.role === "host" && participant.role !== "host"}
                    disabled={pending !== null}
                    onInteraction={(enabled) => void run(`participant-${participant.id}`, () => setRoomParticipantInteraction(participant.id, enabled))}
                    onRemove={() => void run(`remove-${participant.id}`, () => removeRoomParticipant(participant.id))}
                  />
                ))}
              </div>
            </section>

            <section className="room-console-section">
              <SectionHeading icon={MousePointer2} title="Interaction" detail="Remote input is validated by the host before it changes the shared scene." />
              <label className={`room-console-toggle ${snapshot.role !== "host" ? "read-only" : ""}`}>
                <span>
                  <strong>{snapshot.role === "host" ? "Allow guest interaction" : "Interaction permission"}</strong>
                  <small>{snapshot.allowGuestInteraction ? "Guests may manipulate shared objects." : "The room is view-only for guests."}</small>
                </span>
                <input
                  type="checkbox"
                  checked={snapshot.allowGuestInteraction}
                  disabled={snapshot.role !== "host" || pending !== null}
                  onChange={(event) => void run("interaction", () => setRoomInteraction(event.currentTarget.checked))}
                />
                <i aria-hidden="true" />
              </label>
            </section>

            <section className="room-console-section">
              <SectionHeading icon={Sparkles} title={`Shared content (${enabledFeatureCount}/5)`} detail="Host filters control what leaves the authoritative desktop." />
              <div className="room-feature-list">
                {featureOptions.map((feature) => {
                  const Icon = feature.icon;
                  const checked = snapshot.featureFilters[feature.key];
                  return (
                    <label className={`room-feature-row ${checked ? "enabled" : ""}`} key={feature.key}>
                      <Icon size={16} strokeWidth={2} aria-hidden="true" />
                      <span>
                        <strong>{feature.label}</strong>
                        <small>{feature.detail}</small>
                      </span>
                      <input
                        type="checkbox"
                        checked={checked}
                        disabled={snapshot.role !== "host" || pending !== null}
                        onChange={(event) => void updateFeature(feature.key, event.currentTarget.checked)}
                      />
                      <i aria-hidden="true" />
                    </label>
                  );
                })}
              </div>
            </section>

            <section className="room-console-section room-health-section">
              <SectionHeading icon={Wifi} title="Connection" detail="The room service stays active while these controls are hidden." />
              <dl className="room-health-grid">
                <HealthItem label="Relay" value={snapshot.relayRegion ?? "Connecting"} />
                <HealthItem label="Latency" value={snapshot.latencyMs === null ? "Waiting" : `${snapshot.latencyMs} ms`} />
                <HealthItem label="Last update" value={snapshot.lastPacketAgeMs === null ? "Waiting" : `${snapshot.lastPacketAgeMs} ms ago`} />
                <HealthItem label="Mode" value={snapshot.localSimulation ? "Local simulation" : "Internet relay"} />
              </dl>
            </section>

            {visibleError ? <RoomError message={visibleError} /> : null}
          </div>
        )}

        <footer className="room-console-footer">
          <span>
            {active ? `${snapshot.role === "host" ? "Hosting" : "Joined"} ${snapshot.roomId ?? "room"}` : "Private shared-room prototype"}
          </span>
          {active ? (
            <button
              className="room-danger-button bevel-button"
              type="button"
              disabled={pending !== null}
              onClick={() => {
                const leaving = snapshot.role === "host" ? endRoom : leaveRoom;
                const confirmed = snapshot.role !== "host" || window.confirm("End this shared room for everyone?");
                if (confirmed) {
                  void run("leave", leaving);
                }
              }}
            >
              <Unplug size={14} />
              {snapshot.role === "host" ? "End room" : "Leave room"}
            </button>
          ) : null}
        </footer>
      </section>
    </main>
  );
}

type SectionHeadingProps = {
  icon: typeof UsersRound;
  title: string;
  detail: string;
};

function SectionHeading({ icon: Icon, title, detail }: SectionHeadingProps) {
  return (
    <header className="room-section-heading">
      <Icon size={16} strokeWidth={2.1} aria-hidden="true" />
      <span>
        <strong>{title}</strong>
        <small>{detail}</small>
      </span>
    </header>
  );
}

type ParticipantRowProps = {
  participant: RoomParticipant;
  local: boolean;
  hostControls: boolean;
  disabled: boolean;
  onInteraction: (enabled: boolean) => void;
  onRemove: () => void;
};

function ParticipantRow({ participant, local, hostControls, disabled, onInteraction, onRemove }: ParticipantRowProps) {
  return (
    <div className="room-participant-row">
      <span className={`participant-signal status-${participant.status}`} aria-hidden="true" />
      <span className="participant-copy">
        <strong>{participant.displayName}{local ? " (you)" : ""}</strong>
        <small>{participant.role === "host" ? "Host" : participant.canInteract ? "Controller" : "Viewer"}</small>
      </span>
      <span className="participant-latency">{participant.latencyMs === null ? "--" : `${participant.latencyMs} ms`}</span>
      {hostControls ? (
        <>
          <label className="participant-toggle" title="Allow interaction">
            <input type="checkbox" checked={participant.canInteract} disabled={disabled} onChange={(event) => onInteraction(event.currentTarget.checked)} />
            <i aria-hidden="true" />
          </label>
          <button className="room-icon-button bevel-button" type="button" disabled={disabled} aria-label={`Remove ${participant.displayName}`} onClick={onRemove}>
            <UserRoundX size={14} />
          </button>
        </>
      ) : (
        <ShieldCheck className="participant-role-icon" size={14} aria-label={participant.role === "host" ? "Host" : "Participant"} />
      )}
    </div>
  );
}

function HealthItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function RoomError({ message }: { message: string }) {
  return (
    <div className="room-error" role="alert">
      <Unplug size={15} aria-hidden="true" />
      <span>{message}</span>
    </div>
  );
}

function roomStatusView(snapshot: RoomSnapshot, visibleError: string | null) {
  if (visibleError) {
    return { tone: "error", title: "Room action failed", detail: visibleError };
  }
  switch (snapshot.status) {
    case "creating":
      return { tone: "working", title: "Creating room", detail: "Preparing a private invite." };
    case "joining":
      return { tone: "working", title: "Joining room", detail: "Contacting the room host." };
    case "syncing":
      return { tone: "working", title: "Syncing scene", detail: "Receiving the authoritative scene." };
    case "live":
      return { tone: "connected", title: snapshot.role === "host" ? "Room live" : "Connected to room", detail: "Shared scene updates are active." };
    case "degraded":
      return { tone: "warning", title: "Connection degraded", detail: "Updates may appear delayed." };
    case "reconnecting":
      return { tone: "working", title: "Reconnecting", detail: snapshot.retryInMs ? `Retrying in ${Math.ceil(snapshot.retryInMs / 1000)} seconds.` : "Restoring the room connection." };
    case "error":
      return { tone: "error", title: "Room disconnected", detail: snapshot.lastError ?? "Reconnect or leave the room." };
    default:
      return { tone: "idle", title: "No shared room", detail: "Host one or join with an invite." };
  }
}

function compactRoomDetail(snapshot: RoomSnapshot, fallback: string) {
  if (snapshot.status !== "live") {
    return fallback;
  }
  const people = `${snapshot.participants.length} ${snapshot.participants.length === 1 ? "person" : "people"}`;
  const latency = snapshot.latencyMs === null ? "waiting" : `${snapshot.latencyMs} ms`;
  return `${people}, ${latency}`;
}

function healthMetric(snapshot: RoomSnapshot) {
  if (snapshot.status === "reconnecting" && snapshot.retryInMs) {
    return `${Math.ceil(snapshot.retryInMs / 1000)}s`;
  }
  return snapshot.latencyMs === null ? "--" : `${snapshot.latencyMs} ms`;
}

function allFeaturesEnabled(filters: RoomFeatureFilters) {
  return Object.values(filters).every(Boolean);
}

function objectsOnly(filters: RoomFeatureFilters) {
  return filters.objects && !filters.twitch && !filters.worldEffects && !filters.gamesAndPortals && !filters.importedAssets;
}

function savedDisplayName() {
  return window.localStorage.getItem("overlay.roomDisplayName") ?? "Desktop friend";
}

async function copyText(value: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }
  const input = document.createElement("textarea");
  input.value = value;
  input.style.position = "fixed";
  input.style.opacity = "0";
  document.body.appendChild(input);
  input.select();
  const copied = document.execCommand("copy");
  input.remove();
  if (!copied) {
    throw new Error("Copy failed. Select the invite URL and copy it manually.");
  }
}

function errorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === "string" ? error : "Room action failed.";
}
