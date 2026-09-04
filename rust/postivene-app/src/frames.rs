//! Count the frames the window presents, for the developer view.
//!
//! `QQuickWindow::frameSwapped` is the only word the scene graph gives on
//! when a frame reached the screen, and it is emitted from the render
//! thread. A QML handler would run there too, which the QML engine does
//! not survive, so the connection is made here in C++ -- direct, so it
//! costs the render thread one atomic increment and nothing else -- and
//! lands in [`postivene_shim::recorder::note_frame`], which touches
//! nothing but counters.
//!
//! The second `cpp!` block in the tree, and the second `unsafe`
//! allowance; docs/PROJECT.md says why translations.rs has the first.

// `cpp!` expands to a call across the FFI boundary, and a function C++
// calls back into has to be `no_mangle`; both count as `unsafe_code`,
// which the workspace denies everywhere else.
#![allow(unsafe_code)]

use cpp::cpp;

cpp! {{
    #include <QtCore/QObject>
    #include <QtGui/QGuiApplication>
    #include <QtGui/QWindow>
    #include <QtQuick/QQuickWindow>

    extern "C" void postivene_note_frame();
}}

/// Called from the render thread for every frame presented.
#[no_mangle]
pub extern "C" fn postivene_note_frame() {
    postivene_shim::recorder::note_frame();
}

/// Connect to the first QQuickWindow the application has -- the view
/// `main` showed -- and say whether there was one. Called after the view
/// is shown; before that there is no window to find.
pub fn hook() -> bool {
    // SAFETY: reads the application's window list on the main thread and
    // makes one connection whose receiver is the window itself, so the
    // connection goes when the window does. Nothing is kept in Rust.
    cpp!(unsafe [] -> bool as "bool" {
        const auto windows = QGuiApplication::allWindows();
        for (QWindow *window : windows) {
            QQuickWindow *quick = qobject_cast<QQuickWindow *>(window);
            if (!quick) {
                continue;
            }
            QObject::connect(quick, &QQuickWindow::frameSwapped, quick,
                             []() { postivene_note_frame(); },
                             Qt::DirectConnection);
            return true;
        }
        return false;
    })
}
