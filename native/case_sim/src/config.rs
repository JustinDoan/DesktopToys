//! Case definitions.
//!
//! The on-disk format is deliberately the same `rewards.json` schema the
//! browser-source overlay at `zedandgone/csgo-case-overlay` writes, so an
//! existing dashboard config drops in without conversion. Every field is
//! optional; anything missing falls back to the built-in case.

use core_types::AppColor;
use serde::Deserialize;

use crate::rng::Rng;

/// One prize inside a rarity tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reward {
    pub name: String,
    pub description: String,
    /// Short all-caps label stamped on the card, e.g. `TOK` or `VIP`.
    pub badge: String,
}

impl Reward {
    pub fn new(name: &str, description: &str, badge: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            badge: badge.to_string(),
        }
    }
}

/// A rarity tier and the prizes it can award.
#[derive(Clone, Debug)]
pub struct RarityTier {
    pub id: String,
    pub name: String,
    /// Relative weight, expressed as a percentage in the source config.
    pub odds: f64,
    pub color: AppColor,
    pub rewards: Vec<Reward>,
    /// Position on the ladder, ascending. Tiers are listed floor-first.
    pub rank: usize,
}

impl RarityTier {
    /// Drives how loud the reveal gets: 0 for the floor tier, 1 for the top.
    pub fn intensity(&self, tier_count: usize) -> f32 {
        if tier_count <= 1 {
            return 1.0;
        }
        self.rank as f32 / (tier_count - 1) as f32
    }
}

#[derive(Clone, Debug)]
pub struct LegendaryWheel {
    pub title: String,
    pub spin_seconds: f32,
    pub prizes: Vec<Reward>,
}

#[derive(Clone, Debug)]
pub struct HypeMessages {
    pub opening: String,
    pub common: String,
    pub rare: String,
    pub legendary: String,
}

impl Default for HypeMessages {
    fn default() -> Self {
        Self {
            opening: "ROLLING THE CASE".to_string(),
            common: "CLEAN LITTLE HIT".to_string(),
            rare: "THAT ONE HAS SOME SHINE".to_string(),
            legendary: "LEGENDARY HIT".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaseConfig {
    pub profile_name: String,
    pub case_name: String,
    pub currency_name: String,
    /// Chat command that opens a case, including its leading marker.
    pub chat_command: String,
    pub roll_cost: u32,
    pub show_odds: bool,
    pub show_profile_name: bool,
    pub extended_result_hold: bool,
    pub legendary_wheel_enabled: bool,
    pub hype: HypeMessages,
    pub tiers: Vec<RarityTier>,
    pub wheel: Option<LegendaryWheel>,
}

/// The result of one weighted roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollOutcome {
    pub tier_index: usize,
    pub reward_index: usize,
}

impl CaseConfig {
    /// Reads a dashboard `rewards.json`. `profile_id` overrides the file's
    /// own `activeProfileId`.
    pub fn from_rewards_json(text: &str, profile_id: Option<&str>) -> Result<Self, String> {
        let file: RewardsFile =
            serde_json::from_str(text).map_err(|error| format!("rewards.json: {error}"))?;
        let wanted = profile_id
            .map(str::to_string)
            .or_else(|| file.active_profile_id.clone());
        let profile = wanted
            .and_then(|id| {
                file.profiles
                    .iter()
                    .find(|profile| profile.id.as_deref() == Some(id.as_str()))
            })
            .or_else(|| file.profiles.first())
            .ok_or_else(|| "rewards.json has no profiles".to_string())?;

        let tiers: Vec<RarityTier> = profile
            .rarities
            .iter()
            .filter(|tier| !tier.rewards.is_empty())
            .enumerate()
            .map(|(rank, tier)| RarityTier {
                id: tier.id.clone().unwrap_or_else(|| format!("tier_{rank}")),
                name: tier.name.clone().unwrap_or_else(|| "Unknown".to_string()),
                odds: tier.odds.unwrap_or(1.0).max(0.0),
                color: tier
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(FALLBACK_TIER_COLORS[rank.min(FALLBACK_TIER_COLORS.len() - 1)]),
                rewards: tier.rewards.iter().map(RawReward::resolve).collect(),
                rank,
            })
            .collect();

        if tiers.is_empty() {
            return Err("rewards.json has no rarity tiers with prizes".to_string());
        }
        if tiers.iter().all(|tier| tier.odds <= 0.0) {
            return Err("rewards.json rarity odds are all zero".to_string());
        }

        let globals = file.global_settings.unwrap_or_default();
        let wheel = profile.legendary_wheel.as_ref().and_then(|wheel| {
            let prizes: Vec<Reward> = wheel.prizes.iter().map(RawReward::resolve).collect();
            (!prizes.is_empty()).then(|| LegendaryWheel {
                title: wheel
                    .title
                    .clone()
                    .unwrap_or_else(|| "LEGENDARY WHEEL".to_string()),
                spin_seconds: (wheel.spin_duration_ms.unwrap_or(8200.0) / 1000.0).clamp(2.0, 20.0),
                prizes,
            })
        });

        Ok(Self {
            profile_name: profile.name.clone().unwrap_or_else(|| "Case".to_string()),
            case_name: profile
                .case_name
                .clone()
                .or_else(|| profile.name.clone())
                .unwrap_or_else(|| "VIEWER REWARD CASE".to_string()),
            currency_name: globals
                .currency_name
                .unwrap_or_else(|| "points".to_string()),
            chat_command: globals
                .gamble_command
                .map(|command| command.trim().to_string())
                .filter(|command| !command.is_empty())
                .unwrap_or_else(|| "!roll".to_string()),
            roll_cost: globals.roll_cost.unwrap_or(1000),
            show_odds: globals.show_odds.unwrap_or(true),
            show_profile_name: globals.show_profile_name.unwrap_or(true),
            extended_result_hold: globals.extended_result_hold.unwrap_or(false),
            legendary_wheel_enabled: profile.legendary_wheel_enabled.unwrap_or(true),
            hype: profile
                .hype_messages
                .as_ref()
                .map(RawHype::resolve)
                .unwrap_or_default(),
            tiers,
            wheel,
        })
    }

    pub fn total_odds(&self) -> f64 {
        self.tiers.iter().map(|tier| tier.odds).sum()
    }

    /// Weighted tier pick followed by a uniform prize pick, mirroring the
    /// browser overlay so the odds a viewer sees stay true.
    pub fn roll(
        &self,
        rng: &mut Rng,
        forced_tier: Option<&str>,
        forced_reward: Option<&str>,
    ) -> RollOutcome {
        let tier_index = forced_tier
            .and_then(|wanted| self.find_tier(wanted))
            .unwrap_or_else(|| self.roll_tier(rng));
        let tier = &self.tiers[tier_index];
        let reward_index = forced_reward
            .and_then(|wanted| {
                tier.rewards
                    .iter()
                    .position(|reward| reward.name.eq_ignore_ascii_case(wanted))
            })
            .unwrap_or_else(|| rng.below(tier.rewards.len()));
        RollOutcome {
            tier_index,
            reward_index,
        }
    }

    fn roll_tier(&self, rng: &mut Rng) -> usize {
        let total = self.total_odds();
        if total <= 0.0 {
            return 0;
        }
        let target = rng.unit() as f64 * total;
        let mut running = 0.0;
        for (index, tier) in self.tiers.iter().enumerate() {
            running += tier.odds;
            if target < running {
                return index;
            }
        }
        self.tiers.len() - 1
    }

    fn find_tier(&self, wanted: &str) -> Option<usize> {
        self.tiers
            .iter()
            .position(|tier| tier.id.eq_ignore_ascii_case(wanted))
            .or_else(|| {
                self.tiers
                    .iter()
                    .position(|tier| tier.name.eq_ignore_ascii_case(wanted))
            })
    }

    /// Picks a filler card for the reel: any tier, any prize, unweighted, so
    /// the strip stays visually varied instead of a wall of blue.
    pub fn random_any(&self, rng: &mut Rng) -> RollOutcome {
        let tier_index = rng.below(self.tiers.len());
        RollOutcome {
            tier_index,
            reward_index: rng.below(self.tiers[tier_index].rewards.len()),
        }
    }

    pub fn reward(&self, outcome: RollOutcome) -> &Reward {
        &self.tiers[outcome.tier_index].rewards[outcome.reward_index]
    }

    pub fn is_top_tier(&self, tier_index: usize) -> bool {
        tier_index + 1 == self.tiers.len()
    }
}

const FALLBACK_TIER_COLORS: [AppColor; 5] = [
    AppColor::from_rgb(0x4b, 0x69, 0xff),
    AppColor::from_rgb(0x88, 0x47, 0xff),
    AppColor::from_rgb(0xd3, 0x2c, 0xe6),
    AppColor::from_rgb(0xeb, 0x4b, 0x4b),
    AppColor::from_rgb(0xff, 0xd7, 0x00),
];

impl Default for CaseConfig {
    /// The stock Counter-Strike distribution with placeholder viewer rewards.
    fn default() -> Self {
        Self {
            profile_name: "Viewer Reward Case".to_string(),
            case_name: "VIEWER REWARD CASE".to_string(),
            currency_name: "points".to_string(),
            chat_command: "!roll".to_string(),
            roll_cost: 1000,
            show_odds: true,
            show_profile_name: true,
            extended_result_hold: false,
            legendary_wheel_enabled: true,
            hype: HypeMessages::default(),
            tiers: vec![
                RarityTier {
                    id: "mil_spec".to_string(),
                    name: "Mil-Spec".to_string(),
                    odds: 79.92,
                    color: FALLBACK_TIER_COLORS[0],
                    rewards: vec![
                        Reward::new("25 Tokens", "Adds 25 stream tokens.", "TOK"),
                        Reward::new("50 Tokens", "Adds 50 stream tokens.", "TOK"),
                        Reward::new("Free Spin", "One extra case opening.", "SPIN"),
                        Reward::new("Reroll", "Reroll this prize once.", "ROLL"),
                        Reward::new("Shoutout", "A shoutout on stream.", "VOX"),
                    ],
                    rank: 0,
                },
                RarityTier {
                    id: "restricted".to_string(),
                    name: "Restricted".to_string(),
                    odds: 15.98,
                    color: FALLBACK_TIER_COLORS[1],
                    rewards: vec![
                        Reward::new("250 Tokens", "Adds 250 stream tokens.", "TOK"),
                        Reward::new("VIP Time", "Temporary VIP in chat.", "VIP"),
                        Reward::new("Song Request", "Queue one track.", "SONG"),
                        Reward::new("Loadout Pick", "Choose the next loadout.", "KIT"),
                    ],
                    rank: 1,
                },
                RarityTier {
                    id: "classified".to_string(),
                    name: "Classified".to_string(),
                    odds: 3.2,
                    color: FALLBACK_TIER_COLORS[2],
                    rewards: vec![
                        Reward::new("Modifier", "Add a run modifier.", "MOD"),
                        Reward::new("Challenge", "Set a stream challenge.", "CHAL"),
                        Reward::new("Name A Thing", "Name something in game.", "TAG"),
                    ],
                    rank: 2,
                },
                RarityTier {
                    id: "covert".to_string(),
                    name: "Covert".to_string(),
                    odds: 0.64,
                    color: FALLBACK_TIER_COLORS[3],
                    rewards: vec![
                        Reward::new("Play With Streamer", "A scheduled game slot.", "PLAY"),
                        Reward::new("Scene Takeover", "Pick the next scene.", "LIVE"),
                    ],
                    rank: 3,
                },
                RarityTier {
                    id: "rare_special".to_string(),
                    name: "Rare Special".to_string(),
                    odds: 0.26,
                    color: FALLBACK_TIER_COLORS[4],
                    rewards: vec![Reward::new(
                        "Legendary Drop",
                        "Spin the legendary wheel.",
                        "LEG",
                    )],
                    rank: 4,
                },
            ],
            wheel: Some(LegendaryWheel {
                title: "LEGENDARY WHEEL".to_string(),
                spin_seconds: 8.2,
                prizes: vec![
                    Reward::new("Golden VIP Day", "VIP for the whole day.", "VIP"),
                    Reward::new("1000 Tokens", "Adds 1000 stream tokens.", "TOK"),
                    Reward::new("Play With Streamer", "A scheduled game slot.", "PLAY"),
                    Reward::new("Double Legendary", "This prize plus another spin.", "2X"),
                    Reward::new("Stream Takeover", "Pick a major stream moment.", "KING"),
                ],
            }),
        }
    }
}

fn parse_hex_color(text: &str) -> Option<AppColor> {
    let digits = text.trim().trim_start_matches('#');
    let expand = |value: u8| value * 17;
    match digits.len() {
        3 => {
            let mut nibbles = digits.chars().map(|c| c.to_digit(16).map(|v| v as u8));
            Some(AppColor::from_rgb(
                expand(nibbles.next()??),
                expand(nibbles.next()??),
                expand(nibbles.next()??),
            ))
        }
        6 | 8 => {
            let offset = digits.len() - 6;
            let byte = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
            Some(AppColor::from_rgb(
                byte(offset)?,
                byte(offset + 2)?,
                byte(offset + 4)?,
            ))
        }
        _ => None,
    }
}

/// Mirrors the dashboard's generated-asset labels. Uploaded images collapse to
/// a text badge because the case renderer has no texture channel of its own.
fn badge_for_asset(kind: Option<&str>, value: Option<&str>, fallback: &str) -> String {
    let value = value.unwrap_or("").trim();
    match kind {
        Some("emoji") if !value.is_empty() => value.chars().take(2).collect(),
        Some("generated") => match value {
            "camera" => "CAM",
            "challenge" => "CHAL",
            "crown" => "KING",
            "double" => "2X",
            "legendary" => "LEG",
            "loadout" => "KIT",
            "modifier" => "MOD",
            "name" => "TAG",
            "party" => "PLAY",
            // The dashboard's queue icon is for music requests.
            "queue" => "SONG",
            "reroll" => "ROLL",
            "scene" => "LIVE",
            "spin" => "SPIN",
            "tokens" => "TOK",
            "vip" => "VIP",
            "voice" => "VOX",
            _ => "DROP",
        }
        .to_string(),
        _ => initials(fallback),
    }
}

/// Falls back to the prize's own initials so unknown assets still read.
fn initials(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .filter(|c| c.is_alphanumeric())
        .take(4)
        .collect();
    if letters.is_empty() {
        "DROP".to_string()
    } else {
        letters.to_uppercase()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardsFile {
    #[serde(default)]
    active_profile_id: Option<String>,
    #[serde(default)]
    global_settings: Option<RawGlobals>,
    #[serde(default)]
    profiles: Vec<RawProfile>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGlobals {
    #[serde(default)]
    currency_name: Option<String>,
    #[serde(default)]
    gamble_command: Option<String>,
    #[serde(default)]
    roll_cost: Option<u32>,
    #[serde(default)]
    show_odds: Option<bool>,
    #[serde(default)]
    show_profile_name: Option<bool>,
    #[serde(default)]
    extended_result_hold: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProfile {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    case_name: Option<String>,
    #[serde(default)]
    legendary_wheel_enabled: Option<bool>,
    #[serde(default)]
    hype_messages: Option<RawHype>,
    #[serde(default)]
    legendary_wheel: Option<RawWheel>,
    #[serde(default)]
    rarities: Vec<RawTier>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHype {
    #[serde(default)]
    opening: Option<String>,
    #[serde(default)]
    common: Option<String>,
    #[serde(default)]
    rare: Option<String>,
    #[serde(default)]
    legendary: Option<String>,
}

impl RawHype {
    fn resolve(&self) -> HypeMessages {
        let defaults = HypeMessages::default();
        let pick = |value: &Option<String>, fallback: String| {
            value
                .as_deref()
                .map(|text| text.trim().to_uppercase())
                .filter(|text| !text.is_empty())
                .unwrap_or(fallback)
        };
        HypeMessages {
            opening: pick(&self.opening, defaults.opening),
            common: pick(&self.common, defaults.common),
            rare: pick(&self.rare, defaults.rare),
            legendary: pick(&self.legendary, defaults.legendary),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWheel {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    spin_duration_ms: Option<f32>,
    #[serde(default)]
    prizes: Vec<RawReward>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTier {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    odds: Option<f64>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    rewards: Vec<RawReward>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReward {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    asset: Option<RawAsset>,
}

impl RawReward {
    fn resolve(&self) -> Reward {
        let name = self
            .name
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| "Mystery Drop".to_string());
        Reward {
            badge: badge_for_asset(
                self.asset.as_ref().and_then(|asset| asset.kind.as_deref()),
                self.asset.as_ref().and_then(|asset| asset.value.as_deref()),
                &name,
            ),
            description: self.description.clone().unwrap_or_default(),
            name,
        }
    }
}

#[derive(Deserialize)]
struct RawAsset {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_case_uses_the_counter_strike_distribution() {
        let config = CaseConfig::default();
        assert_eq!(config.tiers.len(), 5);
        assert!((config.total_odds() - 100.0).abs() < 1e-6);
        assert_eq!(config.tiers[0].odds, 79.92);
        assert_eq!(config.tiers[4].odds, 0.26);
    }

    #[test]
    fn weighted_roll_matches_the_configured_odds() {
        let config = CaseConfig::default();
        let mut rng = Rng::from_seed(0xBADC0DE);
        let mut counts = [0usize; 5];
        let samples = 400_000;
        for _ in 0..samples {
            counts[config.roll(&mut rng, None, None).tier_index] += 1;
        }
        for (index, tier) in config.tiers.iter().enumerate() {
            let observed = counts[index] as f64 / samples as f64 * 100.0;
            let tolerance = (tier.odds * 0.12).max(0.06);
            assert!(
                (observed - tier.odds).abs() < tolerance,
                "{} drew {observed:.3}% but is configured for {:.2}%",
                tier.name,
                tier.odds
            );
        }
    }

    #[test]
    fn a_seed_replays_the_same_drop() {
        let config = CaseConfig::default();
        let first = config.roll(&mut Rng::from_seed(42), None, None);
        let second = config.roll(&mut Rng::from_seed(42), None, None);
        assert_eq!(first, second);
    }

    #[test]
    fn forced_results_win_over_the_dice() {
        let config = CaseConfig::default();
        let mut rng = Rng::from_seed(1);
        let outcome = config.roll(&mut rng, Some("rare_special"), Some("legendary drop"));
        assert_eq!(config.tiers[outcome.tier_index].id, "rare_special");
        assert_eq!(config.reward(outcome).name, "Legendary Drop");
    }

    #[test]
    fn forcing_by_display_name_also_works() {
        let config = CaseConfig::default();
        let mut rng = Rng::from_seed(1);
        let outcome = config.roll(&mut rng, Some("Covert"), None);
        assert_eq!(config.tiers[outcome.tier_index].id, "covert");
    }

    #[test]
    fn dashboard_json_round_trips_into_a_case() {
        let text = r##"{
            "schemaVersion": 2,
            "activeProfileId": "arc",
            "globalSettings": { "rollCost": 2500, "currencyName": "arcbucks", "showOdds": false, "gambleCommand": "!crate" },
            "profiles": [{
                "id": "arc",
                "name": "Arc Punishment Crate",
                "caseName": "ARC PUNISHMENT CRATE",
                "legendaryWheelEnabled": true,
                "hypeMessages": { "opening": "spinning it up" },
                "legendaryWheel": {
                    "title": "Punishment Wheel",
                    "spinDurationMs": 6000,
                    "prizes": [{ "name": "Cold Shower", "asset": { "type": "generated", "value": "crown" } }]
                },
                "rarities": [
                    {
                        "id": "light", "name": "Light Punishment", "odds": 90, "color": "#5bbcff",
                        "rewards": [{ "name": "25 Pushups", "description": "Do it now.", "asset": { "type": "generated", "value": "tokens" } }]
                    },
                    {
                        "id": "heavy", "name": "Heavy Punishment", "odds": 10, "color": "f38",
                        "rewards": [{ "name": "Hot Sauce Shot" }]
                    }
                ]
            }]
        }"##;
        let config = CaseConfig::from_rewards_json(text, None).expect("config should parse");
        assert_eq!(config.case_name, "ARC PUNISHMENT CRATE");
        assert_eq!(config.roll_cost, 2500);
        assert_eq!(config.currency_name, "arcbucks");
        assert_eq!(config.chat_command, "!crate");
        assert!(!config.show_odds);
        assert_eq!(config.hype.opening, "SPINNING IT UP");
        assert_eq!(config.tiers.len(), 2);
        assert_eq!(config.tiers[0].color, AppColor::from_rgb(0x5b, 0xbc, 0xff));
        assert_eq!(config.tiers[0].rewards[0].badge, "TOK");
        // Short hex and a missing asset both need to survive.
        assert_eq!(config.tiers[1].color, AppColor::from_rgb(0xff, 0x33, 0x88));
        assert_eq!(config.tiers[1].rewards[0].badge, "HSS");
        let wheel = config.wheel.expect("wheel should parse");
        assert_eq!(wheel.title, "Punishment Wheel");
        assert!((wheel.spin_seconds - 6.0).abs() < 1e-6);
    }

    #[test]
    fn a_named_profile_overrides_the_active_one() {
        let text = r#"{
            "activeProfileId": "a",
            "profiles": [
                { "id": "a", "name": "A", "rarities": [{ "id": "t", "odds": 1, "rewards": [{ "name": "x" }] }] },
                { "id": "b", "name": "B", "rarities": [{ "id": "t", "odds": 1, "rewards": [{ "name": "y" }] }] }
            ]
        }"#;
        let config = CaseConfig::from_rewards_json(text, Some("b")).expect("config should parse");
        assert_eq!(config.profile_name, "B");
    }

    #[test]
    fn empty_and_broken_configs_report_instead_of_panicking() {
        assert!(CaseConfig::from_rewards_json("{}", None).is_err());
        assert!(CaseConfig::from_rewards_json("not json", None).is_err());
        assert!(
            CaseConfig::from_rewards_json(
                r#"{"profiles":[{"id":"a","rarities":[{"id":"t","odds":0,"rewards":[{"name":"x"}]}]}]}"#,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn tiers_without_prizes_are_dropped_rather_than_crashing_a_roll() {
        let text = r#"{
            "profiles": [{ "id": "a", "rarities": [
                { "id": "empty", "odds": 50, "rewards": [] },
                { "id": "real", "odds": 50, "rewards": [{ "name": "Prize" }] }
            ]}]
        }"#;
        let config = CaseConfig::from_rewards_json(text, None).expect("config should parse");
        assert_eq!(config.tiers.len(), 1);
        assert_eq!(config.tiers[0].id, "real");
    }

    #[test]
    fn intensity_spans_the_tier_ladder() {
        let config = CaseConfig::default();
        let count = config.tiers.len();
        assert_eq!(config.tiers[0].intensity(count), 0.0);
        assert_eq!(config.tiers[count - 1].intensity(count), 1.0);
    }
}
