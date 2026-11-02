//! Desktop portal requests (xdg-desktop-portal). On Wayland the portals stand in for what a
//! Windows app may do directly: the global shortcuts portal for the keyboard hook, and the
//! remote desktop portal for pressing the paste shortcut. A request answers through a
//! `Response` signal on its own object path, after any permission dialog has closed.
use crate::PlatformError;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
};
use zbus::{
    blocking::{Connection, MessageIterator},
    message::Type as MessageType,
    zvariant::{DynamicType, OwnedValue, Value},
    MatchRule,
};

pub const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
/// Matches the desktop entry the packages install, so dialogs and stored permissions name Onda.
pub const APP_ID: &str = "com.miille.onda";

pub type Results = HashMap<String, OwnedValue>;
pub type Options<'a> = HashMap<&'a str, Value<'a>>;

pub struct Portal {
    connection: Connection,
    requests: AtomicU32,
}

impl Portal {
    pub fn connect() -> Result<Self, PlatformError> {
        let connection = Connection::session().map_err(|_| PlatformError::UnsupportedPlatform)?;
        // An unsandboxed app names itself before its first request. Portals older than 1.19
        // have no registry and identify the app from its process instead.
        let _ = connection.call_method(
            Some(DESTINATION),
            PATH,
            Some("org.freedesktop.host.portal.Registry"),
            "Register",
            &(APP_ID, Options::new()),
        );
        Ok(Self {
            connection,
            requests: AtomicU32::new(0),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// The interface's version, or `None` when this desktop does not offer it.
    pub fn version(&self, interface: &str) -> Option<u32> {
        let reply = self
            .connection
            .call_method(
                Some(DESTINATION),
                PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(interface, "version"),
            )
            .ok()?;
        let value: OwnedValue = reply.body().deserialize().ok()?;
        u32::try_from(value).ok()
    }

    /// A token unique to this connection, for `handle_token` and `session_handle_token`.
    pub fn token(&self) -> String {
        format!(
            "onda{}_{}",
            std::process::id(),
            self.requests.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Calls `method`, whose options must carry `handle_token: token`, and waits for its answer.
    /// A dismissed dialog is `Cancelled`; a desktop without the interface is unsupported.
    pub fn request<B>(
        &self,
        interface: &str,
        method: &str,
        body: &B,
        token: &str,
    ) -> Result<Results, PlatformError>
    where
        B: serde::Serialize + DynamicType,
    {
        let sender = self
            .connection
            .unique_name()
            .ok_or(PlatformError::UnsupportedPlatform)?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = format!("{PATH}/request/{sender}/{token}");
        // Subscribe before calling, or a quick answer could arrive unheard.
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface("org.freedesktop.portal.Request")
            .and_then(|rule| rule.member("Response"))
            .and_then(|rule| rule.path(path.as_str()))
            .map_err(|_| failed())?
            .build();
        let mut responses = MessageIterator::for_match_rule(rule, &self.connection, Some(4))
            .map_err(|_| failed())?;
        self.call(interface, method, body)?;
        let message = responses.next().ok_or_else(failed)?.map_err(|_| failed())?;
        let (code, results): (u32, Results) = message.body().deserialize().map_err(|_| failed())?;
        match code {
            0 => Ok(results),
            1 => Err(PlatformError::Cancelled),
            _ => Err(failed()),
        }
    }

    pub fn call<B>(
        &self,
        interface: &str,
        method: &str,
        body: &B,
    ) -> Result<zbus::Message, PlatformError>
    where
        B: serde::Serialize + DynamicType,
    {
        self.connection
            .call_method(Some(DESTINATION), PATH, Some(interface), method, body)
            .map_err(|error| match error {
                zbus::Error::MethodError(name, _, _)
                    if matches!(
                        name.as_str(),
                        "org.freedesktop.DBus.Error.UnknownMethod"
                            | "org.freedesktop.DBus.Error.UnknownInterface"
                            | "org.freedesktop.DBus.Error.ServiceUnknown"
                            | "org.freedesktop.DBus.Error.NotSupported"
                    ) =>
                {
                    PlatformError::UnsupportedPlatform
                }
                _ => failed(),
            })
    }
}

fn failed() -> PlatformError {
    PlatformError::TargetUnavailable("the desktop portal request failed".into())
}

/// A session handle from a request's results; portals return it as a string.
pub fn session_handle(results: &Results) -> Result<String, PlatformError> {
    match results.get("session_handle").map(|value| &**value) {
        Some(Value::Str(path)) => Ok(path.to_string()),
        Some(Value::ObjectPath(path)) => Ok(path.to_string()),
        _ => Err(failed()),
    }
}
