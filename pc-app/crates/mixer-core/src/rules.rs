use crate::config::{ChannelKind, MixerConfig, RuleMatch};

/// Works out which playback channel an app belongs to.
///
/// Order: exact process name rules, then path rules (first match wins), then
/// the default channel. Returns `None` when the app should be left alone.
pub fn resolve<'a>(
    config: &'a MixerConfig,
    process_name: &str,
    exe_path: Option<&str>,
    is_system_sounds: bool,
) -> Option<&'a str> {
    let target = if is_system_sounds {
        if config.control_system_sounds {
            config.default_channel.as_deref()
        } else {
            None
        }
    } else {
        match_rules(config, process_name, exe_path)
    };
    // A rule pointing at a missing or capture channel must not swallow the app.
    target.filter(|id| {
        config
            .channel(id)
            .is_some_and(|c| c.kind == ChannelKind::Playback)
    })
}

fn match_rules<'a>(
    config: &'a MixerConfig,
    process_name: &str,
    exe_path: Option<&str>,
) -> Option<&'a str> {
    let name = normalize_process_name(process_name);
    if let Some(rule) = config.rules.iter().find(|r| {
        r.match_kind == RuleMatch::ProcessName && normalize_process_name(&r.pattern) == name
    }) {
        return rule.channel.as_deref();
    }

    if let Some(path) = exe_path.map(normalize_path)
        && let Some(rule) = config.rules.iter().find(|r| {
            r.match_kind == RuleMatch::PathContains && path.contains(&normalize_path(&r.pattern))
        })
    {
        return rule.channel.as_deref();
    }

    config.default_channel.as_deref()
}

/// Lowercase, with `.exe` added when the pattern has no extension.
pub fn normalize_process_name(name: &str) -> String {
    let name = name.trim().to_lowercase();
    if name.contains('.') {
        name
    } else {
        format!("{name}.exe")
    }
}

fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppRule;

    fn config() -> MixerConfig {
        MixerConfig::default()
    }

    #[test]
    fn process_name_is_case_insensitive() {
        assert_eq!(resolve(&config(), "Discord.exe", None, false), Some("chat"));
    }

    #[test]
    fn rule_pattern_without_extension_matches_exe() {
        let mut c = config();
        c.rules.push(AppRule::process("slack", "chat"));
        assert_eq!(resolve(&c, "slack.exe", None, false), Some("chat"));
    }

    #[test]
    fn path_rule_matches_game_folder() {
        let path = r"D:\SteamLibrary\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe";
        assert_eq!(
            resolve(&config(), "cs2.exe", Some(path), false),
            Some("game")
        );
    }

    #[test]
    fn path_rule_accepts_forward_slashes() {
        let mut c = config();
        c.rules.push(AppRule::path("C:/Games/", "game"));
        assert_eq!(
            resolve(&c, "x.exe", Some(r"C:\Games\x.exe"), false),
            Some("game")
        );
    }

    #[test]
    fn process_rule_beats_path_rule() {
        let mut c = config();
        c.rules.push(AppRule::process("overlay.exe", "chat"));
        let path = r"C:\steamapps\common\Game\overlay.exe";
        assert_eq!(resolve(&c, "overlay.exe", Some(path), false), Some("chat"));
    }

    #[test]
    fn unmatched_app_goes_to_default() {
        assert_eq!(
            resolve(
                &config(),
                "chrome.exe",
                Some(r"C:\Program Files\chrome.exe"),
                false
            ),
            Some("media")
        );
    }

    #[test]
    fn no_default_leaves_unmatched_app_alone() {
        let mut c = config();
        c.default_channel = None;
        assert_eq!(resolve(&c, "chrome.exe", None, false), None);
    }

    #[test]
    fn uncontrolled_rule_leaves_app_alone() {
        let mut c = config();
        c.rules.push(AppRule {
            match_kind: RuleMatch::ProcessName,
            pattern: "obs64.exe".into(),
            channel: None,
        });
        assert_eq!(resolve(&c, "obs64.exe", None, false), None);
    }

    #[test]
    fn system_sounds_are_left_alone_by_default() {
        assert_eq!(resolve(&config(), "", None, true), None);
        let mut c = config();
        c.control_system_sounds = true;
        assert_eq!(resolve(&c, "", None, true), Some("media"));
    }

    #[test]
    fn rule_to_capture_or_missing_channel_is_ignored() {
        let mut c = config();
        c.rules.push(AppRule::process("a.exe", "mic"));
        c.rules.push(AppRule::process("b.exe", "nope"));
        assert_eq!(resolve(&c, "a.exe", None, false), None);
        assert_eq!(resolve(&c, "b.exe", None, false), None);
    }
}
