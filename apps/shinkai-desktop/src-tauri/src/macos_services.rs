#[cfg(target_os = "macos")]
use cocoa::appkit::{NSApp, NSApplication};
#[cfg(target_os = "macos")]
use cocoa::base::id;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use objc::declare::ClassDecl;
#[cfg(target_os = "macos")]
use objc::runtime::{Object, Sel};

#[cfg(target_os = "macos")]
use once_cell::sync::OnceCell;

#[cfg(target_os = "macos")]
use crate::windows::{recreate_window, Window};

#[cfg(target_os = "macos")]
static APP_HANDLE: OnceCell<tauri::AppHandle> = OnceCell::new();

#[cfg(target_os = "macos")]
pub fn store_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

#[cfg(target_os = "macos")]
extern "C" fn create_chat_service(_: &Object, _: Sel, _: id, _: id, _: *mut id) {
    unsafe {
        if let Some(app) = APP_HANDLE.get() {
            let _ = recreate_window(app.clone(), Window::Main, true);
            if let Some(window) = app.get_webview_window("main") {
                let _ = app.emit("create-chat", ());
                let _ = window.set_focus();
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn register_services() {
    unsafe {
        let mut decl = ClassDecl::new("ShinkaiServiceProvider", class!(NSObject)).unwrap();
        decl.add_method(sel!(createChatService:userData:error:), create_chat_service as extern "C" fn(&Object, Sel, id, id, *mut id));
        let provider: id = msg_send![decl.register(), new];
        let nsapp: id = NSApp();
        let _: () = msg_send![nsapp, setServicesProvider: provider];
        extern "C" { fn NSUpdateDynamicServices(); }
        NSUpdateDynamicServices();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn store_app_handle(_: tauri::AppHandle) {}
#[cfg(not(target_os = "macos"))]
pub fn register_services() {}
