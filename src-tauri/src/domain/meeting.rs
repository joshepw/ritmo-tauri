use regex::Regex;
use std::sync::LazyLock;

use crate::domain::preferences::LinkOpener;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeetingLink {
    pub provider_id: String,
    pub name: String,
    pub short: String,
    pub abbr: String,
    pub color: String,
    pub url: String,
    pub scheme: String,
    pub kind: MeetingKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetingKind {
    Video,
    Phone,
    Room,
    None,
}

impl MeetingLink {
    pub fn none() -> Self {
        Self {
            provider_id: "none".into(),
            name: "Sin enlace".into(),
            short: "—".into(),
            abbr: "·".into(),
            color: "rgba(255,255,255,.18)".into(),
            url: String::new(),
            scheme: String::new(),
            kind: MeetingKind::None,
        }
    }
}

struct Provider {
    id: &'static str,
    name: &'static str,
    short: &'static str,
    abbr: &'static str,
    color: &'static str,
    pattern: Regex,
    scheme: &'static str,
}

fn providers() -> &'static [Provider] {
    static PROVIDERS: LazyLock<Vec<Provider>> = LazyLock::new(|| {
        vec![
            Provider {
                id: "hangouts",
                name: "Google Hangouts",
                short: "Hangouts",
                abbr: "H",
                color: "#0f9d58",
                pattern: Regex::new(r"(?i)https?://hangouts\.google\.com/[^\s<>]+").unwrap(),
                scheme: "",
            },
            Provider {
                id: "meet",
                name: "Google Meet",
                short: "Meet",
                abbr: "M",
                color: "#1e8e3e",
                pattern: Regex::new(r#"(?i)https?://meet\.google\.com/[^\s<>"']+"#).unwrap(),
                scheme: "",
            },
            Provider {
                id: "zoom",
                name: "Zoom",
                short: "Zoom",
                abbr: "Z",
                color: "#2d8cff",
                pattern: Regex::new(r"(?i)https?://[\w.-]*zoom\.us/(j|my|w)/[\w?=&.-]+").unwrap(),
                scheme: "zoommtg://",
            },
            Provider {
                id: "teams",
                name: "Microsoft Teams",
                short: "Teams",
                abbr: "T",
                color: "#5b5fc7",
                pattern: Regex::new(r"(?i)https?://teams\.(microsoft|live)\.com/[^\s<>]+").unwrap(),
                scheme: "msteams://",
            },
            Provider {
                id: "webex",
                name: "Webex",
                short: "Webex",
                abbr: "W",
                color: "#00a0d1",
                pattern: Regex::new(r"(?i)https?://[\w.-]*webex\.com/[^\s<>]+").unwrap(),
                scheme: "webex://",
            },
            Provider {
                id: "slack",
                name: "Slack Huddle",
                short: "Slack",
                abbr: "S",
                color: "#e01e5a",
                pattern: Regex::new(r"(?i)https?://app\.slack\.com/huddle/[^\s<>]+").unwrap(),
                scheme: "slack://",
            },
            Provider {
                id: "jitsi",
                name: "Jitsi",
                short: "Jitsi",
                abbr: "J",
                color: "#1d76ba",
                pattern: Regex::new(r"(?i)https?://meet\.jit\.si/[^\s<>]+").unwrap(),
                scheme: "",
            },
            Provider {
                id: "whereby",
                name: "Whereby",
                short: "Whereby",
                abbr: "W",
                color: "#6b4eff",
                pattern: Regex::new(r"(?i)https?://whereby\.com/[^\s<>]+").unwrap(),
                scheme: "",
            },
            Provider {
                id: "goto",
                name: "GoTo Meeting",
                short: "GoTo",
                abbr: "G",
                color: "#f68d2e",
                pattern: Regex::new(
                    r"(?i)https?://(meet\.goto\.com|[\w.]*gotomeeting\.com)/[^\s<>]+",
                )
                .unwrap(),
                scheme: "gotomeeting://",
            },
            Provider {
                id: "chime",
                name: "Amazon Chime",
                short: "Chime",
                abbr: "C",
                color: "#d48a00",
                pattern: Regex::new(r"(?i)https?://chime\.aws/\d+").unwrap(),
                scheme: "chime://",
            },
            Provider {
                id: "discord",
                name: "Discord",
                short: "Discord",
                abbr: "D",
                color: "#5865f2",
                pattern: Regex::new(r"(?i)https?://discord\.(gg|com)/[^\s<>]+").unwrap(),
                scheme: "discord://",
            },
            Provider {
                id: "facetime",
                name: "FaceTime",
                short: "FaceTime",
                abbr: "F",
                color: "#30b94d",
                pattern: Regex::new(r"(?i)https?://facetime\.apple\.com/join[^\s<>]+").unwrap(),
                scheme: "",
            },
        ]
    });
    &PROVIDERS
}

static PHONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(tel:)?\+\d[\d\s-]{7,}\d").unwrap());
static ROOM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(sala|oficina|piso|edificio|calle|avenida|av\.|presencial)\b").unwrap()
});
static ZOOM_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)zoom\.us/(?:j|my|w)/(\d+)").unwrap());
static ZOOM_PWD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)[?&]pwd=([^&\s]+)").unwrap());

pub fn detect_meeting(text: &str) -> MeetingLink {
    for provider in providers() {
            if let Some(found) = provider.pattern.find(text) {
            let url = found.as_str().trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ')' | ';' | ']'));
            return MeetingLink {
                provider_id: provider.id.into(),
                name: provider.name.into(),
                short: provider.short.into(),
                abbr: provider.abbr.into(),
                color: provider.color.into(),
                url: url.to_string(),
                scheme: provider.scheme.into(),
                kind: MeetingKind::Video,
            };
        }
    }
    if let Some(found) = PHONE.find(text) {
        let digits = found
            .as_str()
            .chars()
            .filter(|ch| ch.is_ascii_digit() || *ch == '+')
            .collect::<String>();
        return MeetingLink {
            provider_id: "phone".into(),
            name: "Llamada telefónica".into(),
            short: "Tel.".into(),
            abbr: "☎︎".into(),
            color: "#8e8e93".into(),
            url: format!("tel:{digits}"),
            scheme: String::new(),
            kind: MeetingKind::Phone,
        };
    }
    if ROOM.is_match(text) {
        return MeetingLink {
            provider_id: "room".into(),
            name: "Presencial".into(),
            short: "Sala".into(),
            abbr: "⌂".into(),
            color: "#ff9f0a".into(),
            url: String::new(),
            scheme: String::new(),
            kind: MeetingKind::Room,
        };
    }
    MeetingLink::none()
}

pub struct LaunchPlan {
    pub target: String,
    pub app_name: Option<&'static str>,
}

pub fn open_target(link: &MeetingLink, opener: LinkOpener) -> Option<LaunchPlan> {
    if link.url.is_empty() {
        return None;
    }
    if link.kind == MeetingKind::Phone {
        return Some(LaunchPlan {
            target: link.url.clone(),
            app_name: None,
        });
    }
    let target = if opener == LinkOpener::NativeApp {
        native_url(link)
    } else {
        link.url.clone()
    };
    Some(LaunchPlan {
        target,
        app_name: opener.app_name(),
    })
}

pub fn native_url(link: &MeetingLink) -> String {
    if link.provider_id == "zoom" {
        if let Some(id) = ZOOM_ID.captures(&link.url).and_then(|caps| caps.get(1)) {
            let mut url = format!("zoommtg://zoom.us/join?confno={}", id.as_str());
            if let Some(pwd) = ZOOM_PWD.captures(&link.url).and_then(|caps| caps.get(1)) {
                url.push_str("&pwd=");
                url.push_str(pwd.as_str());
            }
            return url;
        }
    }
    if link.scheme.is_empty() {
        return link.url.clone();
    }
    if let Some(rest) = link.url.strip_prefix("https://") {
        return format!("{}{rest}", link.scheme);
    }
    if let Some(rest) = link.url.strip_prefix("http://") {
        return format!("{}{rest}", link.scheme);
    }
    link.url.clone()
}

pub fn open_message(link: &MeetingLink, opener: LinkOpener) -> String {
    if link.kind == MeetingKind::Phone {
        return format!("Llamando · {}", link.url.trim_start_matches("tel:"));
    }
    let where_ = if opener == LinkOpener::NativeApp && !link.scheme.is_empty() {
        format!("la app de {}", link.name)
    } else if opener == LinkOpener::NativeApp {
        "el navegador por defecto".into()
    } else {
        opener.label().into()
    };
    format!("Abriendo en {where_}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_zoom_and_builds_native_url() {
        let link = detect_meeting(
            "Enlace: https://place2pay.zoom.us/j/84512093321?pwd=x1 y sala de espera",
        );
        assert_eq!(link.provider_id, "zoom");
        assert_eq!(
            open_target(&link, LinkOpener::NativeApp).unwrap().target,
            "zoommtg://zoom.us/join?confno=84512093321&pwd=x1"
        );
        let browser = open_target(&link, LinkOpener::Chrome).unwrap();
        assert_eq!(browser.target, "https://place2pay.zoom.us/j/84512093321?pwd=x1");
        assert_eq!(browser.app_name, Some("Google Chrome"));
    }

    #[test]
    fn detects_meet_teams_phone_and_room() {
        let meet = detect_meeting("https://meet.google.com/abc-defg-hij?hs=122");
        assert_eq!(meet.provider_id, "meet");
        assert_eq!(meet.url, "https://meet.google.com/abc-defg-hij?hs=122");
        assert_eq!(
            detect_meeting("https://meet.google.com/lookup/abcdefgh").provider_id,
            "meet"
        );
        assert_eq!(
            open_target(&meet, LinkOpener::Chrome).unwrap().app_name,
            Some("Google Chrome")
        );
        assert_eq!(
            detect_meeting("https://hangouts.google.com/call/abc").provider_id,
            "hangouts"
        );
        let phone = detect_meeting("Marcar al +502 2245 8810");
        assert_eq!(phone.kind, MeetingKind::Phone);
        assert!(open_target(&phone, LinkOpener::Chrome).unwrap().app_name.is_none());
        let teams = detect_meeting("https://teams.microsoft.com/l/meetup-join/19%3ameeting_PX");
        assert_eq!(teams.provider_id, "teams");
        assert!(open_target(&teams, LinkOpener::NativeApp)
            .unwrap()
            .target
            .starts_with("msteams://teams.microsoft.com/"));
        assert_eq!(
            detect_meeting("Oficina · Sala 3, piso 2").kind,
            MeetingKind::Room
        );
    }
}
