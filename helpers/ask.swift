// waldo-ask: the dialog behind every hard stop.
//
// It answers one question and exits. The exit code is the answer:
//   0   deny. Also the answer on timeout, on close, and on any failure.
//   10  allow once
//   11  allow for some minutes
//   12  stop everything
//
// Two rules make it safe to show while the person is typing elsewhere:
//   1. The panel never takes the keyboard. No key can answer it.
//   2. Allow needs a real click. A click posted by software, or a press
//      sent through the accessibility API, is refused.

import AppKit

let agentTag: Int64 = 0x05C7_1A6E

var title = "Waldo"
var body = ""
var timeout = 120
var minutes = 0
var probe = false

var args = Array(CommandLine.arguments.dropFirst())
while !args.isEmpty {
    let flag = args.removeFirst()
    switch flag {
    case "--title": title = args.isEmpty ? title : args.removeFirst()
    case "--body": body = args.isEmpty ? body : args.removeFirst()
    case "--timeout": timeout = args.isEmpty ? timeout : (Int(args.removeFirst()) ?? timeout)
    case "--minutes": minutes = args.isEmpty ? minutes : (Int(args.removeFirst()) ?? minutes)
    case "--probe": probe = true
    default: break
    }
}

/// What kind of click is being handled right now.
enum Click: String {
    case real
    case posted
    case none
}

func currentClick() -> Click {
    guard let ev = NSApp.currentEvent, ev.type == .leftMouseUp, let cg = ev.cgEvent else {
        return .none
    }
    if cg.getIntegerValueField(.eventSourceUnixProcessID) != 0 { return .posted }
    if cg.getIntegerValueField(.eventSourceUserData) == agentTag { return .posted }
    return .real
}

final class Panel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

final class Controller: NSObject {
    var remaining = timeout
    let countdown = NSTextField(labelWithString: "")
    let note = NSTextField(labelWithString: "")

    func tick() {
        countdown.stringValue = "No answer in \(remaining) seconds counts as no."
        if remaining <= 0 { exit(0) }
        remaining -= 1
    }

    @objc func deny(_ sender: Any?) { exit(0) }

    @objc func stop(_ sender: Any?) { exit(12) }

    @objc func allow(_ sender: Any?) {
        let click = currentClick()
        if probe {
            // Probe mode reports every press and stays open until it times out.
            print(click.rawValue)
            fflush(stdout)
            note.stringValue = "Last press: \(click.rawValue)"
            return
        }
        if click == .real {
            exit(minutes > 0 ? 11 : 10)
        }
        note.stringValue = "Allow needs a click from your own mouse or trackpad."
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let controller = Controller()

func label(_ text: String, size: CGFloat, bold: Bool = false, dim: Bool = false) -> NSTextField {
    let field = NSTextField(wrappingLabelWithString: text)
    field.font = bold ? .boldSystemFont(ofSize: size) : .systemFont(ofSize: size)
    field.textColor = dim ? .secondaryLabelColor : .labelColor
    field.isSelectable = false
    field.preferredMaxLayoutWidth = 420
    return field
}

func button(_ text: String, _ action: Selector) -> NSButton {
    let b = NSButton(title: text, target: controller, action: action)
    b.bezelStyle = .rounded
    b.refusesFirstResponder = true
    return b
}

let allowTitle = probe ? "Probe" : (minutes > 0 ? "Allow for \(minutes) minutes" : "Allow once")
let buttons = NSStackView(views: [
    button("Stop everything", #selector(Controller.stop(_:))),
    NSView(),
    button(allowTitle, #selector(Controller.allow(_:))),
    button("Deny", #selector(Controller.deny(_:))),
])
buttons.orientation = .horizontal
buttons.spacing = 10

for field in [controller.countdown, controller.note] {
    field.font = .systemFont(ofSize: 11)
    field.textColor = .secondaryLabelColor
}
controller.note.textColor = .systemOrange

let stack = NSStackView(views: [
    label(title, size: 14, bold: true),
    label(body, size: 12),
    controller.countdown,
    controller.note,
    buttons,
])
stack.orientation = .vertical
stack.alignment = .leading
stack.spacing = 10
stack.edgeInsets = NSEdgeInsets(top: 18, left: 20, bottom: 16, right: 20)
stack.translatesAutoresizingMaskIntoConstraints = false

let panel = Panel(
    contentRect: NSRect(x: 0, y: 0, width: 460, height: 200),
    styleMask: [.titled, .nonactivatingPanel],
    backing: .buffered,
    defer: false
)
panel.title = "Waldo"
panel.level = .statusBar
panel.isFloatingPanel = true
panel.hidesOnDeactivate = false
panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
let content = NSView()
content.addSubview(stack)
panel.contentView = content
NSLayoutConstraint.activate([
    stack.leadingAnchor.constraint(equalTo: content.leadingAnchor),
    stack.trailingAnchor.constraint(equalTo: content.trailingAnchor),
    stack.topAnchor.constraint(equalTo: content.topAnchor),
    stack.bottomAnchor.constraint(equalTo: content.bottomAnchor),
    stack.widthAnchor.constraint(equalToConstant: 460),
    buttons.widthAnchor.constraint(equalToConstant: 420),
])
panel.layoutIfNeeded()

// Show it on the display the pointer is on, a third of the way down.
let mouse = NSEvent.mouseLocation
let screen = NSScreen.screens.first { $0.frame.contains(mouse) } ?? NSScreen.main ?? NSScreen.screens[0]
let size = panel.frame.size
panel.setFrameOrigin(NSPoint(
    x: screen.visibleFrame.midX - size.width / 2,
    y: screen.visibleFrame.maxY - screen.visibleFrame.height / 3 - size.height / 2
))
panel.orderFrontRegardless()
NSSound(named: "Funk")?.play()

controller.tick()
let timer = Timer(timeInterval: 1, repeats: true) { _ in controller.tick() }
RunLoop.main.add(timer, forMode: .common)
app.run()
