//! Mouse and keyboard events, posted the way a real device posts them.

use crate::ffi::*;
use crate::gate::Ticket;
use std::thread::sleep;
use std::time::Duration;

/// Marks every event this crate posts, so a later event tap can tell the
/// agent's input from the user's.
pub const EVENT_TAG: i64 = 0x05C7_1A6E;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Button {
    Left,
    Right,
    Middle,
}

struct Source(CGEventSourceRef);

impl Source {
    fn new() -> Source {
        Source(unsafe { CGEventSourceCreate(kCGEventSourceStateHIDSystemState) })
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

fn post(ev: CGEventRef) {
    if ev.is_null() {
        return;
    }
    unsafe {
        CGEventSetIntegerValueField(ev, kCGEventSourceUserData, EVENT_TAG);
        CGEventPost(kCGHIDEventTap, ev);
        CFRelease(ev);
    }
}

fn ms(n: u64) {
    sleep(Duration::from_millis(n));
}

pub fn can_post() -> bool {
    unsafe { CGPreflightPostEventAccess() }
}

/// Where the pointer is now, in global screen points.
pub fn cursor() -> (f64, f64) {
    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return (-1.0, -1.0);
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        (p.x, p.y)
    }
}

/// Seconds since the person last touched the mouse or keyboard.
pub fn idle_seconds() -> f64 {
    unsafe {
        CGEventSourceSecondsSinceLastEventType(
            kCGEventSourceStateHIDSystemState,
            kCGAnyInputEventType,
        )
    }
}

/// Moves the pointer. Takes a gate ticket like every other input.
pub fn move_to(_t: &Ticket, x: f64, y: f64) {
    move_raw(x, y);
}

fn move_raw(x: f64, y: f64) {
    let src = Source::new();
    let ev = unsafe {
        CGEventCreateMouseEvent(src.0, kCGEventMouseMoved, CGPoint { x, y }, kCGMouseButtonLeft)
    };
    unsafe { CGEventSetFlags(ev, 0) };
    post(ev);
}

pub fn click(_t: &Ticket, x: f64, y: f64, button: Button, count: u32, flags: u64) {
    let (down, up, b) = match button {
        Button::Left => (kCGEventLeftMouseDown, kCGEventLeftMouseUp, kCGMouseButtonLeft),
        Button::Right => (kCGEventRightMouseDown, kCGEventRightMouseUp, kCGMouseButtonRight),
        Button::Middle => (kCGEventOtherMouseDown, kCGEventOtherMouseUp, kCGMouseButtonCenter),
    };
    let src = Source::new();
    let at = CGPoint { x, y };
    move_raw(x, y);
    ms(40);
    for n in 1..=count.clamp(1, 3) {
        for kind in [down, up] {
            let ev = unsafe { CGEventCreateMouseEvent(src.0, kind, at, b) };
            unsafe {
                CGEventSetIntegerValueField(ev, kCGMouseEventClickState, n as i64);
                CGEventSetFlags(ev, flags);
            }
            post(ev);
            ms(25);
        }
        ms(50);
    }
}

pub fn drag(_t: &Ticket, from: (f64, f64), to: (f64, f64)) {
    let src = Source::new();
    move_raw(from.0, from.1);
    ms(60);
    let at = CGPoint { x: from.0, y: from.1 };
    let down =
        unsafe { CGEventCreateMouseEvent(src.0, kCGEventLeftMouseDown, at, kCGMouseButtonLeft) };
    unsafe { CGEventSetFlags(down, 0) };
    post(down);
    ms(120);
    let steps = 16;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let p = CGPoint { x: from.0 + (to.0 - from.0) * t, y: from.1 + (to.1 - from.1) * t };
        let ev = unsafe {
            CGEventCreateMouseEvent(src.0, kCGEventLeftMouseDragged, p, kCGMouseButtonLeft)
        };
        unsafe { CGEventSetFlags(ev, 0) };
        post(ev);
        ms(14);
    }
    ms(120);
    let end = CGPoint { x: to.0, y: to.1 };
    let up =
        unsafe { CGEventCreateMouseEvent(src.0, kCGEventLeftMouseUp, end, kCGMouseButtonLeft) };
    unsafe { CGEventSetFlags(up, 0) };
    post(up);
}

/// Scrolls by lines. Positive `dy` shows what is further down the page.
pub fn scroll(_t: &Ticket, dx: i32, dy: i32) {
    let src = Source::new();
    let (mut rx, mut ry) = (dx, dy);
    while rx != 0 || ry != 0 {
        let sx = rx.clamp(-4, 4);
        let sy = ry.clamp(-4, 4);
        let ev = unsafe {
            CGEventCreateScrollWheelEvent2(src.0, kCGScrollEventUnitLine, 2, -sy, -sx, 0)
        };
        unsafe { CGEventSetFlags(ev, 0) };
        post(ev);
        rx -= sx;
        ry -= sy;
        ms(18);
    }
}

/// One key press with its modifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub flags: u64,
    pub code: u16,
    /// Canonical spelling, such as `cmd+shift+d`. Gate rules match on this.
    pub name: String,
    /// The key alone, such as `d` or `return`.
    pub key: String,
}

pub fn press(_t: &Ticket, chord: &Chord) {
    let src = Source::new();
    for down in [true, false] {
        let ev = unsafe { CGEventCreateKeyboardEvent(src.0, chord.code, down) };
        unsafe { CGEventSetFlags(ev, chord.flags) };
        post(ev);
        ms(14);
    }
}

/// The key that types a character on the US layout, and whether it needs
/// shift. None for anything that layout has no key for.
fn us_key(ch: char) -> Option<(u16, bool)> {
    const SHIFTED: &[(char, char)] = &[
        ('!', '1'), ('@', '2'), ('#', '3'), ('$', '4'), ('%', '5'), ('^', '6'), ('&', '7'),
        ('*', '8'), ('(', '9'), (')', '0'), ('_', '-'), ('+', '='), ('{', '['), ('}', ']'),
        ('|', '\\'), (':', ';'), ('"', '\''), ('<', ','), ('>', '.'), ('?', '/'), ('~', '`'),
    ];
    if ch == ' ' {
        return Some((49, false));
    }
    if ch.is_ascii_uppercase() {
        let lower = ch.to_ascii_lowercase().to_string();
        return key_code(&lower).map(|(code, _)| (code, true));
    }
    if let Some((_, base)) = SHIFTED.iter().find(|(s, _)| *s == ch) {
        return key_code(&base.to_string()).map(|(code, _)| (code, true));
    }
    if ch.is_ascii_lowercase() || ch.is_ascii_digit() || "-=[]\\;',./`".contains(ch) {
        return key_code(&ch.to_string()).map(|(code, _)| (code, false));
    }
    None
}

/// Types text. Callers split out newlines and tabs and send those as real
/// keys.
///
/// Every event carries the character itself, so text fields get the right
/// letter on any keyboard layout. A character that has a key on the US
/// layout is also sent on that key, because some apps read the key and
/// ignore the character. Calculator is one.
pub fn type_text(_t: &Ticket, text: &str) {
    let src = Source::new();
    let send = |code: u16, flags: u64, units: &[u16]| {
        for down in [true, false] {
            let ev = unsafe { CGEventCreateKeyboardEvent(src.0, code, down) };
            unsafe {
                CGEventSetFlags(ev, flags);
                CGEventKeyboardSetUnicodeString(ev, units.len() as _, units.as_ptr());
            }
            post(ev);
            ms(4);
        }
        ms(6);
    };
    let mut chunk: Vec<u16> = Vec::with_capacity(20);
    for ch in text.chars() {
        let mut buf = [0u16; 2];
        let units = ch.encode_utf16(&mut buf);
        match us_key(ch) {
            Some((code, shift)) => {
                if !chunk.is_empty() {
                    send(0, 0, &chunk);
                    chunk.clear();
                }
                send(code, if shift { kCGEventFlagMaskShift } else { 0 }, units);
            }
            None => {
                if chunk.len() + units.len() > 16 {
                    send(0, 0, &chunk);
                    chunk.clear();
                }
                chunk.extend_from_slice(units);
            }
        }
    }
    if !chunk.is_empty() {
        send(0, 0, &chunk);
    }
}

/// Types text in 16-unit Unicode chunks for real text controls (`textfield`,
/// `textarea`, `searchfield`, `combobox`) where 800-character messages would
/// otherwise take 15 seconds key-by-key.
pub fn type_text_chunked(_t: &Ticket, text: &str) {
    let src = Source::new();
    let send = |units: &[u16]| {
        for down in [true, false] {
            let ev = unsafe { CGEventCreateKeyboardEvent(src.0, 0, down) };
            unsafe {
                CGEventSetFlags(ev, 0);
                CGEventKeyboardSetUnicodeString(ev, units.len() as _, units.as_ptr());
            }
            post(ev);
            ms(3);
        }
        ms(4);
    };
    let mut chunk: Vec<u16> = Vec::with_capacity(20);
    for ch in text.chars() {
        let mut buf = [0u16; 2];
        let units = ch.encode_utf16(&mut buf);
        if chunk.len() + units.len() > 16 {
            send(&chunk);
            chunk.clear();
        }
        chunk.extend_from_slice(units);
    }
    if !chunk.is_empty() {
        send(&chunk);
    }
}

pub fn modifier_flags(names: &[String]) -> Result<u64, String> {
    let mut flags = 0;
    for n in names {
        flags |= modifier(n).ok_or_else(|| format!("unknown modifier \"{n}\""))?;
    }
    Ok(flags)
}

fn modifier(name: &str) -> Option<u64> {
    Some(match name.trim().to_lowercase().as_str() {
        "cmd" | "command" | "meta" | "super" | "⌘" => kCGEventFlagMaskCommand,
        "shift" | "⇧" => kCGEventFlagMaskShift,
        "alt" | "option" | "opt" | "⌥" => kCGEventFlagMaskAlternate,
        "ctrl" | "control" | "⌃" => kCGEventFlagMaskControl,
        "fn" => kCGEventFlagMaskSecondaryFn,
        _ => return None,
    })
}

/// Virtual key codes of the US layout, by canonical key name.
fn key_code(name: &str) -> Option<(u16, &'static str)> {
    const NAMED: &[(&[&str], u16)] = &[
        (&["return", "enter"], 36),
        (&["tab"], 48),
        (&["space", "spacebar"], 49),
        (&["delete", "backspace"], 51),
        (&["forwarddelete", "del"], 117),
        (&["escape", "esc"], 53),
        (&["left"], 123),
        (&["right"], 124),
        (&["down"], 125),
        (&["up"], 126),
        (&["home"], 115),
        (&["end"], 119),
        (&["pageup"], 116),
        (&["pagedown"], 121),
        (&["f1"], 122),
        (&["f2"], 120),
        (&["f3"], 99),
        (&["f4"], 118),
        (&["f5"], 96),
        (&["f6"], 97),
        (&["f7"], 98),
        (&["f8"], 100),
        (&["f9"], 101),
        (&["f10"], 109),
        (&["f11"], 103),
        (&["f12"], 111),
        (&["a"], 0),
        (&["s"], 1),
        (&["d"], 2),
        (&["f"], 3),
        (&["h"], 4),
        (&["g"], 5),
        (&["z"], 6),
        (&["x"], 7),
        (&["c"], 8),
        (&["v"], 9),
        (&["b"], 11),
        (&["q"], 12),
        (&["w"], 13),
        (&["e"], 14),
        (&["r"], 15),
        (&["y"], 16),
        (&["t"], 17),
        (&["1"], 18),
        (&["2"], 19),
        (&["3"], 20),
        (&["4"], 21),
        (&["6"], 22),
        (&["5"], 23),
        (&["=", "equals"], 24),
        (&["9"], 25),
        (&["7"], 26),
        (&["-", "minus"], 27),
        (&["8"], 28),
        (&["0"], 29),
        (&["]"], 30),
        (&["o"], 31),
        (&["u"], 32),
        (&["["], 33),
        (&["i"], 34),
        (&["p"], 35),
        (&["l"], 37),
        (&["j"], 38),
        (&["'", "quote"], 39),
        (&["k"], 40),
        (&[";", "semicolon"], 41),
        (&["\\", "backslash"], 42),
        (&[",", "comma"], 43),
        (&["/", "slash"], 44),
        (&["n"], 45),
        (&["m"], 46),
        (&[".", "period"], 47),
        (&["`", "grave", "backtick"], 50),
    ];
    for (names, code) in NAMED {
        if names.contains(&name) {
            return Some((*code, names[0]));
        }
    }
    None
}

/// Parses `cmd+shift+d`, `escape`, `ctrl+alt+delete`, or `cmd++`.
pub fn parse_chord(input: &str) -> Result<Chord, String> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return Err("empty key".into());
    }
    // A trailing "+" is the plus key itself, which is shift and equals.
    let (mods_part, key_part): (&str, &str) = if s == "+" {
        ("", "+")
    } else if let Some(rest) = s.strip_suffix("++") {
        (rest, "+")
    } else {
        match s.rfind('+') {
            Some(i) => (&s[..i], &s[i + 1..]),
            None => ("", s.as_str()),
        }
    };
    let mut flags = 0u64;
    for m in mods_part.split('+').map(str::trim).filter(|m| !m.is_empty()) {
        flags |= modifier(m).ok_or_else(|| format!("unknown modifier \"{m}\" in \"{input}\""))?;
    }
    let key_part = key_part.trim();
    let (code, key) = if key_part == "+" || key_part == "plus" {
        flags |= kCGEventFlagMaskShift;
        (24, "=")
    } else {
        key_code(key_part).ok_or_else(|| format!("unknown key \"{key_part}\" in \"{input}\""))?
    };
    let mut name = String::new();
    for (bit, label) in [
        (kCGEventFlagMaskControl, "ctrl"),
        (kCGEventFlagMaskAlternate, "alt"),
        (kCGEventFlagMaskShift, "shift"),
        (kCGEventFlagMaskCommand, "cmd"),
        (kCGEventFlagMaskSecondaryFn, "fn"),
    ] {
        if flags & bit != 0 {
            name.push_str(label);
            name.push('+');
        }
    }
    name.push_str(key);
    Ok(Chord { flags, code, name, key: key.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_to_one_canonical_name() {
        let a = parse_chord("Cmd+Shift+D").unwrap();
        let b = parse_chord("shift + command + d").unwrap();
        assert_eq!(a.name, "shift+cmd+d");
        assert_eq!(a, b);
        assert_eq!(a.code, 2);
        assert_eq!(a.key, "d");
    }

    #[test]
    fn aliases_fold_together() {
        assert_eq!(parse_chord("enter").unwrap().name, "return");
        assert_eq!(parse_chord("esc").unwrap().name, "escape");
        assert_eq!(parse_chord("backspace").unwrap().name, "delete");
        assert_eq!(parse_chord("option+cmd+esc").unwrap().name, "alt+cmd+escape");
    }

    #[test]
    fn plus_key_is_shift_equals() {
        let c = parse_chord("cmd++").unwrap();
        assert_eq!(c.name, "shift+cmd+=");
        assert_eq!(c.code, 24);
        assert_eq!(parse_chord("+").unwrap().name, "shift+=");
    }

    #[test]
    fn characters_map_to_their_us_keys() {
        assert_eq!(us_key('a'), Some((0, false)));
        assert_eq!(us_key('A'), Some((0, true)));
        assert_eq!(us_key('8'), Some((28, false)));
        assert_eq!(us_key('+'), Some((24, true)));
        assert_eq!(us_key('='), Some((24, false)));
        assert_eq!(us_key('?'), Some((44, true)));
        assert_eq!(us_key(' '), Some((49, false)));
        assert_eq!(us_key('é'), None);
        assert_eq!(us_key('\u{1F600}'), None);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(parse_chord("").is_err());
        assert!(parse_chord("hyper+k").is_err());
        assert!(parse_chord("cmd+nosuchkey").is_err());
    }
}
