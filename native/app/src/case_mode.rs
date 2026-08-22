//! Drives case openings for the overlay: config loading, the viewer queue, and
//! the one session that plays at a time.
//!
//! The simulation and rendering live in `case_sim` and `renderer`. This module
//! is the bridge to the app: it decides who is next, keeps a running session
//! ticking, and hands back each settled result exactly once so the caller can
//! log it or forward it to chat.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc::Sender};

use case_sim::{CaseConfig, CaseRequest, CaseResult, CaseSession};
use core_types::AppColor;
use serde::Serialize;

use crate::case_api::CaseApiEvent;

/// Viewers allowed to be waiting. Past this, new requests are refused rather
/// than queued, so a chat flood cannot build an hour-long backlog.
const MAX_QUEUE: usize = 24;

/// Searched only in the working directory and beside the executable, because a
/// bare file name is too generic to go hunting for further up the tree.
const CONFIG_LOCAL_NAME: &str = "rewards.json";

/// Searched in those same two places and in their parents, since this path is
/// specific enough to identify the repository or an install layout.
const CONFIG_ASSET_PATH: &str = "Assets/case/rewards.json";

/// How far to climb while looking for [`CONFIG_ASSET_PATH`]. Four levels
/// reaches the repository root from both `native/` and
/// `native/target/<profile>/`, which is what `cargo run` gives us.
const CONFIG_SEARCH_HEIGHT: usize = 4;

/// How often the report the control UI polls is rebuilt. The UI reads it about
/// once a second, so rebuilding it every frame would only burn allocations.
const STATUS_PUBLISH_INTERVAL: f32 = 0.1;

#[derive(Clone, Debug)]
enum ConfigSource {
    BuiltIn,
    Loaded(PathBuf),
    Failed { path: PathBuf, error: String },
}

/// Everything the control UI's case page shows. Published by the director and
/// served straight off the IPC thread, which cannot reach into the app.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseStatusReport {
    /// Where the definition came from, phrased for display.
    pub config_label: String,
    pub config_error: Option<String>,
    pub case_name: String,
    pub currency_name: String,
    pub chat_command: String,
    pub roll_cost: u32,
    pub wheel_enabled: bool,
    pub queued: usize,
    pub opened: u32,
    pub refused: u32,
    pub tiers: Vec<CaseTierReport>,
    pub wheel_prizes: Vec<String>,
    /// Absent while nothing is playing.
    pub active: Option<CaseActiveReport>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseTierReport {
    pub id: String,
    pub name: String,
    pub odds: f64,
    pub color: String,
    /// Which celebration recipe this tier earns, so the page can label it.
    pub celebration: String,
    pub rewards: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseActiveReport {
    pub viewer: String,
    pub phase: String,
    pub progress: f32,
    pub tier_name: String,
    pub tier_color: String,
    pub reward_name: String,
    pub celebration: String,
    /// Set only once a legendary wheel has decided its prize.
    pub wheel_prize: Option<String>,
}

/// Shared slot the IPC thread reads when the control UI asks for case state.
pub type CaseStatusBoard = Mutex<CaseStatusReport>;

struct QueuedCase {
    request: CaseRequest,
    events: Option<Sender<CaseApiEvent>>,
}

pub struct CaseDirector {
    config: CaseConfig,
    session: Option<CaseSession>,
    session_events: Option<Sender<CaseApiEvent>>,
    queue: VecDeque<QueuedCase>,
    /// Consecutive openings per viewer, shown next to their name.
    streaks: HashMap<String, u32>,
    /// Guards against reporting the same drop on more than one frame.
    reported: bool,
    source: ConfigSource,
    /// Requests refused because the queue was full.
    refused: u32,
    opened: u32,
    /// Throttles rebuilding the control UI's report.
    publish_timer: f32,
}

impl Default for CaseDirector {
    fn default() -> Self {
        let mut director = Self {
            config: CaseConfig::default(),
            session: None,
            session_events: None,
            queue: VecDeque::new(),
            streaks: HashMap::new(),
            reported: true,
            source: ConfigSource::BuiltIn,
            refused: 0,
            opened: 0,
            publish_timer: STATUS_PUBLISH_INTERVAL,
        };
        director.load_default_config();
        director
    }
}

impl CaseDirector {
    /// Looks for a dashboard `rewards.json` next to the app. Missing is normal
    /// and silent; present but broken is reported on the HUD.
    fn load_default_config(&mut self) {
        for candidate in self.candidate_config_paths() {
            if !candidate.is_file() {
                continue;
            }
            match self.load_config(&candidate, None) {
                Ok(()) => return,
                // Keep looking: a stale file in the working directory should not
                // shadow a good one beside the executable.
                Err(_) => continue,
            }
        }
    }

    /// The working directory first, so a streamer's own file beside the app
    /// wins over the copy that shipped with the build. Each root is then
    /// climbed a few levels, which is what makes `cargo run` from `native/`
    /// find the repository's `Assets/case/rewards.json`.
    fn candidate_config_paths(&self) -> Vec<PathBuf> {
        let exe_directory = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let roots: Vec<PathBuf> = [std::env::current_dir().ok(), exe_directory]
            .into_iter()
            .flatten()
            .collect();
        let mut paths = Vec::new();
        for root in &roots {
            paths.push(root.join(CONFIG_LOCAL_NAME));
        }
        for root in &roots {
            for directory in root.ancestors().take(CONFIG_SEARCH_HEIGHT + 1) {
                paths.push(directory.join(CONFIG_ASSET_PATH));
            }
        }
        paths
    }

    /// Replaces the case definition. A sequence already playing keeps the
    /// config it started with, because the session owns its own copy.
    pub fn load_config(&mut self, path: &Path, profile: Option<&str>) -> Result<(), String> {
        match CaseConfig::load(path, profile) {
            Ok(config) => {
                self.config = config;
                self.source = ConfigSource::Loaded(path.to_path_buf());
                Ok(())
            }
            Err(error) => {
                self.source = ConfigSource::Failed {
                    path: path.to_path_buf(),
                    error: error.clone(),
                };
                Err(error)
            }
        }
    }

    pub fn case_name(&self) -> &str {
        &self.config.case_name
    }

    /// The chat command that opens a case, including its leading marker.
    pub fn chat_command(&self) -> &str {
        &self.config.chat_command
    }

    /// What an opening costs, phrased for the HUD footer.
    pub fn cost_summary(&self) -> String {
        format!(
            "{} costs {} {}",
            self.config.chat_command, self.config.roll_cost, self.config.currency_name
        )
    }

    pub fn session(&self) -> Option<&CaseSession> {
        self.session.as_ref()
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Queues an opening. Returns false when the queue is full.
    pub fn request(&mut self, request: CaseRequest) -> bool {
        self.queue_request(request, None)
    }

    /// Queues a WebSocket opening and reports its terminal state to that client.
    pub fn request_from_api(&mut self, request: CaseRequest, events: Sender<CaseApiEvent>) -> bool {
        self.queue_request(request, Some(events))
    }

    fn queue_request(
        &mut self,
        mut request: CaseRequest,
        events: Option<Sender<CaseApiEvent>>,
    ) -> bool {
        if self.queue.len() >= MAX_QUEUE {
            self.refused = self.refused.saturating_add(1);
            if let Some(events) = events {
                let _ = events.send(CaseApiEvent::Rejected("Case queue is full".to_string()));
            }
            return false;
        }
        let key = request.viewer.trim().to_lowercase();
        if !key.is_empty() {
            let streak = self.streaks.entry(key).or_insert(0);
            *streak = streak.saturating_add(1);
            request.streak = *streak;
        }
        self.queue.push_back(QueuedCase { request, events });
        if let Some(events) = self.queue.back().and_then(|queued| queued.events.as_ref()) {
            let _ = events.send(CaseApiEvent::Accepted);
        }
        true
    }

    /// Cancels the running sequence and empties the queue.
    pub fn cancel(&mut self) {
        self.session = None;
        if let Some(events) = self.session_events.take() {
            let _ = events.send(CaseApiEvent::Cancelled);
        }
        for queued in self.queue.drain(..) {
            if let Some(events) = queued.events {
                let _ = events.send(CaseApiEvent::Cancelled);
            }
        }
        self.reported = true;
    }

    /// Advances the sequence. Returns the drop on the single frame it settles,
    /// which is well before the sequence finishes playing.
    pub fn update(&mut self, dt: f32) -> Option<CaseResult> {
        let mut settled = None;
        let mut completed = None;
        if let Some(session) = &mut self.session {
            session.update(dt);
            if !self.reported && session.result_is_settled() {
                self.reported = true;
                settled = Some(session.result());
            }
            if session.is_finished() {
                completed = Some(session.result());
                self.session = None;
            }
        }
        if let Some(result) = completed {
            if let Some(events) = self.session_events.take() {
                let _ = events.send(CaseApiEvent::Completed(result));
            }
        }
        if self.session.is_none() {
            if let Some(queued) = self.queue.pop_front() {
                self.session = Some(CaseSession::new(self.config.clone(), queued.request));
                self.session_events = queued.events;
                self.reported = false;
                self.opened = self.opened.saturating_add(1);
            }
        }
        settled
    }

    /// Refreshes the report the control UI polls, on a timer. Cheap to call
    /// every frame; only rebuilds a few times a second.
    pub fn publish_status(&mut self, board: &CaseStatusBoard, dt: f32) {
        self.publish_timer += dt;
        if self.publish_timer < STATUS_PUBLISH_INTERVAL {
            return;
        }
        self.publish_timer = 0.0;
        let report = self.status_report();
        if let Ok(mut slot) = board.lock() {
            *slot = report;
        }
    }

    /// The state behind the control UI's case page.
    pub fn status_report(&self) -> CaseStatusReport {
        let tier_count = self.config.tiers.len();
        CaseStatusReport {
            config_label: match &self.source {
                ConfigSource::BuiltIn => "Built-in defaults".to_string(),
                ConfigSource::Loaded(path) => path.display().to_string(),
                ConfigSource::Failed { path, .. } => path.display().to_string(),
            },
            config_error: match &self.source {
                ConfigSource::Failed { error, .. } => Some(error.clone()),
                _ => None,
            },
            case_name: self.config.case_name.clone(),
            currency_name: self.config.currency_name.clone(),
            chat_command: self.config.chat_command.clone(),
            roll_cost: self.config.roll_cost,
            wheel_enabled: self.config.legendary_wheel_enabled && self.config.wheel.is_some(),
            queued: self.queue.len(),
            opened: self.opened,
            refused: self.refused,
            tiers: self
                .config
                .tiers
                .iter()
                .map(|tier| CaseTierReport {
                    id: tier.id.clone(),
                    name: tier.name.clone(),
                    odds: tier.odds,
                    color: hex_color(tier.color),
                    celebration: case_sim::Celebration::for_intensity(tier.intensity(tier_count))
                        .label()
                        .to_string(),
                    rewards: tier
                        .rewards
                        .iter()
                        .map(|reward| reward.name.clone())
                        .collect(),
                })
                .collect(),
            wheel_prizes: self
                .config
                .wheel
                .as_ref()
                .map(|wheel| wheel.prizes.iter().map(|p| p.name.clone()).collect())
                .unwrap_or_default(),
            active: self.session.as_ref().map(|session| CaseActiveReport {
                viewer: session.viewer_name().to_string(),
                phase: session.stage.phase.label().to_string(),
                progress: session.stage.phase_progress,
                tier_name: session.tier().name.clone(),
                tier_color: hex_color(session.tier_color()),
                reward_name: session.reward_name(),
                celebration: session.celebration().label().to_string(),
                wheel_prize: session.wheel_prize_name().map(str::to_string),
            }),
        }
    }

    /// Lines for the debug HUD panel.
    pub fn status_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        match &self.source {
            ConfigSource::BuiltIn => lines.push("Config: built-in".to_string()),
            ConfigSource::Loaded(path) => lines.push(format!(
                "Config: {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            )),
            ConfigSource::Failed { path, error } => {
                lines.push(format!(
                    "Config FAILED: {}",
                    truncate(
                        path.file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string())
                            .as_str(),
                        24
                    )
                ));
                lines.push(truncate(error, 52));
            }
        }
        lines.push(format!(
            "{}  {} tiers",
            truncate(&self.config.case_name, 28),
            self.config.tiers.len()
        ));
        match &self.session {
            Some(session) => lines.push(format!(
                "{}  {}  {:.0}%  {}",
                truncate(session.viewer_name(), 12),
                session.stage.phase.label(),
                session.stage.phase_progress * 100.0,
                session.celebration().label()
            )),
            None => lines.push("Idle".to_string()),
        }
        lines.push(format!(
            "Queue {}  Opened {}{}",
            self.queue.len(),
            self.opened,
            if self.refused > 0 {
                format!("  Refused {}", self.refused)
            } else {
                String::new()
            }
        ));
        lines
    }
}

fn hex_color(color: AppColor) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{head}~")
}

/// Recognises the configured chat command, e.g. `!roll` or `!roll please`.
/// Returns false for anything else, including `!rolling`.
pub fn chat_message_opens_a_case(message: &str, command: &str) -> bool {
    let command = command.trim().trim_start_matches('!').to_lowercase();
    if command.is_empty() {
        return false;
    }
    let first = message
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('!')
        .to_lowercase();
    first == command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(viewer: &str) -> CaseRequest {
        CaseRequest {
            viewer: viewer.to_string(),
            seed: Some(7),
            ..CaseRequest::default()
        }
    }

    #[test]
    fn a_queued_request_starts_playing_on_the_next_tick() {
        let mut director = CaseDirector::default();
        assert!(!director.is_active());
        assert!(director.request(request("viewer")));
        director.update(1.0 / 60.0);
        assert!(director.is_active());
        assert_eq!(director.queue_len(), 0);
    }

    #[test]
    fn openings_play_one_at_a_time_and_in_order() {
        let mut director = CaseDirector::default();
        for name in ["first", "second", "third"] {
            assert!(director.request(request(name)));
        }
        director.update(1.0 / 60.0);
        assert_eq!(director.session().unwrap().viewer_name(), "first");
        assert_eq!(director.queue_len(), 2);

        let mut order = vec!["first".to_string()];
        for _ in 0..6000 {
            director.update(1.0 / 60.0);
            if let Some(session) = director.session() {
                let viewer = session.viewer_name().to_string();
                if order.last() != Some(&viewer) {
                    order.push(viewer);
                }
            }
            if !director.is_active() && director.queue_len() == 0 {
                break;
            }
        }
        assert_eq!(order, vec!["first", "second", "third"]);
    }

    #[test]
    fn each_drop_is_reported_exactly_once() {
        let mut director = CaseDirector::default();
        director.request(request("viewer"));
        let mut results = Vec::new();
        for _ in 0..6000 {
            if let Some(result) = director.update(1.0 / 60.0) {
                results.push(result);
            }
            if !director.is_active() {
                break;
            }
        }
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].viewer, "viewer");
        assert!(!results[0].reward_name.is_empty());
    }

    #[test]
    fn a_flood_is_refused_rather_than_queued_forever() {
        let mut director = CaseDirector::default();
        for index in 0..MAX_QUEUE {
            assert!(director.request(request(&format!("viewer{index}"))));
        }
        assert!(!director.request(request("one-too-many")));
        assert_eq!(director.queue_len(), MAX_QUEUE);
        assert!(
            director
                .status_lines()
                .iter()
                .any(|line| line.contains("Refused"))
        );
    }

    #[test]
    fn streaks_count_repeat_openings_by_the_same_viewer() {
        let mut director = CaseDirector::default();
        director.request(request("Zed"));
        director.request(request("zed"));
        director.request(request("other"));
        assert_eq!(director.queue[0].request.streak, 1);
        assert_eq!(director.queue[1].request.streak, 2);
        assert_eq!(director.queue[2].request.streak, 1);
    }

    #[test]
    fn cancelling_clears_the_running_sequence_and_the_queue() {
        let mut director = CaseDirector::default();
        director.request(request("a"));
        director.request(request("b"));
        director.update(1.0 / 60.0);
        director.cancel();
        assert!(!director.is_active());
        assert_eq!(director.queue_len(), 0);
        // Nothing left to report, and nothing restarts on the next tick.
        assert!(director.update(1.0 / 60.0).is_none());
        assert!(!director.is_active());
    }

    #[test]
    fn api_completion_waits_for_the_entire_animation() {
        let mut director = CaseDirector::default();
        let (events_tx, events_rx) = std::sync::mpsc::channel();
        assert!(director.request_from_api(request("api-viewer"), events_tx));
        assert!(matches!(events_rx.try_recv(), Ok(CaseApiEvent::Accepted)));

        let mut saw_settled_result = false;
        for _ in 0..8000 {
            if director.update(1.0 / 60.0).is_some() {
                saw_settled_result = true;
                assert!(
                    events_rx.try_recv().is_err(),
                    "completion arrived when the result settled, before the outro"
                );
            }
            if !director.is_active() && director.queue_len() == 0 {
                break;
            }
        }

        assert!(saw_settled_result);
        match events_rx.try_recv() {
            Ok(CaseApiEvent::Completed(result)) => {
                assert_eq!(result.viewer, "api-viewer");
            }
            event => panic!("expected one completion event, got {event:?}"),
        }
        assert!(events_rx.try_recv().is_err());
    }

    #[test]
    fn cancelling_notifies_api_openings() {
        let mut director = CaseDirector::default();
        let (first_tx, first_rx) = std::sync::mpsc::channel();
        let (second_tx, second_rx) = std::sync::mpsc::channel();
        director.request_from_api(request("first"), first_tx);
        director.request_from_api(request("second"), second_tx);
        let _ = first_rx.recv();
        let _ = second_rx.recv();
        director.update(1.0 / 60.0);

        director.cancel();

        assert!(matches!(first_rx.try_recv(), Ok(CaseApiEvent::Cancelled)));
        assert!(matches!(second_rx.try_recv(), Ok(CaseApiEvent::Cancelled)));
    }

    #[test]
    fn the_config_that_ships_with_the_repository_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Assets/case/rewards.json");
        let mut director = CaseDirector::default();
        director
            .load_config(&path, None)
            .expect("the shipped rewards.json should load");
        // It is meant to read the same as the built-in case, so a streamer can
        // start editing from something they already saw on screen.
        let built_in = CaseConfig::default();
        assert_eq!(director.case_name(), built_in.case_name);
        assert_eq!(director.config.tiers.len(), built_in.tiers.len());
        for (loaded, expected) in director.config.tiers.iter().zip(&built_in.tiers) {
            assert_eq!(loaded.id, expected.id);
            assert_eq!(loaded.odds, expected.odds);
            assert_eq!(loaded.color, expected.color);
            assert_eq!(loaded.rewards, expected.rewards);
        }
        assert_eq!(
            director
                .config
                .wheel
                .as_ref()
                .map(|wheel| wheel.prizes.len()),
            built_in.wheel.as_ref().map(|wheel| wheel.prizes.len())
        );
    }

    #[test]
    fn a_missing_config_falls_back_to_the_built_in_case() {
        let mut director = CaseDirector::default();
        let error = director
            .load_config(Path::new("definitely-not-here-9f3a.json"), None)
            .expect_err("a missing file should report");
        assert!(!error.is_empty());
        // The previous config stays usable, so an opening still plays.
        director.request(request("viewer"));
        director.update(1.0 / 60.0);
        assert!(director.is_active());
        assert!(
            director
                .status_lines()
                .iter()
                .any(|line| line.contains("FAILED"))
        );
    }

    #[test]
    fn status_lines_stay_short_enough_for_the_hud() {
        let mut director = CaseDirector::default();
        director.request(request("a-rather-long-viewer-name-here"));
        director.update(1.0 / 60.0);
        for line in director.status_lines() {
            assert!(line.chars().count() <= 60, "too wide: {line}");
        }
    }

    #[test]
    fn the_chat_command_matches_exactly() {
        assert!(chat_message_opens_a_case("!roll", "!roll"));
        assert!(chat_message_opens_a_case("  !ROLL  ", "!roll"));
        assert!(chat_message_opens_a_case("!roll please", "roll"));
        assert!(chat_message_opens_a_case("roll", "!roll"));
        assert!(!chat_message_opens_a_case("!rolling", "!roll"));
        assert!(!chat_message_opens_a_case("please !roll", "!roll"));
        assert!(!chat_message_opens_a_case("hello", "!roll"));
        assert!(!chat_message_opens_a_case("!roll", ""));
    }
}
