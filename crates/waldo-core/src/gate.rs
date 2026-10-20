//! The four hard stops: money, sending as you, passwords and keys, and
//! destructive changes.
//!
//! The model does not decide whether a hard stop applies. This file does,
//! from facts read off the screen: the app, the page, the label on the
//! thing about to be clicked, and the key about to be pressed. When a rule
//! matches, macOS shows a dialog that only the person can answer.
// The constants keep the names Apple gives them.
#![allow(non_upper_case_globals)]

use crate::cf::Cf;
use crate::ffi::*;
use crate::input::parse_chord;
use crate::state;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::ptr::null;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gate {
    Money,
    Sending,
    Passwords,
    Destructive,
}

impl Gate {
    pub fn parse(s: &str) -> Option<Gate> {
        Some(match s.trim().to_lowercase().as_str() {
            "money" => Gate::Money,
            "sending" => Gate::Sending,
            "passwords" => Gate::Passwords,
            "destructive" => Gate::Destructive,
            _ => return None,
        })
    }

    pub fn id(&self) -> &'static str {
        match self {
            Gate::Money => "money",
            Gate::Sending => "sending",
            Gate::Passwords => "passwords",
            Gate::Destructive => "destructive",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Gate::Money => "Money",
            Gate::Sending => "Sending as you",
            Gate::Passwords => "Passwords and keys",
            Gate::Destructive => "Destructive changes",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Words on the thing clicked, or on the default button when Return is
    /// pressed. A rule matches whole words in order.
    pub labels: Vec<String>,
    /// Key presses. `cmd+shift+d@com.apple.mail` limits one to an app.
    pub chords: Vec<String>,
    /// Bundle ids where any click or key press stops first.
    pub apps: Vec<String>,
    /// Parts of a web address or window title where any click or key press
    /// stops first.
    pub places: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub money: Rules,
    pub sending: Rules,
    pub passwords: Rules,
    pub destructive: Rules,
    /// Bundle ids of apps where Return sends a message.
    pub chat_apps: Vec<String>,
    /// Parts of a web address where Return sends a message.
    pub chat_places: Vec<String>,
    /// Labels of places where a drop destroys the thing dropped.
    pub drop_targets: Vec<String>,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl Default for Policy {
    fn default() -> Policy {
        Policy {
            money: Rules {
                labels: strings(&[
                    "pay", "buy", "purchase", "place order", "place your order", "order now",
                    "complete order", "complete purchase", "confirm order", "confirm purchase",
                    "confirm payment", "submit order", "submit payment", "send money",
                    "send payment", "transfer", "make payment", "make a payment", "checkout",
                    "check out", "subscribe", "start subscription", "upgrade now", "upgrade plan",
                    "donate", "withdraw", "deposit", "place bid", "confirm bid", "place trade",
                    "confirm trade", "sell", "swap", "top up", "add funds",
                ]),
                chords: vec![],
                apps: strings(&["com.apple.Passbook"]),
                places: strings(&[
                    "commbank.com.au", "netbank", "nab.com.au", "anz.com", "westpac.com.au",
                    "stgeorge.com.au", "macquarie.com", "ing.com.au", "up.com.au",
                    "bankwest.com.au", "bendigobank.com.au", "suncorp.com.au", "wise.com",
                    "revolut.com", "paypal.com", "checkout.stripe.com", "pay.google.com",
                    "binance.com", "coinbase.com", "kraken.com", "swyftx.com", "coinspot.com.au",
                    "commsec.com.au", "interactivebrokers", "stake.com", "afterpay.com",
                    "zip.co", "klarna.com", "chase.com", "bankofamerica.com", "wellsfargo.com",
                    "citi.com", "capitalone.com", "americanexpress.com", "schwab.com",
                    "fidelity.com", "vanguard.com", "robinhood.com", "etrade.com", "venmo.com",
                    "cash.app", "hsbc.", "barclays.", "lloydsbank.", "natwest.com", "monzo.com",
                    "starlingbank.com", "n26.com", "santander.", "bnpparibas", "dbs.com",
                    "/checkout", "/payment",
                ]),
            },
            sending: Rules {
                labels: strings(&[
                    "send", "post", "publish", "tweet", "reply", "reply all", "submit", "share",
                    "comment", "invite", "apply now", "submit application", "sign and send",
                    "finish and send", "send for signature",
                ]),
                chords: strings(&["cmd+return", "cmd+shift+d@com.apple.mail"]),
                apps: vec![],
                places: vec![],
            },
            passwords: Rules {
                labels: strings(&[
                    "copy password", "show password", "reveal password", "export passwords",
                    "copy passkey", "recovery key", "seed phrase", "private key", "secret key",
                    "copy token", "reveal secret", "show secret", "autofill password",
                ]),
                chords: vec![],
                apps: strings(&[
                    "com.apple.keychainaccess", "com.apple.Passwords", "com.1password.1password",
                    "com.agilebits.onepassword7", "com.bitwarden.desktop", "com.lastpass.LastPass",
                    "org.keepassxc.keepassxc", "com.dashlane.Dashlane", "com.apple.SecurityAgent",
                    "com.apple.LocalAuthentication.UIAgent", "com.apple.loginwindow",
                ]),
                places: strings(&[
                    "passwords.google.com", "my.1password.com", "vault.bitwarden.com",
                    "chrome://password-manager", "account.apple.com",
                ]),
            },
            destructive: Rules {
                labels: strings(&[
                    "delete", "erase", "remove", "empty trash", "empty bin", "move to trash",
                    "move to bin", "format", "reset", "factory reset", "uninstall", "discard",
                    "don't save", "revoke", "deactivate", "close account", "force quit",
                    "overwrite", "replace", "shut down", "restart", "log out", "sign out",
                    "turn off filevault", "turn off find my", "clear history",
                    "clear browsing data", "clear data", "clear all", "wipe", "terminate",
                    "cancel subscription", "cancel plan",
                ]),
                chords: strings(&[
                    "cmd+delete@com.apple.finder", "cmd+shift+delete", "cmd+alt+shift+delete",
                    "cmd+alt+escape", "ctrl+cmd+q", "cmd+shift+q", "cmd+alt+shift+q",
                ]),
                apps: strings(&["com.apple.DiskUtility"]),
                places: vec![],
            },
            chat_apps: strings(&[
                "com.apple.MobileSMS", "net.whatsapp.WhatsApp", "com.tinyspeck.slackmacgap",
                "ru.keepcoder.Telegram", "org.telegram.desktop",
                "org.whispersystems.signal-desktop", "com.hnc.Discord", "com.microsoft.teams2",
                "com.microsoft.teams", "com.facebook.archon",
            ]),
            chat_places: strings(&[
                "web.whatsapp.com", "messenger.com", "linkedin.com/messaging",
                "instagram.com/direct", "discord.com/channels", "app.slack.com",
                "teams.microsoft.com", "web.telegram.org", "messages.google.com",
            ]),
            drop_targets: strings(&["trash", "bin"]),
        }
    }
}

impl Policy {
    /// The built-in policy, or `policy.json` from the state folder if the
    /// person has written one. A file that does not parse is an error, so a
    /// typo can never switch the gates off without a word.
    pub fn load() -> Result<Policy, String> {
        let path = state::dir().join("policy.json");
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} does not parse: {e}", path.display())),
            Err(_) => Ok(Policy::default()),
        }
    }

    fn rules(&self, gate: Gate) -> &Rules {
        match gate {
            Gate::Money => &self.money,
            Gate::Sending => &self.sending,
            Gate::Passwords => &self.passwords,
            Gate::Destructive => &self.destructive,
        }
    }
}

/// Order matters when two gates match. The first one names the dialog.
const ORDER: [Gate; 4] = [Gate::Money, Gate::Passwords, Gate::Destructive, Gate::Sending];

/// A label next to the role of the element that carries it.
#[derive(Clone, Debug, Default)]
pub struct Labeled {
    pub label: String,
    /// True for buttons, links, menu items, and the like.
    pub control: bool,
}

/// The facts about one action, read off the screen before it happens.
#[derive(Clone, Debug, Default)]
pub struct Act {
    /// `click`, `type`, `key`, `drag`, or `scroll`.
    pub verb: String,
    pub app_name: String,
    pub app_bundle: String,
    pub window: String,
    pub url: String,
    /// For a click: the target and what encloses it. For Return: the
    /// default button of the window.
    pub labels: Vec<Labeled>,
    /// For a drag: labels at the place the drop lands.
    pub drop_labels: Vec<String>,
    /// Canonical chord name, such as `shift+cmd+d`.
    pub chord: Option<String>,
    /// Short role of the element that has keyboard focus.
    pub focus_role: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub gate: Gate,
    pub why: String,
    /// Set when the rule is about where the action happens, not what it is.
    /// Holds the app or place, so one yes can cover a few minutes there.
    pub place: Option<String>,
}

/// Lowercase words, with punctuation dropped. `Don’t Save…` gives
/// `["don't", "save"]`.
pub fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        let ch = if ch == '’' { '\'' } else { ch };
        if ch.is_alphanumeric() || ch == '\'' {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// True when the words of `phrase` appear in `label` in order, as whole
/// words. Long labels are prose, not buttons, and never match.
pub fn label_matches(label: &str, phrase: &str) -> bool {
    let l = words(label);
    let p = words(phrase);
    if p.is_empty() || l.is_empty() || l.len() > 12 {
        return false;
    }
    l.windows(p.len()).any(|w| w == p.as_slice())
}

fn canonical_chord(s: &str) -> Option<String> {
    parse_chord(s).ok().map(|c| c.name)
}

fn chord_rule_matches(rule: &str, chord: &str, bundle: &str) -> bool {
    let (keys, app) = match rule.split_once('@') {
        Some((k, a)) => (k, Some(a)),
        None => (rule, None),
    };
    if let Some(a) = app {
        if !a.eq_ignore_ascii_case(bundle) {
            return false;
        }
    }
    canonical_chord(keys).as_deref() == Some(chord)
}

fn is_return(chord: &str) -> bool {
    chord == "return"
}

/// Proof that the gate looked at an act and let it through.
///
/// Only [`admit`] can make one, and every function that sends input or
/// changes an element asks for one. So an act that skips the gate does not
/// compile:
///
/// ```compile_fail
/// let t = waldo_core::gate::Ticket { _sealed: () };
/// ```
pub struct Ticket {
    _sealed: (),
}

/// Runs the gate for one act. When a rule hits, `decide` asks the person;
/// a ticket comes back only when the act may go ahead.
pub fn admit(policy: &Policy, act: &Act, decide: impl FnOnce(Hit) -> Result<(), String>) -> Result<Ticket, String> {
    if let Some(hit) = evaluate(policy, act) {
        decide(hit)?;
    }
    Ok(Ticket { _sealed: () })
}

pub fn evaluate(policy: &Policy, act: &Act) -> Option<Hit> {
    // Looking is never gated.
    if act.verb == "scroll" {
        return None;
    }
    let bundle = act.app_bundle.as_str();
    let place_text = format!("{} {}", act.url, act.window).to_lowercase();

    // 1. Typing into a password field.
    if (act.verb == "type" || act.chord.as_deref() == Some("cmd+v")) && act.focus_role == "securefield"
    {
        return Some(Hit {
            gate: Gate::Passwords,
            why: "the focus is in a password field".into(),
            place: None,
        });
    }

    // 2. What the action is: a label or a key press.
    for gate in ORDER {
        let rules = policy.rules(gate);
        for item in act.labels.iter().filter(|l| l.control) {
            if let Some(p) = rules.labels.iter().find(|p| label_matches(&item.label, p)) {
                return Some(Hit {
                    gate,
                    why: format!("the label \"{}\" matches \"{}\"", item.label, p),
                    place: None,
                });
            }
        }
        if let Some(chord) = &act.chord {
            if let Some(r) = rules.chords.iter().find(|r| chord_rule_matches(r, chord, bundle)) {
                return Some(Hit { gate, why: format!("the key {chord} matches \"{r}\""), place: None });
            }
        }
    }

    // 3. Return in a chat sends the message.
    if let Some(chord) = &act.chord {
        if is_return(chord) && act.focus_role != "searchfield" {
            let chat_app = policy.chat_apps.iter().any(|a| a.eq_ignore_ascii_case(bundle));
            let chat_place = policy
                .chat_places
                .iter()
                .any(|p| !act.url.is_empty() && act.url.to_lowercase().contains(&p.to_lowercase()));
            if chat_app || chat_place {
                return Some(Hit {
                    gate: Gate::Sending,
                    why: "Return sends the message in a chat".into(),
                    place: None,
                });
            }
        }
    }

    // 4. A drop onto the Trash.
    if act.verb == "drag" {
        for l in &act.drop_labels {
            let w = words(l);
            if policy.drop_targets.iter().any(|t| w.iter().any(|x| x == &t.to_lowercase())) {
                return Some(Hit {
                    gate: Gate::Destructive,
                    why: format!("the drop lands on \"{l}\""),
                    place: None,
                });
            }
        }
    }

    // 5. Where the action happens.
    for gate in ORDER {
        let rules = policy.rules(gate);
        if let Some(a) = rules.apps.iter().find(|a| a.eq_ignore_ascii_case(bundle)) {
            return Some(Hit {
                gate,
                why: format!("the app is {}", act.app_name),
                place: Some(a.to_lowercase()),
            });
        }
        if let Some(p) = rules.places.iter().find(|p| place_text.contains(&p.to_lowercase())) {
            return Some(Hit { gate, why: format!("the page is on {p}"), place: Some(p.to_lowercase()) });
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer {
    Deny,
    Once,
    /// Yes for this app or place for a few minutes.
    Minutes(u64),
    /// No, and stop every session until the person resumes.
    Stop,
}

pub const WINDOW_MINUTES: u64 = 10;

fn pending_file() -> std::path::PathBuf {
    state::dir().join("gate.pending")
}

/// True while a dialog is waiting for the person. No input may be posted
/// while this holds, so the agent can never answer its own question.
pub fn pending() -> bool {
    let path = pending_file();
    let Ok(text) = fs::read_to_string(&path) else { return false };
    let started: u64 = text.trim().parse().unwrap_or(0);
    if state::unix_seconds().saturating_sub(started) > 900 {
        let _ = fs::remove_file(&path);
        return false;
    }
    true
}

/// The panel helper sits next to this binary.
fn helper() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let path = exe.parent()?.join("waldo-ask");
    path.is_file().then_some(path)
}

/// Shows the panel: it never takes the keyboard, and Allow takes only a
/// real click. Any exit code it does not define is a no.
#[allow(clippy::disallowed_methods)] // the gate panel is a separate helper process by design
fn ask_with_panel(
    helper: &std::path::Path,
    gate: Gate,
    summary: &str,
    place: Option<&str>,
    timeout: Duration,
) -> Answer {
    let mut cmd = std::process::Command::new(helper);
    cmd.arg("--title").arg(gate.title());
    cmd.arg("--body").arg(format!("Waldo wants to do this on your Mac:\n\n{summary}"));
    cmd.arg("--timeout").arg(timeout.as_secs().to_string());
    if place.is_some() {
        cmd.arg("--minutes").arg(WINDOW_MINUTES.to_string());
    }
    cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    match cmd.status().ok().and_then(|s| s.code()) {
        Some(10) if place.is_none() => Answer::Once,
        Some(11) if place.is_some() => Answer::Minutes(WINDOW_MINUTES),
        Some(12) => Answer::Stop,
        _ => Answer::Deny,
    }
}

/// The fallback when the panel helper is missing: a system alert. Return
/// and Escape both land on a no, so a stray key can never allow.
fn ask_with_alert(gate: Gate, summary: &str, place: Option<&str>, timeout: Duration) -> Answer {
    let header = Cf::string(&format!("Waldo: {}", gate.title()));
    let body = Cf::string(&format!(
        "Waldo wants to do this on your Mac:\n\n{summary}\n\nNo answer in {} seconds counts as no.",
        timeout.as_secs()
    ));
    let deny = Cf::string("Deny");
    let stop = Cf::string("Stop everything");
    let allow = Cf::string(&match place {
        Some(_) => format!("Allow for {WINDOW_MINUTES} minutes"),
        None => "Allow once".to_string(),
    });
    let mut response: CFOptionFlags = kCFUserNotificationCancelResponse;
    let rc = unsafe {
        CFUserNotificationDisplayAlert(
            timeout.as_secs_f64(),
            kCFUserNotificationCautionAlertLevel,
            null(),
            null(),
            null(),
            header.as_ptr(),
            body.as_ptr(),
            deny.as_ptr(),
            stop.as_ptr(),
            allow.as_ptr(),
            &mut response,
        )
    };
    if rc != 0 {
        return Answer::Deny;
    }
    match response & 0x3 {
        kCFUserNotificationOtherResponse if place.is_some() => Answer::Minutes(WINDOW_MINUTES),
        kCFUserNotificationOtherResponse => Answer::Once,
        kCFUserNotificationAlternateResponse => Answer::Stop,
        _ => Answer::Deny,
    }
}

/// Asks the person and waits. No answer in time is a no.
pub fn ask(gate: Gate, summary: &str, place: Option<&str>, timeout: Duration) -> Answer {
    let _ = fs::write(pending_file(), state::unix_seconds().to_string());
    let answer = match helper() {
        Some(h) => ask_with_panel(&h, gate, summary, place, timeout),
        None => ask_with_alert(gate, summary, place, timeout),
    };
    let _ = fs::remove_file(pending_file());
    state::audit(json!({
        "event": "gate",
        "gate": gate.id(),
        "summary": summary,
        "place": place,
        "answer": match answer {
            Answer::Deny => "deny",
            Answer::Once => "once",
            Answer::Minutes(_) => "minutes",
            Answer::Stop => "stop",
        },
    }));
    if answer == Answer::Stop {
        state::stop("stopped from a gate dialog");
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(label: &str) -> Vec<Labeled> {
        vec![Labeled { label: label.into(), control: true }]
    }

    fn click(label: &str) -> Act {
        Act { verb: "click".into(), labels: control(label), ..Default::default() }
    }

    fn key(chord: &str, bundle: &str) -> Act {
        Act {
            verb: "key".into(),
            chord: Some(parse_chord(chord).unwrap().name),
            app_bundle: bundle.into(),
            ..Default::default()
        }
    }

    fn gate_of(act: &Act) -> Option<Gate> {
        evaluate(&Policy::default(), act).map(|h| h.gate)
    }

    #[test]
    fn words_drop_punctuation_and_case() {
        assert_eq!(words("Don’t Save…"), vec!["don't", "save"]);
        assert_eq!(words("  Pay $42.00 now! "), vec!["pay", "42", "00", "now"]);
    }

    #[test]
    fn labels_match_whole_words_only() {
        assert!(label_matches("Pay now", "pay"));
        assert!(label_matches("Delete 3 items…", "delete"));
        assert!(label_matches("Move to Trash", "move to trash"));
        assert!(!label_matches("Payment settings", "pay"));
        assert!(!label_matches("Resend", "send"));
        assert!(!label_matches("Trash to move", "move to trash"));
    }

    #[test]
    fn long_prose_is_not_a_button() {
        let prose = "you can pay later if you prefer and we will send a reminder to you by email";
        assert!(!label_matches(prose, "pay"));
    }

    #[test]
    fn each_gate_catches_its_buttons() {
        assert_eq!(gate_of(&click("Place Order")), Some(Gate::Money));
        assert_eq!(gate_of(&click("Send")), Some(Gate::Sending));
        assert_eq!(gate_of(&click("Copy Password")), Some(Gate::Passwords));
        assert_eq!(gate_of(&click("Empty Trash")), Some(Gate::Destructive));
        assert_eq!(gate_of(&click("Don’t Save")), Some(Gate::Destructive));
    }

    #[test]
    fn plain_buttons_pass() {
        for label in ["OK", "Cancel", "Next", "Open", "Save", "7", "Search", "Back", "Sign in"] {
            assert_eq!(gate_of(&click(label)), None, "{label}");
        }
    }

    #[test]
    fn text_that_is_not_a_control_passes() {
        let act = Act {
            verb: "click".into(),
            labels: vec![Labeled { label: "Send".into(), control: false }],
            ..Default::default()
        };
        assert_eq!(gate_of(&act), None);
    }

    #[test]
    fn money_wins_over_sending() {
        assert_eq!(gate_of(&click("Submit payment")), Some(Gate::Money));
    }

    #[test]
    fn chords_respect_their_app() {
        assert_eq!(gate_of(&key("cmd+shift+d", "com.apple.mail")), Some(Gate::Sending));
        assert_eq!(gate_of(&key("cmd+shift+d", "com.apple.Safari")), None);
        assert_eq!(gate_of(&key("cmd+delete", "com.apple.finder")), Some(Gate::Destructive));
        assert_eq!(gate_of(&key("cmd+delete", "com.apple.TextEdit")), None);
        assert_eq!(gate_of(&key("command+enter", "com.anything")), Some(Gate::Sending));
        assert_eq!(gate_of(&key("cmd+c", "com.apple.finder")), None);
    }

    #[test]
    fn return_sends_in_a_chat_but_not_in_its_search() {
        let mut act = key("return", "net.whatsapp.WhatsApp");
        act.focus_role = "textarea".into();
        assert_eq!(gate_of(&act), Some(Gate::Sending));
        act.focus_role = "searchfield".into();
        assert_eq!(gate_of(&act), None);
        assert_eq!(gate_of(&key("shift+return", "net.whatsapp.WhatsApp")), None);
        assert_eq!(gate_of(&key("return", "com.apple.calculator")), None);
    }

    #[test]
    fn return_sends_on_a_chat_page() {
        let mut act = key("return", "com.google.Chrome");
        act.url = "https://web.whatsapp.com/".into();
        assert_eq!(gate_of(&act), Some(Gate::Sending));
    }

    #[test]
    fn return_reads_the_default_button() {
        let mut act = key("return", "com.apple.finder");
        act.labels = control("Delete");
        assert_eq!(gate_of(&act), Some(Gate::Destructive));
    }

    #[test]
    fn typing_in_a_password_field_stops() {
        let act = Act { verb: "type".into(), focus_role: "securefield".into(), ..Default::default() };
        assert_eq!(gate_of(&act), Some(Gate::Passwords));
        let mut paste = key("cmd+v", "com.apple.Safari");
        paste.focus_role = "securefield".into();
        assert_eq!(gate_of(&paste), Some(Gate::Passwords));
    }

    #[test]
    fn places_gate_any_input_and_name_the_place() {
        let mut act = click("Accounts");
        act.url = "https://www.commbank.com.au/retail/netbank/home".into();
        let hit = evaluate(&Policy::default(), &act).unwrap();
        assert_eq!(hit.gate, Gate::Money);
        assert_eq!(hit.place.as_deref(), Some("commbank.com.au"));

        let mut act = click("All Items");
        act.app_bundle = "com.apple.keychainaccess".into();
        act.app_name = "Keychain Access".into();
        let hit = evaluate(&Policy::default(), &act).unwrap();
        assert_eq!(hit.gate, Gate::Passwords);
        assert!(hit.place.is_some());
    }

    #[test]
    fn an_act_rule_beats_a_place_rule() {
        let mut act = click("Transfer");
        act.url = "https://www.commbank.com.au/".into();
        let hit = evaluate(&Policy::default(), &act).unwrap();
        assert_eq!(hit.place, None, "a transfer must ask every time");
    }

    #[test]
    fn scrolling_is_never_gated() {
        let act = Act {
            verb: "scroll".into(),
            url: "https://www.commbank.com.au/".into(),
            ..Default::default()
        };
        assert_eq!(gate_of(&act), None);
    }

    #[test]
    fn a_drop_on_the_trash_stops() {
        let act = Act { verb: "drag".into(), drop_labels: vec!["Trash".into()], ..Default::default() };
        assert_eq!(gate_of(&act), Some(Gate::Destructive));
        let act = Act { verb: "drag".into(), drop_labels: vec!["Desktop".into()], ..Default::default() };
        assert_eq!(gate_of(&act), None);
    }

    #[test]
    fn every_default_chord_rule_parses() {
        let p = Policy::default();
        for gate in ORDER {
            for rule in &p.rules(gate).chords {
                let keys = rule.split('@').next().unwrap();
                assert!(canonical_chord(keys).is_some(), "{rule}");
            }
        }
    }

    #[test]
    fn a_partial_policy_file_keeps_the_other_defaults() {
        let p: Policy = serde_json::from_str(r#"{"money":{"labels":["pay"]}}"#).unwrap();
        assert_eq!(p.money.labels, vec!["pay"]);
        assert!(p.money.places.is_empty());
        assert!(!p.destructive.labels.is_empty());
        assert!(!p.chat_apps.is_empty());
    }
}
