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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsBatch {
    pub messages: Vec<CapturedSms>,
    /// How many the native side read and dropped, so the count can be shown
    /// without the content ever existing on this side.
    #[serde(default)]
    pub filtered_out: u32,
    #[serde(default)]
    pub secrets_refused: u32,
    /// More pages behind this one.
    #[serde(default)]
    pub has_more: bool,
    /// Where the next page starts. Reading only.
    #[serde(default)]
    pub next_offset: u32,
    /// Still queued after this take. Draining only.
    #[serde(default)]
    pub remaining: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    /// "granted" | "denied" | "prompt" | "prompt-with-rationale"
    pub sms: String,
    /// The same, for posting the classify prompt.
    ///
    /// ★★★ Reported SEPARATELY rather than folded into `sms`, because the two
    /// refusals mean different things and have different remedies. Declining
    /// to be notified still leaves a working importer; declining to have texts
    /// read leaves nothing. A single field would make the smaller refusal look
    /// like the larger one, and a surface cannot say *"reading works, telling
    /// you does not"* if it was never told which failed.
    ///
    /// Defaulted, so an older plugin build that does not report it deserialises
    /// rather than failing the whole permission check.
    #[serde(default)]
    pub notify: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReadInboxArgs {
    /// 0 or less means the whole inbox.
    pub since_days: i32,
    /// Matching messages to skip. Paging exists because a phone holding a few
    /// thousand texts froze the app when every match came back at once.
    pub offset: u32,
    /// How many to take in this page.
    pub limit: u32,
    /// Only messages newer than this. The caller's high-water mark, so a repeat
    /// read walks what arrived since rather than the whole inbox again.
    pub since_ms: i64,
    /// Where the household's record begins, INCLUSIVE, in unix milliseconds.
    ///
    /// ★★★ The intake boundary, and it is applied at the content query on the
    /// device: a message older than this is never read off the phone at all.
    /// Not read, not returned, not captured, not queued -- the backlog does not
    /// enter and then get filtered, it never enters.
    ///
    /// 0 means no start, which is open.
    pub start_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DrainArgs {
    /// How many to take. The rest stay queued for the next call.
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueueDepth {
    pub depth: u32,
}

/// What a notification tap was about, if there was one.
///
/// ★★★ The RAW TEXT rather than an id, because the engine keys an intake on
/// the fact rather than on an identifier the Android side could mint (ING-5).
/// The text is the only handle that means the same thing on both sides of an
/// unlock, and it is the same handle the sweep will key on a moment later.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PendingClassify {
    pub body: Option<String>,
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

    /// **Tell the device which senders to look at.**
    ///
    /// ★★★ Pushed, not pulled, and that is forced by where the decision
    /// happens: the receiver runs while the app does not, so the list has to be
    /// sitting on the device before a text arrives.
    ///
    /// ★★ Sending an EMPTY list is meaningful -- it says read nothing -- so
    /// it is passed through rather than treated as "unset".
    pub fn set_threads(&self, _senders: Vec<String>) -> Result<()> {
        #[cfg(target_os = "android")]
        {
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                senders: Vec<String>,
            }
            self.0
                .run_mobile_plugin::<()>("setThreads", Args { senders: _senders })
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Ok(())
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

    /// Whatever arrived while the app was closed, up to `limit`. Taking clears
    /// what was taken, so a message is handed over once.
    pub fn drain_queue(&self, _args: DrainArgs) -> Result<SmsBatch> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<SmsBatch>("drainQueue", _args)
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Err(Error::Unsupported)
    }

    /// How many texts are waiting, without taking any.
    pub fn queue_depth(&self) -> Result<u32> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<QueueDepth>("queueDepth", Empty {})
                .map(|d| d.depth)
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Err(Error::Unsupported)
    }

    /// The text a notification tap was about, taken once.
    ///
    /// ★★ Consumed rather than read, so a target that was already acted on
    /// cannot re-open the same card on a later, unrelated launch.
    pub fn consume_pending_classify(&self) -> Result<Option<String>> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<PendingClassify>("consumePendingClassify", Empty {})
                .map(|p| p.body)
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Ok(None)
    }

    /// Take the prompt down.
    ///
    /// ★★ Called once the queue has actually been swept rather than when the
    /// app merely opens: a prompt cancelled by launching claims the work is
    /// done when it is not.
    pub fn clear_classify_prompt(&self) -> Result<()> {
        #[cfg(target_os = "android")]
        {
            self.0
                .run_mobile_plugin::<serde_json::Value>("clearClassifyPrompt", Empty {})
                .map(|_| ())
                .map_err(|e| Error::PluginInvoke(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        Ok(())
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
