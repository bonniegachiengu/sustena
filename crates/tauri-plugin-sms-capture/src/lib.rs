//! Reads M-Pesa and KCB transaction texts already on this phone, and the ones
//! that arrive while the app is closed, and hands them to the host to capture.
//!
//! WHAT THIS PLUGIN DOES NOT DO. It never opens a socket and it never writes to
//! the engine. It reads, it filters, it hands text back. Every write still goes
//! through the host's own capture path, which means the transducer, the same
//! admission checks, and the unlocked identity. That separation is the reason a
//! message can be captured while the app is locked: capturing costs nothing and
//! needs nobody, applying needs a key.
//!
//! THE FILTER IS TWO CHECKS AND BOTH RUN NATIVELY, before anything crosses into
//! Rust. The sender must be M-Pesa or KCB, and the body must not look like a
//! one-time code. The second check is the one that matters: a real OTP for a
//! bank action arrives from the SAME sender id as a real confirmation, so the
//! sender alone can never tell them apart. A message that fails either check is
//! dropped where it was read and is never queued, returned, or logged.
use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "online.vyybandasky.sustena.smscapture";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    PluginInvoke(String),
    /// Every desktop build. There is no inbox to read.
    #[error("SMS capture is only available on Android")]
    Unsupported,
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One text that passed both filters. `sender` is kept because the SOURCE is
/// decided by who sent the message, never by what it says: several real KCB
/// messages mention M-PESA in their own wording.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedSms {
    pub sender: String,
    pub body: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsBatch {
    pub messages: Vec<CapturedSms>,
    /// How many the native side read and dropped, so the count can be shown
    /// without the content ever existing on this side.
    #[serde(default)]
    pub filtered_out: u32,
    #[serde(default)]
    pub secrets_refused: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    /// "granted" | "denied" | "prompt" | "prompt-with-rationale"
    pub sms: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReadInboxArgs {
    /// 0 or less means the whole inbox.
    pub since_days: i32,
}

/// A command that takes nothing still needs a body to serialize.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Empty {}

pub struct SmsCapture<R: Runtime>(
    #[cfg(target_os = "android")] tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))] std::marker::PhantomData<fn() -> R>,
);

impl<R: Runtime> SmsCapture<R> {
    pub fn permission_state(&self) -> Result<PermissionStatus> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<PermissionStatus>("checkPermissions", Empty {})
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Err(Error::Unsupported)
    }

    pub fn request_permission(&self) -> Result<PermissionStatus> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<PermissionStatus>("requestPermissions", Empty {})
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Err(Error::Unsupported)
    }

    /// The backfill. Reads what is already in the inbox.
    pub fn read_inbox(&self, _args: ReadInboxArgs) -> Result<SmsBatch> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<SmsBatch>("readInbox", _args)
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Err(Error::Unsupported)
    }

    /// Whatever arrived while the app was closed or in the background. Reading
    /// clears it, so a message is handed over once.
    pub fn drain_queue(&self) -> Result<SmsBatch> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<SmsBatch>("drainQueue", Empty {})
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Err(Error::Unsupported)
    }
}

pub trait SmsCaptureExt<R: Runtime> {
    fn sms_capture(&self) -> &SmsCapture<R>;
}

impl<R: Runtime, T: Manager<R>> SmsCaptureExt<R> for T {
    fn sms_capture(&self) -> &SmsCapture<R> {
        self.state::<SmsCapture<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("sms-capture")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle = _api.register_android_plugin(PLUGIN_IDENTIFIER, "SmsCapturePlugin")?;
            #[cfg(target_os = "android")]
            app.manage(SmsCapture(handle));
            #[cfg(not(target_os = "android"))]
            app.manage(SmsCapture::<R>(std::marker::PhantomData));
            Ok(())
        })
        .build()
}
