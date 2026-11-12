//! macOS `applicationShouldTerminate:` (PD-189): the Dock's 종료, AppleScript `quit` and a system
//! shutdown or logout reach the app as `[NSApp terminate:]`. The tao delegate in use implements only
//! `applicationWillTerminate:`, so such a terminate raised neither `ExitRequested` nor a window
//! `Destroyed` and ended the process over unsaved windows. This adds the missing method to the
//! delegate's class: it hands the terminate to the quit queue (`app_quit.rs`) and answers the
//! system with `NSTerminateLater`; the queue's end calls [`reply_to_terminate`].
//!
//! Implementation review (PD-189): the newest tao (0.37.2), tauri (2.12.5) and tauri-runtime-wry
//! (2.12.3) have no such method or hook, so the method is added to the delegate class tao registers
//! (`class_addMethod`, only when the class does not define it yet), and the delegate is set again
//! so AppKit looks its methods up anew.

use std::ffi::c_char;
use std::sync::OnceLock;

use objc2::ffi::NSUInteger;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::MainThreadMarker;
use tauri::AppHandle;

use crate::app_quit::{self, Terminate};

/// `NSTerminateNow` and `NSTerminateLater` of `NSApplicationTerminateReply`.
const TERMINATE_NOW: NSUInteger = 1;
const TERMINATE_LATER: NSUInteger = 2;

/// `NSUInteger applicationShouldTerminate:(NSApplication *)`: return, receiver, selector, sender.
const SHOULD_TERMINATE_TYPES: &std::ffi::CStr = c"Q@:@";

/// The callback has no context pointer; the handle is set before the method exists.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Adds `applicationShouldTerminate:` to the delegate of `NSApp`. Call on the main thread after the
/// event loop is created and before it runs.
pub fn install(app: &AppHandle) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("macos_terminate: not on the main thread")?;
    let ns_app = NSApplication::sharedApplication(mtm);
    // SAFETY: `delegate` takes no argument and returns an object or nil.
    let delegate: Option<Retained<AnyObject>> = unsafe { msg_send![&ns_app, delegate] };
    let delegate = delegate.ok_or("macos_terminate: NSApp has no delegate")?;
    APP.set(app.clone())
        .map_err(|_| "macos_terminate: installed twice")?;
    add_should_terminate(delegate.class())?;
    // SAFETY: `setDelegate:` takes the (unretained) delegate object this call just read back.
    // AppKit asks the delegate which methods it implements when it is set, so setting the same
    // object again makes it see the method added above.
    let _: () = unsafe { msg_send![&ns_app, setDelegate: &*delegate] };
    Ok(())
}

/// `class_addMethod` leaves a method the class defines itself alone and returns false; that is an
/// error here because the class then answers terminate its own way.
fn add_should_terminate(class: &AnyClass) -> Result<(), String> {
    let imp: extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> NSUInteger = should_terminate;
    // SAFETY: `Imp` is the type-erased form of a method function; `should_terminate` has the shape
    // `SHOULD_TERMINATE_TYPES` describes (object, selector, one object argument, NSUInteger).
    let imp = unsafe {
        std::mem::transmute::<
            extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> NSUInteger,
            Imp,
        >(imp)
    };
    // SAFETY: `class` is a registered class, the selector and encoding match `imp`, and the type
    // string is NUL-terminated. The runtime does not mutate through the class pointer's referent
    // beyond its own method table.
    let added = unsafe {
        objc2::ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel!(applicationShouldTerminate:),
            imp,
            SHOULD_TERMINATE_TYPES.as_ptr().cast::<c_char>(),
        )
    };
    if added.as_bool() {
        Ok(())
    } else {
        Err(format!(
            "macos_terminate: {} already answers applicationShouldTerminate:",
            class.name().to_string_lossy()
        ))
    }
}

extern "C-unwind" fn should_terminate(
    _this: &AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
) -> NSUInteger {
    match APP.get().map(app_quit::request_terminate) {
        Some(Terminate::Later(_)) => TERMINATE_LATER,
        Some(Terminate::Now) | None => TERMINATE_NOW,
    }
}

/// The queue's answer to a terminate that returned `NSTerminateLater`: true ends the app, false
/// keeps it running (a shutdown or logout is cancelled too). Always deferred, whichever event
/// ended the queue.
pub fn reply_to_terminate(app: &AppHandle, terminate: bool) {
    app_quit::defer_to_main(app, move |_| {
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(terminate);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::runtime::{ClassBuilder, NSObject};
    use objc2::ClassType;

    #[test]
    fn the_method_is_added_once_to_a_delegate_class_with_the_terminate_encoding() {
        let mut builder = ClassBuilder::new(c"EasyOfficeTerminateTestDelegate", NSObject::class())
            .expect("the test class name is unused");
        // A delegate method tao has (its own class is built the same way).
        extern "C-unwind" fn will_terminate(_: &AnyObject, _: Sel, _: *mut AnyObject) {}
        // SAFETY: the function matches the `v@:@` encoding of the selector.
        unsafe {
            builder.add_method(
                sel!(applicationWillTerminate:),
                will_terminate as extern "C-unwind" fn(_, _, _),
            );
        }
        let class = builder.register();
        assert!(class
            .instance_method(sel!(applicationShouldTerminate:))
            .is_none());

        add_should_terminate(class).expect("the class has no such method yet");
        let method = class
            .instance_method(sel!(applicationShouldTerminate:))
            .expect("the method is registered");
        assert_eq!(method.return_type().to_string_lossy(), "Q");
        let argument = |at| {
            method
                .argument_type(at)
                .map(|t| t.to_string_lossy().into_owned())
        };
        assert_eq!(argument(2).as_deref(), Some("@"), "the sender");
        assert_eq!(argument(3), None, "no further argument");
        assert!(
            class
                .instance_method(sel!(applicationWillTerminate:))
                .is_some(),
            "the delegate's own methods stay"
        );
        assert!(
            add_should_terminate(class).is_err(),
            "a class that answers terminate itself is left alone and reported"
        );
    }

    #[test]
    fn the_replies_are_the_ns_application_terminate_reply_values() {
        // NSTerminateCancel = 0, NSTerminateNow = 1, NSTerminateLater = 2.
        assert_eq!((TERMINATE_NOW, TERMINATE_LATER), (1, 2));
    }
}
