#![allow(non_upper_case_globals)]
use core_foundation_sys::string::CFStringRef;

extern "C" {
    pub static kAXFontTextAttribute: CFStringRef;
    pub static kAXForegroundColorTextAttribute: CFStringRef;
    pub static kAXBackgroundColorTextAttribute: CFStringRef;
    pub static kAXUnderlineColorTextAttribute: CFStringRef;
    pub static kAXStrikethroughColorTextAttribute: CFStringRef;
    pub static kAXUnderlineTextAttribute: CFStringRef;
    pub static kAXSuperscriptTextAttribute: CFStringRef;
    pub static kAXStrikethroughTextAttribute: CFStringRef;
    pub static kAXShadowTextAttribute: CFStringRef;
    pub static kAXAttachmentTextAttribute: CFStringRef;
    pub static kAXLinkTextAttribute: CFStringRef;
    pub static kAXNaturalLanguageTextAttribute: CFStringRef;
    pub static kAXReplacementStringTextAttribute: CFStringRef;
    pub static kAXMisspelledTextAttribute: CFStringRef;
    pub static kAXMarkedMisspelledTextAttribute: CFStringRef;
    pub static kAXAutocorrectedTextAttribute: CFStringRef;
    pub static kAXListItemPrefixTextAttribute: CFStringRef;
    pub static kAXListItemIndexTextAttribute: CFStringRef;
    pub static kAXListItemLevelTextAttribute: CFStringRef;
    pub static kAXFontNameKey: CFStringRef;
    pub static kAXFontFamilyKey: CFStringRef;
    pub static kAXVisibleNameKey: CFStringRef;
    pub static kAXFontSizeKey: CFStringRef;
}

pub const kAXUnderlineStyleNone: u32 = 0x0;
pub const kAXUnderlineStyleSingle: u32 = 0x1;
pub const kAXUnderlineStyleThick: u32 = 0x2;
pub const kAXUnderlineStyleDouble: u32 = 0x9;
