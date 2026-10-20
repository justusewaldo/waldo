//! Small owned wrappers over CoreFoundation values.

use crate::ffi::*;
use std::ffi::c_void;
use std::ptr;

/// An owned CoreFoundation reference. Released on drop.
pub struct Cf(pub CFTypeRef);

unsafe impl Send for Cf {}

impl Cf {
    /// Takes ownership of a +1 reference. Returns None for null.
    pub fn owned(p: CFTypeRef) -> Option<Cf> {
        if p.is_null() {
            None
        } else {
            Some(Cf(p))
        }
    }

    /// Retains a borrowed reference so it can outlive its container.
    pub fn retained(p: CFTypeRef) -> Option<Cf> {
        if p.is_null() {
            None
        } else {
            unsafe { CFRetain(p) };
            Some(Cf(p))
        }
    }

    pub fn as_ptr(&self) -> CFTypeRef {
        self.0
    }

    pub fn type_id(&self) -> CFTypeID {
        unsafe { CFGetTypeID(self.0) }
    }

    pub fn string(s: &str) -> Cf {
        let p = unsafe {
            CFStringCreateWithBytes(
                kCFAllocatorDefault,
                s.as_ptr(),
                s.len() as CFIndex,
                kCFStringEncodingUTF8,
                0,
            )
        };
        Cf(p)
    }

    pub fn array_of(items: &[&Cf]) -> Cf {
        let ptrs: Vec<*const c_void> = items.iter().map(|c| c.0).collect();
        let p = unsafe {
            CFArrayCreate(
                kCFAllocatorDefault,
                ptrs.as_ptr(),
                ptrs.len() as CFIndex,
                &kCFTypeArrayCallBacks as *const c_void,
            )
        };
        Cf(p)
    }

    pub fn is_string(&self) -> bool {
        self.type_id() == unsafe { CFStringGetTypeID() }
    }

    pub fn is_array(&self) -> bool {
        self.type_id() == unsafe { CFArrayGetTypeID() }
    }

    pub fn to_string(&self) -> Option<String> {
        unsafe { string_from(self.0) }
    }

    pub fn to_bool(&self) -> Option<bool> {
        unsafe {
            if self.type_id() == CFBooleanGetTypeID() {
                Some(CFBooleanGetValue(self.0) != 0)
            } else if self.type_id() == CFNumberGetTypeID() {
                self.to_f64().map(|v| v != 0.0)
            } else {
                None
            }
        }
    }

    pub fn to_f64(&self) -> Option<f64> {
        unsafe {
            if self.type_id() != CFNumberGetTypeID() {
                return None;
            }
            let mut v: f64 = 0.0;
            CFNumberGetValue(self.0, kCFNumberFloat64Type, &mut v as *mut f64 as *mut c_void);
            Some(v)
        }
    }

    pub fn to_i64(&self) -> Option<i64> {
        unsafe {
            if self.type_id() != CFNumberGetTypeID() {
                return None;
            }
            let mut v: i64 = 0;
            CFNumberGetValue(self.0, kCFNumberSInt64Type, &mut v as *mut i64 as *mut c_void);
            Some(v)
        }
    }

    /// Text form of a scalar value: string, number, boolean, or URL.
    pub fn to_text(&self) -> Option<String> {
        unsafe {
            let t = self.type_id();
            if t == CFStringGetTypeID() {
                self.to_string()
            } else if t == CFBooleanGetTypeID() {
                Some(if CFBooleanGetValue(self.0) != 0 { "1" } else { "0" }.to_string())
            } else if t == CFNumberGetTypeID() {
                if CFNumberIsFloatType(self.0) != 0 {
                    self.to_f64().map(|v| {
                        if v.fract() == 0.0 {
                            format!("{}", v as i64)
                        } else {
                            format!("{:.3}", v)
                        }
                    })
                } else {
                    self.to_i64().map(|v| v.to_string())
                }
            } else if t == CFURLGetTypeID() {
                string_from(CFURLGetString(self.0))
            } else {
                None
            }
        }
    }

    pub fn array_len(&self) -> usize {
        if !self.is_array() {
            return 0;
        }
        unsafe { CFArrayGetCount(self.0) as usize }
    }

    /// Borrowed item pointer. Valid while the array lives.
    pub fn array_get(&self, i: usize) -> CFTypeRef {
        unsafe { CFArrayGetValueAtIndex(self.0, i as CFIndex) }
    }

    /// Looks up a string key in a dictionary. The result is retained.
    pub fn dict_get(&self, key: &str) -> Option<Cf> {
        unsafe {
            if self.type_id() != CFDictionaryGetTypeID() {
                return None;
            }
            let k = Cf::string(key);
            Cf::retained(CFDictionaryGetValue(self.0, k.0))
        }
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

impl Clone for Cf {
    fn clone(&self) -> Cf {
        unsafe { CFRetain(self.0) };
        Cf(self.0)
    }
}

/// Copies a CFString into a Rust String. Null or a non-string gives None.
///
/// # Safety
/// `s` must be null or a valid CoreFoundation object.
pub unsafe fn string_from(s: CFStringRef) -> Option<String> {
    if s.is_null() || CFGetTypeID(s) != CFStringGetTypeID() {
        return None;
    }
    let len = CFStringGetLength(s);
    let cap = CFStringGetMaximumSizeForEncoding(len, kCFStringEncodingUTF8) + 1;
    let mut buf = vec![0u8; cap as usize];
    if CFStringGetCString(s, buf.as_mut_ptr() as *mut _, cap, kCFStringEncodingUTF8) == 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    String::from_utf8(buf).ok()
}

pub fn null() -> CFTypeRef {
    ptr::null()
}
