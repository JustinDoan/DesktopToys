//! Local Twitch Device OAuth + EventSub WebSocket service.
//!
//! The webview only sees status and the short-lived device code. Access and refresh
//! tokens stay in Windows Credential Manager. Network work and renderer IPC delivery
//! run on separate bounded worker threads so chat floods cannot touch the UI or render
//! loops and slow renderer IPC cannot stall Twitch keepalives.

use std::{
    collections::{HashMap, VecDeque},
    net::TcpStream,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError, TrySendError};
use reqwest::blocking::{Client as HttpClient, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tungstenite::{stream::MaybeTlsStream, Message};

const DEVICE_URL: &str = "https://id.twitch.tv/oauth2/device";
const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const SUBSCRIPTIONS_URL: &str = "https://api.twitch.tv/helix/eventsub/subscriptions";
const EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";
const REQUIRED_SCOPES: [&str; 2] = ["user:read:chat", "bits:read"];
const VALIDATE_INTERVAL: Duration = Duration::from_secs(60 * 60);
const WELCOME_TIMEOUT: Duration = Duration::from_secs(11);
const KEEPALIVE_GRACE: Duration = Duration::from_secs(2);
const MESSAGE_ID_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_SEEN_MESSAGE_IDS: usize = 4096;
const COMMAND_CAPACITY: usize = 32;
const CHAT_CAPACITY: usize = 256;
const PRIORITY_CAPACITY: usize = 64;
const CREDENTIAL_SERVICE: &str = "ScreenOverlayPhysics";
const CREDENTIAL_USER: &str = "twitch-oauth";
type RendererForwarder = dyn Fn(&str, &Value) -> Result<(), String> + Send + Sync;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchSnapshot {
    pub status: String,
    pub client_configured: bool,
    pub broadcaster_display_name: Option<String>,
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
    pub chat_enabled: bool,
    pub bits_enabled: bool,
    pub last_error: Option<String>,
    pub chat_messages_received: u64,
    pub bits_events_received: u64,
}

impl TwitchSnapshot {
    fn new(client_configured: bool) -> Self {
        Self {
            status: if client_configured {
                "disconnected"
            } else {
                "notConfigured"
            }
            .to_string(),
            client_configured,
            broadcaster_display_name: None,
            user_code: None,
            verification_uri: None,
            chat_enabled: true,
            bits_enabled: true,
            last_error: if client_configured {
                None
            } else {
                Some("Set SCREEN_OVERLAY_TWITCH_CLIENT_ID, or compile with TWITCH_CLIENT_ID, then restart.".to_string())
            },
            chat_messages_received: 0,
            bits_events_received: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FeatureFlags {
    chat: bool,
    bits: bool,
}

enum Command {
    Connect,
    ConnectSaved,
    Disconnect,
    SetFeatures(FeatureFlags),
}

#[derive(Debug)]
enum RendererEvent {
    Chat(ChatMessage),
    Bits(BitsUse),
}

#[derive(Debug)]
struct ChatMessage {
    id: String,
    chatter_id: String,
    chatter_login: String,
    chatter_name: String,
    text: String,
    color: Option<String>,
    fragments: Vec<ChatFragment>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatFragment {
    text: String,
    emote_id: Option<String>,
    animated: bool,
}

#[derive(Debug)]
struct BitsUse {
    user_id: Option<String>,
    user_login: Option<String>,
    user_name: Option<String>,
    bits: u32,
    kind: String,
    message: Option<String>,
}

impl RendererEvent {
    fn command_and_payload(self) -> (&'static str, Value) {
        match self {
            Self::Chat(message) => (
                "twitch_chat_message",
                json!({
                    "messageId": message.id,
                    "userId": message.chatter_id,
                    "userLogin": message.chatter_login,
                    "userName": message.chatter_name,
                    "donor": message.chatter_name,
                    "text": message.text,
                    "message": message.text,
                    "color": message.color,
                    "fragments": message.fragments,
                }),
            ),
            Self::Bits(event) => (
                "twitch_bits_event",
                json!({
                    "userId": event.user_id,
                    "userLogin": event.user_login,
                    "donor": event.user_name,
                    "anonymous": event.user_name.is_none(),
                    "bits": event.bits,
                    "kind": event.kind,
                    "message": event.message,
                }),
            ),
        }
    }
}

pub struct TwitchService {
    snapshot: Arc<Mutex<TwitchSnapshot>>,
    commands: Sender<Command>,
}

impl TwitchService {
    pub fn start(
        client_id: Option<String>,
        forward_to_renderer: impl Fn(&str, &Value) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        let client_id = client_id.filter(|value| !value.trim().is_empty());
        let snapshot = Arc::new(Mutex::new(TwitchSnapshot::new(client_id.is_some())));
        let (commands_tx, commands_rx) = bounded(COMMAND_CAPACITY);
        let (priority_tx, priority_rx) = bounded(PRIORITY_CAPACITY);
        let (chat_tx, chat_rx) = bounded(CHAT_CAPACITY);

        let forward_to_renderer = Arc::new(forward_to_renderer);
        thread::Builder::new()
            .name("twitch-renderer-dispatch".to_string())
            .spawn(move || dispatch_renderer_events(priority_rx, chat_rx, forward_to_renderer))
            .expect("failed to start Twitch renderer dispatcher");

        let worker_snapshot = Arc::clone(&snapshot);
        thread::Builder::new()
            .name("twitch-eventsub".to_string())
            .spawn(move || {
                Worker::new(
                    client_id,
                    worker_snapshot,
                    commands_rx,
                    priority_tx,
                    chat_tx,
                )
                .run()
            })
            .expect("failed to start Twitch EventSub worker");

        let service = Self {
            snapshot,
            commands: commands_tx,
        };
        if service.snapshot().client_configured {
            let _ = service.commands.try_send(Command::ConnectSaved);
        }
        service
    }

    pub fn snapshot(&self) -> TwitchSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn connect(&self) -> Result<TwitchSnapshot, String> {
        {
            let mut snapshot = self
                .snapshot
                .lock()
                .map_err(|_| "Twitch state lock was poisoned".to_string())?;
            if !snapshot.client_configured {
                return Ok(snapshot.clone());
            }
            snapshot.status = "authorizing".to_string();
            snapshot.user_code = None;
            snapshot.verification_uri = None;
            snapshot.last_error = None;
        }
        self.send(Command::Connect)?;
        Ok(self.snapshot())
    }

    pub fn disconnect(&self) -> Result<TwitchSnapshot, String> {
        self.send(Command::Disconnect)?;
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Twitch state lock was poisoned".to_string())?;
        snapshot.status = if snapshot.client_configured {
            "disconnected"
        } else {
            "notConfigured"
        }
        .to_string();
        snapshot.broadcaster_display_name = None;
        snapshot.user_code = None;
        snapshot.verification_uri = None;
        snapshot.last_error = None;
        Ok(snapshot.clone())
    }

    pub fn set_features(&self, chat: bool, bits: bool) -> Result<TwitchSnapshot, String> {
        self.send(Command::SetFeatures(FeatureFlags { chat, bits }))?;
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Twitch state lock was poisoned".to_string())?;
        snapshot.chat_enabled = chat;
        snapshot.bits_enabled = bits;
        Ok(snapshot.clone())
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => "Twitch command queue is busy; try again.".to_string(),
                TrySendError::Disconnected(_) => "Twitch worker is unavailable.".to_string(),
            })
    }
}

fn dispatch_renderer_events(
    priority: Receiver<RendererEvent>,
    chat: Receiver<RendererEvent>,
    forward: Arc<RendererForwarder>,
) {
    loop {
        let event = match priority.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Disconnected) => match chat.recv() {
                Ok(event) => event,
                Err(_) => return,
            },
            Err(TryRecvError::Empty) => crossbeam_channel::select_biased! {
                recv(priority) -> event => match event { Ok(event) => event, Err(_) => continue },
                recv(chat) -> event => match event { Ok(event) => event, Err(_) => continue },
            },
        };
        let (command, payload) = event.command_and_payload();
        let _ = forward(command, &payload);
    }
}

struct Worker {
    client_id: Option<String>,
    snapshot: Arc<Mutex<TwitchSnapshot>>,
    commands: Receiver<Command>,
    priority_events: Sender<RendererEvent>,
    chat_events: Sender<RendererEvent>,
    http: HttpClient,
    features: FeatureFlags,
    tokens: Option<AuthTokens>,
    identity: Option<Identity>,
    subscriptions: ActiveSubscriptions,
    seen_ids: MessageDeduper,
}

impl Worker {
    fn new(
        client_id: Option<String>,
        snapshot: Arc<Mutex<TwitchSnapshot>>,
        commands: Receiver<Command>,
        priority_events: Sender<RendererEvent>,
        chat_events: Sender<RendererEvent>,
    ) -> Self {
        let features = {
            let state = snapshot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            FeatureFlags {
                chat: state.chat_enabled,
                bits: state.bits_enabled,
            }
        };
        let http = HttpClient::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(8))
            .user_agent("ScreenOverlayPhysics/0.1 TwitchEventSub")
            .build()
            .expect("static HTTP client configuration should be valid");
        Self {
            client_id,
            snapshot,
            commands,
            priority_events,
            chat_events,
            http,
            features,
            tokens: None,
            identity: None,
            subscriptions: ActiveSubscriptions::default(),
            seen_ids: MessageDeduper::default(),
        }
    }

    fn run(mut self) {
        let mut should_connect = false;
        let mut reconnect_url: Option<String> = None;
        let mut reconnect_attempt = 0u32;
        loop {
            if !should_connect {
                match self.commands.recv() {
                    Ok(Command::Connect) => match load_tokens() {
                        Ok(Some(tokens)) => {
                            self.tokens = Some(tokens);
                            match self.ensure_valid_token() {
                                Ok(()) => should_connect = true,
                                Err(_) => match self.authorize_device() {
                                    Ok(true) => should_connect = true,
                                    Ok(false) => {}
                                    Err(error) => self.set_error(error),
                                },
                            }
                        }
                        Ok(None) => match self.authorize_device() {
                            Ok(true) => should_connect = true,
                            Ok(false) => {}
                            Err(error) => self.set_error(error),
                        },
                        Err(error) => self.set_error(error),
                    },
                    Ok(Command::ConnectSaved) => match load_tokens() {
                        Ok(Some(tokens)) => {
                            self.tokens = Some(tokens);
                            should_connect = true;
                        }
                        Ok(None) => self.set_status("disconnected"),
                        Err(error) => self.set_error(error),
                    },
                    Ok(Command::Disconnect) => self.set_disconnected(),
                    Ok(Command::SetFeatures(features)) => self.features = features,
                    Err(_) => return,
                }
                continue;
            }

            if let Err(error) = self.ensure_valid_token() {
                self.set_error(error);
                should_connect = false;
                continue;
            }
            self.set_status(if reconnect_attempt == 0 {
                "connecting"
            } else {
                "reconnecting"
            });

            if reconnect_attempt > 0 && reconnect_url.is_none() {
                match self
                    .commands
                    .recv_timeout(reconnect_delay(reconnect_attempt))
                {
                    Ok(Command::Disconnect) => {
                        should_connect = false;
                        self.set_disconnected();
                        continue;
                    }
                    Ok(Command::SetFeatures(features)) => self.features = features,
                    Ok(Command::Connect)
                    | Ok(Command::ConnectSaved)
                    | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }

            let url = reconnect_url
                .take()
                .unwrap_or_else(|| EVENTSUB_URL.to_string());
            let migrating = url != EVENTSUB_URL;
            match self.run_socket(&url, migrating) {
                SocketOutcome::Disconnected => {
                    should_connect = false;
                    self.set_disconnected();
                }
                SocketOutcome::Reconnect(url) => {
                    reconnect_url = Some(url);
                    reconnect_attempt = 0;
                }
                SocketOutcome::ConnectionLost(error) => {
                    self.subscriptions = ActiveSubscriptions::default();
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    self.with_snapshot(|snapshot| {
                        snapshot.status = "reconnecting".to_string();
                        snapshot.last_error = Some(format!("Connection lost: {error}"));
                    });
                }
            }
        }
    }

    fn authorize_device(&mut self) -> Result<bool, String> {
        let client_id = self.client_id()?.to_string();
        self.with_snapshot(|snapshot| {
            snapshot.status = "authorizing".to_string();
            snapshot.user_code = None;
            snapshot.verification_uri = None;
            snapshot.last_error = None;
        });
        let scopes = REQUIRED_SCOPES.join(" ");
        let response = self
            .http
            .post(DEVICE_URL)
            .form(&[
                ("client_id", client_id.as_str()),
                ("scopes", scopes.as_str()),
            ])
            .send()
            .map_err(|error| format!("Could not start Twitch authorization: {error}"))?;
        let device: DeviceCodeResponse = decode_success(response, "start Twitch authorization")?;
        self.with_snapshot(|snapshot| {
            snapshot.user_code = Some(device.user_code.clone());
            snapshot.verification_uri = Some(device.verification_uri.clone());
        });
        let _ = open::that_detached(&device.verification_uri);

        let expires_at = Instant::now() + Duration::from_secs(device.expires_in);
        let mut interval = Duration::from_secs(device.interval.max(1));
        while Instant::now() < expires_at {
            match self.commands.recv_timeout(interval) {
                Ok(Command::Disconnect) => {
                    self.set_disconnected();
                    return Ok(false);
                }
                Ok(Command::SetFeatures(features)) => {
                    self.features = features;
                    continue;
                }
                Ok(Command::Connect) | Ok(Command::ConnectSaved) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(false),
                Err(RecvTimeoutError::Timeout) => {}
            }
            match self.poll_device_token(&client_id, &device.device_code)? {
                TokenPoll::Pending => {}
                TokenPoll::SlowDown => interval += Duration::from_secs(5),
                TokenPoll::Denied(error) => return Err(error),
                TokenPoll::Granted(tokens) => {
                    save_tokens(&tokens)?;
                    self.tokens = Some(tokens);
                    self.with_snapshot(|snapshot| {
                        snapshot.status = "connecting".to_string();
                        snapshot.user_code = None;
                        snapshot.verification_uri = None;
                    });
                    return Ok(true);
                }
            }
        }
        Err("Twitch authorization code expired. Choose Connect to try again.".to_string())
    }

    fn poll_device_token(&self, client_id: &str, device_code: &str) -> Result<TokenPoll, String> {
        let response = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|error| format!("Could not check Twitch authorization: {error}"))?;
        if response.status().is_success() {
            let token: TokenResponse = response
                .json()
                .map_err(|error| format!("Invalid Twitch token response: {error}"))?;
            return Ok(TokenPoll::Granted(AuthTokens {
                access_token: token.access_token,
                refresh_token: token
                    .refresh_token
                    .ok_or_else(|| "Twitch did not return a refresh token".to_string())?,
                scopes: token.scope,
            }));
        }
        let status = response.status();
        let body: ApiError = response.json().unwrap_or(ApiError {
            message: status.to_string(),
        });
        Ok(match body.message.as_str() {
            "authorization_pending" => TokenPoll::Pending,
            "slow_down" => TokenPoll::SlowDown,
            "access_denied" => TokenPoll::Denied("Twitch authorization was declined.".to_string()),
            "expired_token" => TokenPoll::Denied("Twitch authorization code expired.".to_string()),
            _ => TokenPoll::Denied(format!("Twitch authorization failed: {}", body.message)),
        })
    }

    fn ensure_valid_token(&mut self) -> Result<(), String> {
        let client_id = self.client_id()?.to_string();
        let tokens = self
            .tokens
            .clone()
            .ok_or_else(|| "No Twitch account is authorized.".to_string())?;
        let identity = match self.validate(&tokens.access_token) {
            Ok(identity) => identity,
            Err(_) => {
                let refreshed = self.refresh(&client_id, &tokens.refresh_token)?;
                save_tokens(&refreshed)?;
                self.tokens = Some(refreshed.clone());
                self.validate(&refreshed.access_token)?
            }
        };
        if identity.client_id != client_id {
            return Err(
                "Saved Twitch authorization belongs to a different Client ID. Connect again."
                    .to_string(),
            );
        }
        let missing: Vec<&str> = REQUIRED_SCOPES
            .iter()
            .filter(|scope| !identity.scopes.iter().any(|granted| granted == **scope))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Twitch authorization is missing {}. Connect again.",
                missing.join(", ")
            ));
        }
        self.identity = Some(identity);
        Ok(())
    }

    fn validate(&self, access_token: &str) -> Result<Identity, String> {
        let response = self
            .http
            .get(VALIDATE_URL)
            .header("Authorization", format!("OAuth {access_token}"))
            .send()
            .map_err(|error| format!("Could not validate Twitch account: {error}"))?;
        decode_success(response, "validate Twitch account")
    }

    fn refresh(&self, client_id: &str, refresh_token: &str) -> Result<AuthTokens, String> {
        let response = self
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", client_id),
            ])
            .send()
            .map_err(|error| format!("Could not refresh Twitch account: {error}"))?;
        let token: TokenResponse = decode_success(response, "refresh Twitch account")?;
        Ok(AuthTokens {
            access_token: token.access_token,
            refresh_token: token
                .refresh_token
                .unwrap_or_else(|| refresh_token.to_string()),
            scopes: token.scope,
        })
    }

    fn run_socket(&mut self, url: &str, migrating: bool) -> SocketOutcome {
        let url = match url::Url::parse(url) {
            Ok(url) => url,
            Err(error) => {
                return SocketOutcome::ConnectionLost(format!("invalid reconnect URL: {error}"))
            }
        };
        let (mut socket, _) = match tungstenite::connect(url.as_str()) {
            Ok(connection) => connection,
            Err(error) => return SocketOutcome::ConnectionLost(error.to_string()),
        };
        if let Err(error) = set_socket_timeout(socket.get_mut(), Duration::from_millis(200)) {
            return SocketOutcome::ConnectionLost(format!("could not configure socket: {error}"));
        }
        let mut session_id: Option<String> = None;
        let mut last_validation = Instant::now();
        let connected_at = Instant::now();
        let mut last_server_message = connected_at;
        let mut keepalive_timeout: Option<Duration> = None;

        loop {
            while let Ok(command) = self.commands.try_recv() {
                match command {
                    Command::Disconnect => return SocketOutcome::Disconnected,
                    Command::SetFeatures(features) => {
                        self.features = features;
                        if let Some(session_id) = session_id.as_deref() {
                            if let Err(error) = self.reconcile_subscriptions(session_id) {
                                self.with_snapshot(|snapshot| snapshot.last_error = Some(error));
                            }
                        }
                    }
                    Command::Connect | Command::ConnectSaved => {}
                }
            }

            if last_validation.elapsed() >= VALIDATE_INTERVAL {
                if let Err(error) = self.ensure_valid_token() {
                    return SocketOutcome::ConnectionLost(error);
                }
                last_validation = Instant::now();
            }

            if session_id.is_none() && connected_at.elapsed() > WELCOME_TIMEOUT {
                return SocketOutcome::ConnectionLost(
                    "Twitch did not welcome the EventSub session in time".to_string(),
                );
            }
            if keepalive_timeout.is_some_and(|timeout| {
                last_server_message.elapsed() > timeout.saturating_add(KEEPALIVE_GRACE)
            }) {
                return SocketOutcome::ConnectionLost(
                    "Twitch EventSub keepalive timed out".to_string(),
                );
            }

            match socket.read() {
                Ok(Message::Text(text)) => match serde_json::from_str::<Envelope>(&text) {
                    Ok(envelope) => {
                        last_server_message = Instant::now();
                        if !self.seen_ids.insert(&envelope.metadata.message_id) {
                            continue;
                        }
                        match envelope.metadata.message_type.as_str() {
                            "session_welcome" => {
                                let Some(session) = envelope.payload.session else {
                                    return SocketOutcome::ConnectionLost(
                                        "welcome omitted its session".to_string(),
                                    );
                                };
                                keepalive_timeout =
                                    session.keepalive_timeout_seconds.map(Duration::from_secs);
                                session_id = Some(session.id);
                                if !migrating {
                                    self.subscriptions = ActiveSubscriptions::default();
                                }
                                if let Err(error) = self.reconcile_subscriptions(
                                    session_id.as_deref().unwrap_or_default(),
                                ) {
                                    return SocketOutcome::ConnectionLost(error);
                                }
                                let display_name = self
                                    .identity
                                    .as_ref()
                                    .map(|identity| identity.login.clone());
                                self.with_snapshot(|snapshot| {
                                    snapshot.status = "connected".to_string();
                                    snapshot.broadcaster_display_name = display_name;
                                    snapshot.last_error = None;
                                });
                            }
                            "notification" => self.handle_notification(envelope),
                            "session_reconnect" => {
                                if let Some(url) = envelope
                                    .payload
                                    .session
                                    .and_then(|session| session.reconnect_url)
                                {
                                    return SocketOutcome::Reconnect(url);
                                }
                            }
                            "revocation" => self.handle_revocation(envelope),
                            "session_keepalive" => {}
                            _ => {}
                        }
                    }
                    Err(error) => self.with_snapshot(|snapshot| {
                        snapshot.last_error =
                            Some(format!("Ignored invalid Twitch message: {error}"));
                    }),
                },
                Ok(Message::Ping(payload)) => {
                    last_server_message = Instant::now();
                    if let Err(error) = socket.send(Message::Pong(payload)) {
                        return SocketOutcome::ConnectionLost(error.to_string());
                    }
                }
                Ok(Message::Close(_)) => {
                    return SocketOutcome::ConnectionLost("Twitch closed the socket".to_string())
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => return SocketOutcome::ConnectionLost(error.to_string()),
            }
        }
    }

    fn reconcile_subscriptions(&mut self, session_id: &str) -> Result<(), String> {
        let user_id = self
            .identity
            .as_ref()
            .ok_or_else(|| "Twitch identity is unavailable".to_string())?
            .user_id
            .clone();
        if self.features.chat && self.subscriptions.chat.is_none() {
            self.subscriptions.chat = Some(self.create_subscription(
                "channel.chat.message",
                json!({ "broadcaster_user_id": user_id, "user_id": user_id }),
                session_id,
            )?);
        } else if !self.features.chat {
            if let Some(id) = self.subscriptions.chat.take() {
                self.delete_subscription(&id)?;
            }
        }
        if self.features.bits && self.subscriptions.bits.is_none() {
            self.subscriptions.bits = Some(self.create_subscription(
                "channel.bits.use",
                json!({ "broadcaster_user_id": user_id }),
                session_id,
            )?);
        } else if !self.features.bits {
            if let Some(id) = self.subscriptions.bits.take() {
                self.delete_subscription(&id)?;
            }
        }
        Ok(())
    }

    fn create_subscription(
        &self,
        kind: &str,
        condition: Value,
        session_id: &str,
    ) -> Result<String, String> {
        let token = &self
            .tokens
            .as_ref()
            .ok_or_else(|| "Twitch token is unavailable".to_string())?
            .access_token;
        let response = self
            .http
            .post(SUBSCRIPTIONS_URL)
            .bearer_auth(token)
            .header("Client-Id", self.client_id()?)
            .json(&json!({
                "type": kind,
                "version": "1",
                "condition": condition,
                "transport": { "method": "websocket", "session_id": session_id },
            }))
            .send()
            .map_err(|error| format!("Could not subscribe to {kind}: {error}"))?;
        let response: SubscriptionResponse =
            decode_success(response, &format!("subscribe to {kind}"))?;
        response
            .data
            .into_iter()
            .next()
            .map(|item| item.id)
            .ok_or_else(|| format!("Twitch returned no ID for {kind}"))
    }

    fn delete_subscription(&self, id: &str) -> Result<(), String> {
        let token = &self
            .tokens
            .as_ref()
            .ok_or_else(|| "Twitch token is unavailable".to_string())?
            .access_token;
        let response = self
            .http
            .delete(SUBSCRIPTIONS_URL)
            .bearer_auth(token)
            .header("Client-Id", self.client_id()?)
            .query(&[("id", id)])
            .send()
            .map_err(|error| format!("Could not disable Twitch event: {error}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(decode_error(response, "disable Twitch event"))
        }
    }

    fn handle_notification(&mut self, envelope: Envelope) {
        let Some(subscription) = envelope.payload.subscription else {
            return;
        };
        let Some(event) = envelope.payload.event else {
            return;
        };
        match subscription.kind.as_str() {
            "channel.chat.message" if self.features.chat => {
                if let Some(message) = parse_chat(event) {
                    self.with_snapshot(|snapshot| {
                        snapshot.chat_messages_received =
                            snapshot.chat_messages_received.saturating_add(1);
                    });
                    let _ = self.chat_events.try_send(RendererEvent::Chat(message));
                }
            }
            "channel.bits.use" if self.features.bits => {
                if let Some(bits) = parse_bits(event) {
                    self.with_snapshot(|snapshot| {
                        snapshot.bits_events_received =
                            snapshot.bits_events_received.saturating_add(1);
                    });
                    // Bits are priority. A short bounded wait preserves ordinary bursts without ever
                    // allowing renderer IPC backpressure to stall the EventSub socket indefinitely.
                    if self
                        .priority_events
                        .send_timeout(RendererEvent::Bits(bits), Duration::from_millis(100))
                        .is_err()
                    {
                        self.with_snapshot(|snapshot| {
                            snapshot.last_error = Some(
                                "A Bits event was received but the renderer IPC queue was saturated."
                                    .to_string(),
                            );
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_revocation(&mut self, envelope: Envelope) {
        let Some(subscription) = envelope.payload.subscription else {
            return;
        };
        let detail = subscription.status.unwrap_or_else(|| "revoked".to_string());
        match subscription.kind.as_str() {
            "channel.chat.message" => self.subscriptions.chat = None,
            "channel.bits.use" => self.subscriptions.bits = None,
            _ => return,
        }
        self.with_snapshot(|snapshot| {
            snapshot.last_error = Some(format!(
                "Twitch {} subscription was {detail}.",
                subscription.kind
            ))
        });
    }

    fn client_id(&self) -> Result<&str, String> {
        self.client_id
            .as_deref()
            .ok_or_else(|| "Twitch Client ID is not configured.".to_string())
    }

    fn set_status(&self, status: &str) {
        self.with_snapshot(|snapshot| snapshot.status = status.to_string());
    }

    fn set_disconnected(&self) {
        self.with_snapshot(|snapshot| {
            snapshot.status = "disconnected".to_string();
            snapshot.broadcaster_display_name = None;
            snapshot.user_code = None;
            snapshot.verification_uri = None;
        });
    }

    fn set_error(&self, error: String) {
        self.with_snapshot(|snapshot| {
            snapshot.status = "error".to_string();
            snapshot.last_error = Some(error);
            snapshot.user_code = None;
            snapshot.verification_uri = None;
        });
    }

    fn with_snapshot(&self, update: impl FnOnce(&mut TwitchSnapshot)) {
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut snapshot);
    }
}

fn parse_chat(event: Value) -> Option<ChatMessage> {
    let message = event.get("message")?;
    Some(ChatMessage {
        id: event.get("message_id")?.as_str()?.to_string(),
        chatter_id: event.get("chatter_user_id")?.as_str()?.to_string(),
        chatter_login: event.get("chatter_user_login")?.as_str()?.to_string(),
        chatter_name: event.get("chatter_user_name")?.as_str()?.to_string(),
        text: message.get("text")?.as_str()?.to_string(),
        color: optional_string(&event, "color"),
        fragments: message
            .get("fragments")
            .and_then(Value::as_array)
            .map(|fragments| {
                fragments
                    .iter()
                    .filter_map(|fragment| {
                        Some(ChatFragment {
                            text: fragment.get("text")?.as_str()?.to_string(),
                            emote_id: fragment
                                .get("emote")
                                .and_then(|emote| emote.get("id"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            animated: fragment
                                .get("emote")
                                .and_then(|emote| emote.get("format"))
                                .and_then(Value::as_array)
                                .is_some_and(|formats| {
                                    formats
                                        .iter()
                                        .any(|format| format.as_str() == Some("animated"))
                                }),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn parse_bits(event: Value) -> Option<BitsUse> {
    Some(BitsUse {
        user_id: optional_string(&event, "user_id"),
        user_login: optional_string(&event, "user_login"),
        user_name: optional_string(&event, "user_name"),
        bits: event.get("bits")?.as_u64()?.min(u32::MAX as u64) as u32,
        kind: event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        message: event
            .get("message")
            .and_then(|message| message.get("text"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn decode_success<T: for<'de> Deserialize<'de>>(
    response: Response,
    action: &str,
) -> Result<T, String> {
    if response.status().is_success() {
        response.json().map_err(|error| {
            format!("Twitch returned invalid data while trying to {action}: {error}")
        })
    } else {
        Err(decode_error(response, action))
    }
}

fn decode_error(response: Response, action: &str) -> String {
    let status = response.status();
    let body: ApiError = response.json().unwrap_or(ApiError {
        message: status.to_string(),
    });
    format!("Could not {action}: {} ({status})", body.message)
}

fn set_socket_timeout(
    stream: &mut MaybeTlsStream<TcpStream>,
    timeout: Duration,
) -> std::io::Result<()> {
    match stream {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout)),
        MaybeTlsStream::Rustls(stream) => stream.get_mut().set_read_timeout(Some(timeout)),
        _ => Ok(()),
    }
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(
        1u64.checked_shl(attempt.saturating_sub(1).min(5))
            .unwrap_or(32)
            .min(30),
    )
}

#[cfg(target_os = "windows")]
fn load_tokens() -> Result<Option<AuthTokens>, String> {
    let entry = keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        .map_err(|error| format!("Could not open Windows Credential Manager: {error}"))?;
    match entry.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map(Some)
            .map_err(|error| format!("Stored Twitch authorization is invalid: {error}")),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Could not read Twitch authorization: {error}")),
    }
}

#[cfg(target_os = "windows")]
fn save_tokens(tokens: &AuthTokens) -> Result<(), String> {
    let entry = keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        .map_err(|error| format!("Could not open Windows Credential Manager: {error}"))?;
    let value = serde_json::to_string(tokens)
        .map_err(|error| format!("Could not encode Twitch authorization: {error}"))?;
    entry
        .set_password(&value)
        .map_err(|error| format!("Could not save Twitch authorization: {error}"))
}

#[cfg(not(target_os = "windows"))]
fn load_tokens() -> Result<Option<AuthTokens>, String> {
    Ok(None)
}

#[cfg(not(target_os = "windows"))]
fn save_tokens(_tokens: &AuthTokens) -> Result<(), String> {
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
struct AuthTokens {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Default)]
struct ActiveSubscriptions {
    chat: Option<String>,
    bits: Option<String>,
}

#[derive(Default)]
struct MessageDeduper {
    times: HashMap<String, Instant>,
    order: VecDeque<String>,
}

impl MessageDeduper {
    fn insert(&mut self, id: &str) -> bool {
        let now = Instant::now();
        while self.order.len() >= MAX_SEEN_MESSAGE_IDS {
            if let Some(oldest) = self.order.pop_front() {
                self.times.remove(&oldest);
            }
        }
        while let Some(oldest) = self.order.front() {
            let expired = self
                .times
                .get(oldest)
                .map(|seen| now.duration_since(*seen) >= MESSAGE_ID_TTL)
                .unwrap_or(true);
            if !expired {
                break;
            }
            if let Some(expired) = self.order.pop_front() {
                self.times.remove(&expired);
            }
        }
        if self.times.contains_key(id) {
            return false;
        }
        self.times.insert(id.to_string(), now);
        self.order.push_back(id.to_string());
        true
    }
}

enum SocketOutcome {
    Disconnected,
    Reconnect(String),
    ConnectionLost(String),
}

enum TokenPoll {
    Pending,
    SlowDown,
    Denied(String),
    Granted(AuthTokens),
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Vec<String>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Clone, Deserialize)]
struct Identity {
    client_id: String,
    login: String,
    user_id: String,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct SubscriptionResponse {
    data: Vec<SubscriptionItem>,
}

#[derive(Deserialize)]
struct SubscriptionItem {
    id: String,
}

#[derive(Deserialize)]
struct Envelope {
    metadata: EnvelopeMetadata,
    payload: EnvelopePayload,
}

#[derive(Deserialize)]
struct EnvelopeMetadata {
    message_id: String,
    message_type: String,
}

#[derive(Deserialize)]
struct EnvelopePayload {
    #[serde(default)]
    session: Option<Session>,
    #[serde(default)]
    subscription: Option<EnvelopeSubscription>,
    #[serde(default)]
    event: Option<Value>,
}

#[derive(Deserialize)]
struct Session {
    id: String,
    #[serde(default)]
    keepalive_timeout_seconds: Option<u64>,
    #[serde(default)]
    reconnect_url: Option<String>,
}

#[derive(Deserialize)]
struct EnvelopeSubscription {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chat() {
        let message = parse_chat(json!({
            "chatter_user_id": "1", "chatter_user_login": "viewer", "chatter_user_name": "Viewer",
            "message_id": "m1", "message": {
                "text": "hello Kappa",
                "fragments": [
                    { "type": "text", "text": "hello ", "emote": null },
                    { "type": "emote", "text": "Kappa", "emote": {
                        "id": "25", "format": ["static", "animated"]
                    } }
                ]
            }, "color": "#00FF00"
        }))
        .unwrap();
        assert_eq!(message.chatter_name, "Viewer");
        assert_eq!(message.text, "hello Kappa");
        assert_eq!(message.fragments[1].emote_id.as_deref(), Some("25"));
        assert!(message.fragments[1].animated);
    }

    #[test]
    fn parses_anonymous_bits() {
        let event = parse_bits(json!({
            "user_id": null, "user_login": null, "user_name": null,
            "bits": 250, "type": "cheer", "message": { "text": "nice" }
        }))
        .unwrap();
        assert_eq!(event.bits, 250);
        assert_eq!(event.user_name, None);
    }

    #[test]
    fn deduplicates_messages() {
        let mut deduper = MessageDeduper::default();
        assert!(deduper.insert("abc"));
        assert!(!deduper.insert("abc"));
        assert!(deduper.insert("def"));
    }

    #[test]
    fn reconnect_backoff_is_capped() {
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(4), Duration::from_secs(8));
        assert_eq!(reconnect_delay(99), Duration::from_secs(30));
    }

    #[test]
    fn renderer_chat_payload_keeps_native_ingress_names() {
        let (command, payload) = RendererEvent::Chat(ChatMessage {
            id: "m1".to_string(),
            chatter_id: "1".to_string(),
            chatter_login: "viewer".to_string(),
            chatter_name: "Viewer".to_string(),
            text: "hello".to_string(),
            color: None,
            fragments: vec![],
        })
        .command_and_payload();
        assert_eq!(command, "twitch_chat_message");
        assert_eq!(payload["donor"], "Viewer");
        assert_eq!(payload["message"], "hello");
    }
}
