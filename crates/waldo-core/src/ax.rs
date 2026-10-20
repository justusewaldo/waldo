//! The accessibility tree: the same tree a screen reader walks.
//!
//! Every element already carries its role, its label, and its real frame,
//! so the model picks a thing and never has to invent a coordinate.
// The constants keep the names Apple gives them.
#![allow(non_upper_case_globals)]

use crate::gate::Ticket;
use crate::cf::{null, Cf};
use crate::ffi::*;
use crate::geom::Rect;
use std::ffi::c_void;
use std::time::{Duration, Instant};

/// An owned reference to one UI element in some app.
#[derive(Clone)]
pub struct Element(Cf);

pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Asks macOS to show the Accessibility grant prompt for this process.
pub fn request_trust() -> bool {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let vals = [kCFBooleanTrue];
        let d = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr(),
            vals.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const c_void,
        );
        let ok = AXIsProcessTrustedWithOptions(d) != 0;
        CFRelease(d);
        ok
    }
}

pub fn error_text(e: AXError) -> String {
    match e {
        kAXErrorAPIDisabled => {
            "Accessibility is not granted to the app that runs this tool. Run `waldo grant`.".into()
        }
        kAXErrorInvalidUIElement => "that element no longer exists. Call see again.".into(),
        kAXErrorCannotComplete => "the app did not answer in time. It may be busy.".into(),
        kAXErrorActionUnsupported => "the element does not support that action.".into(),
        kAXErrorAttributeUnsupported => "the element does not have that attribute.".into(),
        kAXErrorNoValue => "the attribute has no value.".into(),
        other => format!("accessibility error {other}"),
    }
}

impl Element {
    pub fn app(pid: i32) -> Element {
        Element(Cf(unsafe { AXUIElementCreateApplication(pid) }))
    }

    pub fn system() -> Element {
        Element(Cf(unsafe { AXUIElementCreateSystemWide() }))
    }

    fn from_borrowed(p: CFTypeRef) -> Option<Element> {
        unsafe {
            if p.is_null() || CFGetTypeID(p) != AXUIElementGetTypeID() {
                return None;
            }
        }
        Cf::retained(p).map(Element)
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_ptr()
    }

    pub fn set_timeout(&self, secs: f32) {
        unsafe { AXUIElementSetMessagingTimeout(self.raw(), secs) };
    }

    pub fn copy_attr(&self, name: &str) -> Result<Cf, AXError> {
        let k = Cf::string(name);
        let mut out: CFTypeRef = null();
        let e = unsafe { AXUIElementCopyAttributeValue(self.raw(), k.as_ptr(), &mut out) };
        if e == kAXErrorSuccess {
            Cf::owned(out).ok_or(kAXErrorNoValue)
        } else {
            Err(e)
        }
    }

    pub fn text(&self, name: &str) -> Option<String> {
        self.copy_attr(name).ok().and_then(|v| v.to_text())
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        self.copy_attr(name).ok().and_then(|v| v.to_bool())
    }

    pub fn element(&self, name: &str) -> Option<Element> {
        let v = self.copy_attr(name).ok()?;
        Element::from_borrowed(v.as_ptr())
    }

    pub fn elements(&self, name: &str) -> Vec<Element> {
        match self.copy_attr(name) {
            Ok(v) => elements_of(&v),
            Err(_) => Vec::new(),
        }
    }

    pub fn frame(&self) -> Option<Rect> {
        let p = self.copy_attr("AXPosition").ok().and_then(|v| point_of(v.as_ptr()))?;
        let s = self.copy_attr("AXSize").ok().and_then(|v| size_of(v.as_ptr()))?;
        Some(Rect { x: p.0, y: p.1, w: s.0, h: s.1 })
    }

    /// Every attribute this element offers. For `waldo inspect`.
    pub fn attr_names(&self) -> Vec<String> {
        let mut out: CFArrayRef = null();
        let e = unsafe { AXUIElementCopyAttributeNames(self.raw(), &mut out) };
        if e != kAXErrorSuccess {
            return Vec::new();
        }
        let Some(arr) = Cf::owned(out) else { return Vec::new() };
        (0..arr.array_len())
            .filter_map(|i| unsafe { crate::cf::string_from(arr.array_get(i)) })
            .collect()
    }

    /// An attribute that holds a list of strings, such as the class list
    /// of a web element.
    pub fn strings(&self, name: &str) -> Vec<String> {
        let Ok(v) = self.copy_attr(name) else { return Vec::new() };
        (0..v.array_len())
            .filter_map(|i| unsafe { crate::cf::string_from(v.array_get(i)) })
            .collect()
    }

    /// Stable for one element. Two references to one element hash alike.
    pub fn hash(&self) -> usize {
        unsafe { CFHash(self.raw()) as usize }
    }

    pub fn actions(&self) -> Vec<String> {
        let mut out: CFArrayRef = null();
        let e = unsafe { AXUIElementCopyActionNames(self.raw(), &mut out) };
        if e != kAXErrorSuccess {
            return Vec::new();
        }
        let Some(arr) = Cf::owned(out) else { return Vec::new() };
        (0..arr.array_len())
            .filter_map(|i| unsafe { crate::cf::string_from(arr.array_get(i)) })
            .collect()
    }

    pub fn perform(&self, _t: &Ticket, action: &str) -> Result<(), String> {
        let a = Cf::string(action);
        let e = unsafe { AXUIElementPerformAction(self.raw(), a.as_ptr()) };
        if e == kAXErrorSuccess {
            Ok(())
        } else {
            Err(error_text(e))
        }
    }

    /// Sets a true or false attribute, such as AXFocused or AXHidden.
    pub fn set_flag(&self, _t: &Ticket, name: &str, value: bool) -> Result<(), String> {
        self.set_flag_raw(name, value)
    }

    fn set_flag_raw(&self, name: &str, value: bool) -> Result<(), String> {
        let k = Cf::string(name);
        let v = unsafe {
            if value {
                kCFBooleanTrue
            } else {
                kCFBooleanFalse
            }
        };
        let e = unsafe { AXUIElementSetAttributeValue(self.raw(), k.as_ptr(), v) };
        if e == kAXErrorSuccess {
            Ok(())
        } else {
            Err(error_text(e))
        }
    }

    pub fn set_text(&self, _t: &Ticket, name: &str, value: &str) -> Result<(), String> {
        let k = Cf::string(name);
        let v = Cf::string(value);
        let e = unsafe { AXUIElementSetAttributeValue(self.raw(), k.as_ptr(), v.as_ptr()) };
        if e == kAXErrorSuccess {
            Ok(())
        } else {
            Err(error_text(e))
        }
    }

    pub fn pid(&self) -> Option<i32> {
        let mut pid: i32 = 0;
        let e = unsafe { AXUIElementGetPid(self.raw(), &mut pid) };
        (e == kAXErrorSuccess).then_some(pid)
    }

    pub fn same(&self, other: &Element) -> bool {
        unsafe { CFEqual(self.raw(), other.raw()) != 0 }
    }

    /// The deepest element under a global screen point.
    pub fn at_point(x: f64, y: f64) -> Option<Element> {
        let sys = Element::system();
        let mut out: AXUIElementRef = null();
        let e = unsafe {
            AXUIElementCopyElementAtPosition(sys.raw(), x as f32, y as f32, &mut out)
        };
        if e != kAXErrorSuccess {
            return None;
        }
        Cf::owned(out).map(Element)
    }

    /// Role, subrole, and best label of this one element.
    pub fn describe(&self) -> Described {
        let role = self.text("AXRole").unwrap_or_default();
        let subrole = self.text("AXSubrole").unwrap_or_default();
        let mut label = String::new();
        for name in ["AXTitle", "AXDescription", "AXPlaceholderValue", "AXHelp"] {
            if let Some(t) = self.text(name) {
                if !t.trim().is_empty() {
                    label = clean(&t, 160);
                    break;
                }
            }
        }
        if label.is_empty() && role == "AXStaticText" {
            label = self.text("AXValue").map(|t| clean(&t, 160)).unwrap_or_default();
        }
        if label.is_empty() {
            label = subrole_label(&subrole).to_string();
        }
        Described { short: short_role(&role, &subrole).to_string(), role, subrole, label }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Described {
    pub role: String,
    pub subrole: String,
    pub short: String,
    pub label: String,
}

fn elements_of(arr: &Cf) -> Vec<Element> {
    (0..arr.array_len()).filter_map(|i| Element::from_borrowed(arr.array_get(i))).collect()
}

fn is_ax_value(p: CFTypeRef) -> bool {
    unsafe { !p.is_null() && CFGetTypeID(p) == AXValueGetTypeID() }
}

fn point_of(p: CFTypeRef) -> Option<(f64, f64)> {
    if !is_ax_value(p) {
        return None;
    }
    let mut pt = CGPoint::default();
    let ok = unsafe {
        AXValueGetValue(p, kAXValueCGPointType, &mut pt as *mut CGPoint as *mut c_void)
    };
    (ok != 0).then_some((pt.x, pt.y))
}

fn size_of(p: CFTypeRef) -> Option<(f64, f64)> {
    if !is_ax_value(p) {
        return None;
    }
    let mut sz = CGSize::default();
    let ok =
        unsafe { AXValueGetValue(p, kAXValueCGSizeType, &mut sz as *mut CGSize as *mut c_void) };
    (ok != 0).then_some((sz.width, sz.height))
}

/// Collapses whitespace and cuts to `max` characters.
pub fn clean(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut last_space = true;
    let mut count = 0;
    for ch in s.chars() {
        // Direction marks are invisible and would break label matching.
        if matches!(ch, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}') {
            continue;
        }
        // Icon fonts draw their glyphs from the private-use ranges. A
        // screen reader would say nothing for them, and neither do we.
        if matches!(ch, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}' | '\u{FFFC}') {
            continue;
        }
        let ch = if ch.is_whitespace() || ch.is_control() { ' ' } else { ch };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        if count >= max {
            out.push('…');
            break;
        }
        out.push(ch);
        count += 1;
    }
    out.trim().to_string()
}

fn subrole_label(subrole: &str) -> &'static str {
    match subrole {
        "AXCloseButton" => "Close window",
        "AXMinimizeButton" => "Minimize window",
        "AXZoomButton" => "Zoom window",
        "AXFullScreenButton" => "Full screen",
        _ => "",
    }
}

/// The short role name the model sees.
pub fn short_role(role: &str, subrole: &str) -> &'static str {
    match subrole {
        "AXSecureTextField" => return "securefield",
        "AXSearchField" => return "searchfield",
        "AXSwitch" => return "switch",
        "AXToggle" => return "toggle",
        "AXTabButton" => return "tab",
        _ => {}
    }
    match role {
        "AXButton" => "button",
        "AXLink" => "link",
        "AXCheckBox" => "checkbox",
        "AXRadioButton" => "radio",
        "AXPopUpButton" => "popup",
        "AXMenuButton" => "menubutton",
        "AXMenuItem" => "menuitem",
        "AXMenuBarItem" => "menu",
        "AXTextField" => "textfield",
        "AXTextArea" => "textarea",
        "AXComboBox" => "combobox",
        "AXSlider" => "slider",
        "AXIncrementor" => "stepper",
        "AXDisclosureTriangle" => "disclosure",
        "AXColorWell" => "colorwell",
        "AXDockItem" => "dockitem",
        "AXRow" => "row",
        "AXCell" => "cell",
        "AXStaticText" => "text",
        "AXHeading" => "heading",
        "AXImage" => "image",
        "AXWebArea" => "webarea",
        "AXProgressIndicator" | "AXBusyIndicator" => "progress",
        _ => "other",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Shown, can be acted on, children are only read for a label.
    Leaf,
    /// Shown, can be acted on, children are shown too.
    Container,
    /// Shown for reading only.
    Info,
    /// Not shown. Children are walked.
    Pass,
}

fn kind_of(role: &str, parent_role: &str) -> Kind {
    match role {
        "AXButton" | "AXLink" | "AXCheckBox" | "AXRadioButton" | "AXTextField" | "AXTextArea"
        | "AXSlider" | "AXIncrementor" | "AXDisclosureTriangle" | "AXColorWell" | "AXDockItem"
        | "AXMenuBarItem" => Kind::Leaf,
        "AXPopUpButton" | "AXMenuButton" | "AXComboBox" | "AXMenuItem" | "AXRow" => Kind::Container,
        "AXCell" if parent_role != "AXRow" => Kind::Container,
        "AXStaticText" | "AXHeading" | "AXImage" | "AXWebArea" | "AXProgressIndicator"
        | "AXBusyIndicator" => Kind::Info,
        _ => Kind::Pass,
    }
}

/// One line of the tree the model sees.
#[derive(Clone)]
pub struct Node {
    pub el: Element,
    pub role: String,
    pub subrole: String,
    pub short: &'static str,
    pub label: String,
    pub value: Option<String>,
    pub frame: Rect,
    pub enabled: bool,
    pub focused: bool,
    pub selected: bool,
    pub actionable: bool,
}

pub struct WalkOpts {
    pub max_visited: usize,
    pub max_depth: usize,
    pub budget: Duration,
    /// Elements wholly outside this rect are skipped with their children.
    pub clip: Option<Rect>,
}

impl Default for WalkOpts {
    fn default() -> Self {
        WalkOpts {
            max_visited: 6000,
            max_depth: 60,
            budget: Duration::from_millis(2500),
            clip: None,
        }
    }
}

pub struct Walk {
    pub nodes: Vec<Node>,
    pub visited: usize,
    pub truncated: bool,
    pub url: Option<String>,
    pub elapsed: Duration,
}

const BATCH: [&str; 13] = [
    "AXRole",
    "AXSubrole",
    "AXTitle",
    "AXDescription",
    "AXValue",
    "AXPosition",
    "AXSize",
    "AXEnabled",
    "AXFocused",
    "AXSelected",
    "AXChildren",
    "AXPlaceholderValue",
    "AXURL",
];

struct Attrs {
    role: String,
    subrole: String,
    title: String,
    desc: String,
    value: Option<String>,
    frame: Option<Rect>,
    enabled: bool,
    focused: bool,
    selected: bool,
    children: Vec<Element>,
    placeholder: String,
    url: Option<String>,
}

struct Ctx<'a> {
    opts: &'a WalkOpts,
    /// Elements already visited, by hash. A web table offers each cell
    /// twice, once under its row and once under its column.
    seen: std::collections::HashMap<usize, Vec<Element>>,
    names: Cf,
    start: Instant,
    visited: usize,
    truncated: bool,
    nodes: Vec<Node>,
    url: Option<String>,
}

fn read(el: &Element, names: &Cf) -> Option<Attrs> {
    let mut out: CFArrayRef = null();
    let e = unsafe {
        AXUIElementCopyMultipleAttributeValues(el.raw(), names.as_ptr(), 0, &mut out)
    };
    if e != kAXErrorSuccess {
        return None;
    }
    let arr = Cf::owned(out)?;
    if arr.array_len() < BATCH.len() {
        return None;
    }
    // A missing attribute comes back as an AXValue that holds an error code.
    let item = |i: usize| -> Option<Cf> {
        let p = arr.array_get(i);
        if p.is_null() {
            return None;
        }
        if is_ax_value(p) && unsafe { AXValueGetType(p) } == kAXValueAXErrorType {
            return None;
        }
        Cf::retained(p)
    };
    let text = |i: usize| item(i).and_then(|v| v.to_text()).unwrap_or_default();
    let flag = |i: usize, default: bool| item(i).and_then(|v| v.to_bool()).unwrap_or(default);
    let pos = item(5).and_then(|v| point_of(v.as_ptr()));
    let size = item(6).and_then(|v| size_of(v.as_ptr()));
    let frame = match (pos, size) {
        (Some(p), Some(s)) => Some(Rect { x: p.0, y: p.1, w: s.0, h: s.1 }),
        _ => None,
    };
    Some(Attrs {
        role: text(0),
        subrole: text(1),
        title: text(2),
        desc: text(3),
        value: item(4).and_then(|v| v.to_text()),
        frame,
        enabled: flag(7, true),
        focused: flag(8, false),
        selected: flag(9, false),
        children: item(10).map(|v| elements_of(&v)).unwrap_or_default(),
        placeholder: text(11),
        url: item(12).and_then(|v| v.to_text()),
    })
}

fn first_nonempty(items: &[&str]) -> String {
    for s in items {
        if !s.trim().is_empty() {
            return clean(s, 160);
        }
    }
    String::new()
}

/// Walks the subtree. Returns the first readable text found in it, which a
/// parent with no label of its own takes as its label.
fn visit(el: &Element, parent_role: &str, depth: usize, quiet: bool, ctx: &mut Ctx) -> String {
    if ctx.visited >= ctx.opts.max_visited || ctx.start.elapsed() > ctx.opts.budget {
        ctx.truncated = true;
        return String::new();
    }
    let twins = ctx.seen.entry(el.hash()).or_default();
    if twins.iter().any(|t| t.same(el)) {
        return String::new();
    }
    twins.push(el.clone());
    ctx.visited += 1;
    let Some(a) = read(el, &ctx.names) else { return String::new() };

    if let (Some(f), Some(clip)) = (a.frame, ctx.opts.clip) {
        if f.w > 0.0 && f.h > 0.0 && !f.intersects(&clip) {
            return String::new();
        }
    }
    if a.role == "AXWebArea" && ctx.url.is_none() {
        ctx.url = a.url.clone().filter(|u| !u.is_empty());
    }

    let kind = kind_of(&a.role, parent_role);
    let is_text = a.role == "AXStaticText";
    let mut own = if is_text {
        first_nonempty(&[a.value.as_deref().unwrap_or(""), &a.title, &a.desc])
    } else {
        first_nonempty(&[&a.title, &a.desc, &a.placeholder, subrole_label(&a.subrole)])
    };

    let visible = a.frame.map(|f| f.w > 0.0 && f.h > 0.0).unwrap_or(false);
    let show = !quiet && visible && kind != Kind::Pass;
    let index = if show {
        let value = if is_text {
            None
        } else {
            a.value.as_deref().map(|v| clean(v, 200)).filter(|v| !v.is_empty())
        };
        ctx.nodes.push(Node {
            el: el.clone(),
            short: short_role(&a.role, &a.subrole),
            role: a.role.clone(),
            subrole: a.subrole.clone(),
            label: own.clone(),
            value,
            frame: a.frame.unwrap_or_default(),
            enabled: a.enabled,
            focused: a.focused,
            selected: a.selected,
            actionable: matches!(kind, Kind::Leaf | Kind::Container),
        });
        Some(ctx.nodes.len() - 1)
    } else {
        None
    };

    // A leaf with its own label needs nothing from its children. A leaf with
    // no label reads them quietly, only to borrow their text.
    let descend = match kind {
        Kind::Leaf => own.is_empty(),
        Kind::Info => a.role == "AXHeading" || a.role == "AXWebArea",
        _ => true,
    };
    let child_quiet = quiet || kind == Kind::Leaf || a.role == "AXHeading";
    let mut derived = String::new();
    if descend && depth < ctx.opts.max_depth {
        let children = visible_children(el, &a);
        for child in &children {
            let t = visit(child, &a.role, depth + 1, child_quiet, ctx);
            if derived.is_empty() && !t.is_empty() {
                derived = t;
            }
        }
    }

    if own.is_empty() && !derived.is_empty() && kind != Kind::Pass {
        own = derived.clone();
        if let Some(i) = index {
            ctx.nodes[i].label = own.clone();
        }
    }
    if own.is_empty() {
        derived
    } else {
        own
    }
}

/// Big tables hold every row, on screen or not. Ask for the visible ones.
fn visible_children(el: &Element, a: &Attrs) -> Vec<Element> {
    if (a.role == "AXTable" || a.role == "AXOutline") && a.children.len() > 40 {
        let rows = el.elements("AXVisibleRows");
        if !rows.is_empty() {
            return rows;
        }
    }
    if (a.role == "AXList" || a.role == "AXBrowser") && a.children.len() > 40 {
        let kids = el.elements("AXVisibleChildren");
        if !kids.is_empty() {
            return kids;
        }
    }
    a.children.clone()
}

pub fn walk(root: &Element, opts: &WalkOpts) -> Walk {
    let name_strings: Vec<Cf> = BATCH.iter().map(|n| Cf::string(n)).collect();
    let refs: Vec<&Cf> = name_strings.iter().collect();
    let mut ctx = Ctx {
        opts,
        seen: std::collections::HashMap::new(),
        names: Cf::array_of(&refs),
        start: Instant::now(),
        visited: 0,
        truncated: false,
        nodes: Vec::new(),
        url: None,
    };
    visit(root, "", 0, false, &mut ctx);
    // Drop shown nodes that carry nothing the model can use.
    ctx.nodes.retain(|n| {
        let empty = n.label.is_empty() && n.value.is_none();
        !(empty && (!n.actionable || n.role == "AXRow" || n.role == "AXCell"))
    });
    dedupe_text(&mut ctx.nodes);
    Walk {
        nodes: ctx.nodes,
        visited: ctx.visited,
        truncated: ctx.truncated,
        url: ctx.url,
        elapsed: ctx.start.elapsed(),
    }
}

/// A text line that only repeats the label of the control around it is noise.
fn dedupe_text(nodes: &mut Vec<Node>) {
    let mut keep = vec![true; nodes.len()];
    for i in 0..nodes.len() {
        if nodes[i].short != "text" {
            continue;
        }
        let lo = i.saturating_sub(12);
        for j in lo..i {
            let p = &nodes[j];
            if p.actionable && p.label == nodes[i].label && p.frame.contains_rect(&nodes[i].frame) {
                keep[i] = false;
                break;
            }
        }
    }
    let mut it = keep.iter();
    nodes.retain(|_| *it.next().unwrap());
}

/// Finds the address of the first web page under `root`, breadth first.
/// Browsers put the page a few levels down, so a small budget is enough.
pub fn find_url(root: &Element, max_visited: usize) -> Option<String> {
    let mut queue = std::collections::VecDeque::from([root.clone()]);
    let mut visited = 0;
    while let Some(el) = queue.pop_front() {
        visited += 1;
        if visited > max_visited {
            return None;
        }
        if el.text("AXRole").as_deref() == Some("AXWebArea") {
            if let Some(u) = el.text("AXURL").filter(|u| !u.is_empty()) {
                return Some(u);
            }
        }
        queue.extend(el.elements("AXChildren"));
    }
    None
}

/// Labels from an element up to the first real control around it.
/// The flag is true when such a control was found.
pub fn label_chain(el: &Element) -> (Vec<String>, bool) {
    let mut labels = Vec::new();
    let mut cur = Some(el.clone());
    for _ in 0..5 {
        let Some(e) = cur else { break };
        let d = e.describe();
        if !d.label.is_empty() && !labels.contains(&d.label) {
            labels.push(d.label.clone());
        }
        if is_control(d.short.as_str()) {
            return (labels, true);
        }
        cur = e.element("AXParent");
    }
    (labels, false)
}

/// The control a click at a point means, when it can be pressed through
/// accessibility: the element under the point, or the first control around
/// it (the point often lands on an icon or a text inside a button).
pub fn pressable(hit: &Element) -> Option<Element> {
    let mut cur = Some(hit.clone());
    for _ in 0..5 {
        let e = cur?;
        let d = e.describe();
        if is_control(d.short.as_str()) {
            return (press_by_point(d.short.as_str()) && e.actions().iter().any(|a| a == "AXPress")).then_some(e);
        }
        cur = e.element("AXParent");
    }
    None
}

/// Controls a press can stand in for a click on. Where you click on a
/// stepper, a colour well, a row, or a cell matters, so those keep the pointer.
pub fn press_by_point(short: &str) -> bool {
    is_control(short) && !matches!(short, "stepper" | "colorwell" | "row" | "cell")
}

/// Roles where a click does something, as opposed to placing a caret.
pub fn is_control(short: &str) -> bool {
    matches!(
        short,
        "button" | "link" | "checkbox" | "radio" | "popup" | "menubutton" | "menuitem" | "menu"
            | "tab" | "switch" | "toggle" | "disclosure" | "dockitem" | "row" | "cell"
            | "stepper" | "combobox" | "colorwell"
    )
}

/// Chromium and Electron apps build their tree only when asked.
/// The flags that ask an app to build its accessibility tree.
pub enum WakeFlag {
    /// Electron honours this one.
    Manual,
    /// Chromium browsers need this one.
    Enhanced,
}

/// Sets one wake flag. It changes nothing a person can see.
pub fn wake_with(app: &Element, flag: WakeFlag) {
    let name = match flag {
        WakeFlag::Manual => "AXManualAccessibility",
        WakeFlag::Enhanced => "AXEnhancedUserInterface",
    };
    let _ = app.set_flag_raw(name, true);
}

/// Asks Chromium and Electron apps to build their accessibility tree. This
/// changes nothing a person can see, so it needs no ticket.
pub fn wake(app: &Element) {
    let _ = app.set_flag_raw("AXManualAccessibility", true);
    let _ = app.set_flag_raw("AXEnhancedUserInterface", true);
}

/// The window to read for an app: focused, else main, else the first one.
pub fn focus_window(app: &Element) -> Option<Element> {
    app.element("AXFocusedWindow")
        .or_else(|| app.element("AXMainWindow"))
        .or_else(|| app.elements("AXWindows").into_iter().next())
}

/// The app that owns keyboard focus, as an element and a pid.
pub fn focused_app() -> Option<(Element, i32)> {
    let app = Element::system().element("AXFocusedApplication")?;
    let pid = app.pid()?;
    Some((app, pid))
}

/// The element that would receive typed keys right now.
pub fn focused_element() -> Option<Element> {
    Element::system().element("AXFocusedUIElement")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_collapses_and_cuts() {
        assert_eq!(clean("  a \n\t b  ", 10), "a b");
        assert_eq!(clean("abcdef", 3), "abc…");
        assert_eq!(clean("", 3), "");
        assert_eq!(clean("\u{200E}1,080\u{200E}", 20), "1,080");
        assert_eq!(clean("\u{F095} Call back", 40), "Call back");
        assert_eq!(clean("\u{F02B}", 40), "");
    }

    #[test]
    fn a_press_stands_in_for_a_click_only_where_the_spot_does_not_matter() {
        for short in ["button", "link", "checkbox", "radio", "popup", "menuitem", "tab", "switch", "disclosure"] {
            assert!(press_by_point(short), "{short} should be pressed");
        }
        for short in ["stepper", "colorwell", "row", "cell", "textfield", "text", "image", "webarea", "other"] {
            assert!(!press_by_point(short), "{short} should keep the pointer");
        }
    }

    #[test]
    fn short_roles_prefer_subrole() {
        assert_eq!(short_role("AXTextField", "AXSecureTextField"), "securefield");
        assert_eq!(short_role("AXRadioButton", "AXTabButton"), "tab");
        assert_eq!(short_role("AXButton", ""), "button");
        assert_eq!(short_role("AXGroup", ""), "other");
    }

    #[test]
    fn cells_inside_rows_are_passed_over() {
        assert!(kind_of("AXCell", "AXRow") == Kind::Pass);
        assert!(kind_of("AXCell", "AXTable") == Kind::Container);
        assert!(kind_of("AXButton", "") == Kind::Leaf);
    }
}
