//! The language of the system items AppKit adds to the menu bar on its own (the 윈도우 menu's
//! "Close All", "Move to ..." and the like). AppKit titles them in the first language of the
//! `AppleLanguages` preference, so an English system showed them in English next to the Korean menus
//! of `native_menu.rs`. The shell is Korean only, so it asks for Korean in the argument domain, the
//! volatile preference layer that `-AppleLanguages (ko)` on the command line fills: nothing is
//! written to the user's preferences. The bundle's `Info.plist` declares the same language for
//! installed copies.

#[cfg(target_os = "macos")]
pub fn prefer_korean() {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArgumentDomain, NSArray, NSDictionary, NSString, NSUserDefaults};
    let key = NSString::from_str("AppleLanguages");
    let languages = NSArray::from_retained_slice(&[NSString::from_str("ko")]);
    let value: &AnyObject = &languages;
    let domain = NSDictionary::from_slices(&[&*key], &[value]);
    // SAFETY: the dictionary maps a string to an array of strings, the shape the preference has.
    unsafe {
        NSUserDefaults::standardUserDefaults().setVolatileDomain_forName(&domain, NSArgumentDomain);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prefer_korean() {}

#[cfg(test)]
mod tests {
    /// The request comes before anything of the shell exists, and installed copies declare Korean.
    #[test]
    fn korean_is_asked_for_before_the_builder_and_declared_by_the_bundle() {
        let main = include_str!("main.rs");
        let ask = main.find("macos_language::prefer_korean()").unwrap();
        assert!(ask < main.find("tauri::Builder::default()").unwrap());
        let plist = include_str!("../Info.plist");
        assert!(plist.contains("<key>CFBundleLocalizations</key>"));
        assert!(plist.contains("<string>ko</string>"));
    }
}
