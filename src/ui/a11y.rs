//! Accessible names and descriptions for widgets whose text a screen
//! reader can't take from a label (icon-and-text buttons, chips, groups of
//! choices).
//!
//! GTK has had `gtk_accessible_update_property_value()` since 4.0, but
//! gtk4-rs exposes the GtkAccessible API only with its `v4_10` feature, and
//! this app uses GTK 4.0 APIs only (it runs on GTK 4.6, Ubuntu 22.04). So
//! the one call is declared here; libgtk-4 is already linked by gtk4-sys.

use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use std::ffi::{c_int, c_void};

/// `GTK_ACCESSIBLE_PROPERTY_DESCRIPTION` (gtkenums.h).
const DESCRIPTION: c_int = 1;
/// `GTK_ACCESSIBLE_PROPERTY_LABEL` (gtkenums.h).
const LABEL: c_int = 4;

extern "C" {
    fn gtk_accessible_update_property_value(
        accessible: *mut c_void,
        n_properties: c_int,
        properties: *mut c_int,
        values: *const glib::gobject_ffi::GValue,
    );
}

fn update(widget: &impl IsA<gtk::Widget>, property: c_int, text: &str) {
    let value = text.to_value();
    let mut properties = [property];
    let ptr: *mut gtk::ffi::GtkWidget = widget.as_ref().to_glib_none().0;
    // SAFETY: every GtkWidget implements GtkAccessible (the same instance
    // pointer); one property with one string GValue, both alive for the
    // call; GTK copies the string.
    unsafe {
        gtk_accessible_update_property_value(
            ptr.cast(),
            1,
            properties.as_mut_ptr(),
            value.to_glib_none().0,
        );
    }
}

/// The name a screen reader says for `widget`.
pub fn set_label(widget: &impl IsA<gtk::Widget>, text: &str) {
    update(widget, LABEL, text);
}

/// The longer description a screen reader offers for `widget`.
pub fn set_description(widget: &impl IsA<gtk::Widget>, text: &str) {
    update(widget, DESCRIPTION, text);
}
