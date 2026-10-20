//! Raw declarations for the macOS C APIs this crate calls.
//!
//! Everything is declared here by hand, so the crate has no binding
//! dependency whose API could drift. Types are opaque pointers. The safe
//! wrappers live in `cf.rs`, `ax.rs`, `input.rs`, `screen.rs`, and `apps.rs`.
#![allow(non_upper_case_globals, non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;
use std::os::raw::{c_char, c_ulong};

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFBooleanRef = *const c_void;
pub type CFURLRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFIndex = isize;
pub type CFTypeID = usize;
pub type Boolean = u8;
pub type CFOptionFlags = c_ulong;

pub const kCFStringEncodingUTF8: u32 = 0x0800_0100;
pub const kCFNumberSInt64Type: CFIndex = 4;
pub const kCFNumberFloat64Type: CFIndex = 6;

pub const kCFUserNotificationCautionAlertLevel: CFOptionFlags = 2;
pub const kCFUserNotificationDefaultResponse: CFOptionFlags = 0;
pub const kCFUserNotificationAlternateResponse: CFOptionFlags = 1;
pub const kCFUserNotificationOtherResponse: CFOptionFlags = 2;
pub const kCFUserNotificationCancelResponse: CFOptionFlags = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;
    pub static kCFBooleanTrue: CFBooleanRef;
    pub static kCFBooleanFalse: CFBooleanRef;
    pub static kCFTypeArrayCallBacks: c_void;
    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> Boolean;
    pub fn CFHash(cf: CFTypeRef) -> c_ulong;
    pub fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub fn CFStringGetTypeID() -> CFTypeID;
    pub fn CFArrayGetTypeID() -> CFTypeID;
    pub fn CFNumberGetTypeID() -> CFTypeID;
    pub fn CFBooleanGetTypeID() -> CFTypeID;
    pub fn CFURLGetTypeID() -> CFTypeID;
    pub fn CFDictionaryGetTypeID() -> CFTypeID;

    pub fn CFStringCreateWithBytes(
        alloc: CFAllocatorRef,
        bytes: *const u8,
        numBytes: CFIndex,
        encoding: u32,
        isExternalRepresentation: Boolean,
    ) -> CFStringRef;
    pub fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    pub fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
    pub fn CFStringGetCString(
        s: CFStringRef,
        buffer: *mut c_char,
        bufferSize: CFIndex,
        encoding: u32,
    ) -> Boolean;

    pub fn CFArrayGetCount(a: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(a: CFArrayRef, idx: CFIndex) -> *const c_void;
    pub fn CFArrayCreate(
        alloc: CFAllocatorRef,
        values: *const *const c_void,
        numValues: CFIndex,
        callBacks: *const c_void,
    ) -> CFArrayRef;

    pub fn CFNumberGetValue(n: CFNumberRef, theType: CFIndex, valuePtr: *mut c_void) -> Boolean;
    pub fn CFNumberIsFloatType(n: CFNumberRef) -> Boolean;
    pub fn CFBooleanGetValue(b: CFBooleanRef) -> Boolean;

    pub fn CFDictionaryGetValue(d: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub fn CFDictionaryCreate(
        alloc: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        numValues: CFIndex,
        keyCallBacks: *const c_void,
        valueCallBacks: *const c_void,
    ) -> CFDictionaryRef;

    pub fn CFURLGetString(url: CFURLRef) -> CFStringRef;

    pub fn CFUserNotificationDisplayAlert(
        timeout: f64,
        flags: CFOptionFlags,
        iconURL: CFURLRef,
        soundURL: CFURLRef,
        localizationURL: CFURLRef,
        alertHeader: CFStringRef,
        alertMessage: CFStringRef,
        defaultButtonTitle: CFStringRef,
        alternateButtonTitle: CFStringRef,
        otherButtonTitle: CFStringRef,
        responseFlags: *mut CFOptionFlags,
    ) -> i32;
}

pub type AXUIElementRef = *const c_void;
pub type AXValueRef = *const c_void;
pub type AXError = i32;

pub const kAXErrorSuccess: AXError = 0;
pub const kAXErrorInvalidUIElement: AXError = -25202;
pub const kAXErrorCannotComplete: AXError = -25204;
pub const kAXErrorAttributeUnsupported: AXError = -25205;
pub const kAXErrorActionUnsupported: AXError = -25206;
pub const kAXErrorAPIDisabled: AXError = -25211;
pub const kAXErrorNoValue: AXError = -25212;

pub const kAXValueCGPointType: u32 = 1;
pub const kAXValueCGSizeType: u32 = 2;
pub const kAXValueAXErrorType: u32 = 5;

pub type CGEventRef = *const c_void;
pub type CGEventSourceRef = *const c_void;

pub const kCGEventSourceStateHIDSystemState: i32 = 1;
pub const kCGHIDEventTap: u32 = 0;

pub const kCGEventLeftMouseDown: u32 = 1;
pub const kCGEventLeftMouseUp: u32 = 2;
pub const kCGEventRightMouseDown: u32 = 3;
pub const kCGEventRightMouseUp: u32 = 4;
pub const kCGEventMouseMoved: u32 = 5;
pub const kCGEventLeftMouseDragged: u32 = 6;
pub const kCGEventOtherMouseDown: u32 = 25;
pub const kCGEventOtherMouseUp: u32 = 26;
pub const kCGAnyInputEventType: u32 = 0xFFFF_FFFF;

pub const kCGMouseButtonLeft: u32 = 0;
pub const kCGMouseButtonRight: u32 = 1;
pub const kCGMouseButtonCenter: u32 = 2;

pub const kCGMouseEventClickState: u32 = 1;
pub const kCGEventSourceUserData: u32 = 42;

pub const kCGEventFlagMaskShift: u64 = 0x0002_0000;
pub const kCGEventFlagMaskControl: u64 = 0x0004_0000;
pub const kCGEventFlagMaskAlternate: u64 = 0x0008_0000;
pub const kCGEventFlagMaskCommand: u64 = 0x0010_0000;
pub const kCGEventFlagMaskSecondaryFn: u64 = 0x0080_0000;

pub const kCGScrollEventUnitLine: u32 = 1;

pub const kCGWindowListOptionOnScreenOnly: u32 = 1 << 0;
pub const kCGWindowListExcludeDesktopElements: u32 = 1 << 4;

#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;

    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(
        el: AXUIElementRef,
        attr: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementCopyMultipleAttributeValues(
        el: AXUIElementRef,
        attrs: CFArrayRef,
        options: u32,
        values: *mut CFArrayRef,
    ) -> AXError;
    pub fn AXUIElementCopyActionNames(el: AXUIElementRef, names: *mut CFArrayRef) -> AXError;
    pub fn AXUIElementCopyAttributeNames(el: AXUIElementRef, names: *mut CFArrayRef) -> AXError;
    pub fn AXUIElementPerformAction(el: AXUIElementRef, action: CFStringRef) -> AXError;
    pub fn AXUIElementSetAttributeValue(
        el: AXUIElementRef,
        attr: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementCopyElementAtPosition(
        app: AXUIElementRef,
        x: f32,
        y: f32,
        element: *mut AXUIElementRef,
    ) -> AXError;
    pub fn AXUIElementGetPid(el: AXUIElementRef, pid: *mut i32) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, timeout: f32) -> AXError;
    pub fn AXUIElementGetTypeID() -> CFTypeID;
    pub fn AXValueGetTypeID() -> CFTypeID;
    pub fn AXValueGetType(v: AXValueRef) -> u32;
    pub fn AXValueGetValue(v: AXValueRef, theType: u32, valuePtr: *mut c_void) -> Boolean;

    pub fn CGEventSourceCreate(stateID: i32) -> CGEventSourceRef;
    pub fn CGEventSourceSecondsSinceLastEventType(stateID: i32, eventType: u32) -> f64;
    pub fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    pub fn CGEventGetLocation(ev: CGEventRef) -> CGPoint;
    pub fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouseType: u32,
        pos: CGPoint,
        button: u32,
    ) -> CGEventRef;
    pub fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtualKey: u16,
        keyDown: bool,
    ) -> CGEventRef;
    pub fn CGEventCreateScrollWheelEvent2(
        source: CGEventSourceRef,
        units: u32,
        wheelCount: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    pub fn CGEventSetFlags(ev: CGEventRef, flags: u64);
    pub fn CGEventSetIntegerValueField(ev: CGEventRef, field: u32, value: i64);
    pub fn CGEventKeyboardSetUnicodeString(ev: CGEventRef, len: c_ulong, s: *const u16);
    pub fn CGEventPost(tap: u32, ev: CGEventRef);

    pub fn CGPreflightScreenCaptureAccess() -> bool;
    pub fn CGRequestScreenCaptureAccess() -> bool;
    pub fn CGPreflightPostEventAccess() -> bool;
    pub fn CGRequestPostEventAccess() -> bool;

    pub fn CGMainDisplayID() -> u32;
    pub fn CGDisplayBounds(display: u32) -> CGRect;
    pub fn CGDisplayPixelsWide(display: u32) -> usize;
    pub fn CGDisplayPixelsHigh(display: u32) -> usize;
    pub fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    pub fn CGDisplayCopyDisplayMode(display: u32) -> *const c_void;
    pub fn CGDisplayModeGetPixelWidth(mode: *const c_void) -> usize;
    pub fn CGDisplayModeRelease(mode: *const c_void);
    pub fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: u32) -> CFArrayRef;
}

#[link(name = "objc")]
extern "C" {
    pub fn objc_getClass(name: *const c_char) -> *mut c_void;
    pub fn sel_registerName(name: *const c_char) -> *mut c_void;
    pub fn objc_msgSend();
    pub fn objc_autoreleasePoolPush() -> *mut c_void;
    pub fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {}
