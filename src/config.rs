use derive_more::with_trait::FromStr;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use tracing::{trace, Level};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, FromStr, Display)]
pub struct Port(pub u16);
impl Deref for Port {
    type Target = u16;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Default for Port {
    fn default() -> Self {
        Self(80)
    }
}
#[derive(
    Clone, Debug, Serialize, Deserialize, PartialEq, Eq, FromStr, Display, Ord, PartialOrd,
)]

pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl From<tracing::Level> for LogLevel {
    fn from(val: tracing::Level) -> LogLevel {
        match val {
            Level::DEBUG => LogLevel::Debug,
            Level::INFO => LogLevel::Info,
            Level::WARN => LogLevel::Warn,
            Level::ERROR => LogLevel::Error,
            Level::TRACE => LogLevel::Trace,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Display)]
#[display("\nUrl = {host}:{port},\nhttps = {use_https},\nself signed = {self_signed},\nloglevel = {log_level}")]
pub struct Config {
    pub host: String,
    #[display("{host}")]
    #[display("{port}")]
    pub port: Port,
    pub use_https: bool,
    pub http_redirect: bool,
    pub self_signed: bool,
    pub log_level: LogLevel,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Builder {
    host: Option<String>,
    port: Option<Port>,
    http_redirect: bool,
    use_https: bool,
    self_signed: bool,
    log_level: Option<LogLevel>,
}
impl Builder {
    pub fn new() -> Self {
        Self {
            host: None,
            port: None,
            http_redirect: false,
            use_https: false,
            self_signed: false,
            log_level: None,
        }
    }
    pub fn with_host(mut self, host: String) -> Self {
        trace!("Setting host: {}", host);
        self.host = Some(host);
        self
    }
    pub fn with_port(mut self, port: Port) -> Self {
        trace!("Setting port: {}", port);
        self.port = Some(port);
        self
    }
    pub fn with_https(mut self, use_https: bool) -> Self {
        trace!("Setting use_https: {}", use_https);
        self.use_https = use_https;
        self
    }
    pub fn with_self_signed(mut self, self_signed: bool) -> Self {
        trace!("Setting self_signed: {}", self_signed);
        self.self_signed = self_signed;
        self
    }
    pub fn with_log_level(mut self, log_level: LogLevel) -> Self {
        trace!("Setting log level: {}", log_level);
        self.log_level = Some(log_level);
        self
    }
    pub fn with_redirect(mut self, redirect: bool) -> Self {
        trace!("Setting do http redirect: {}", redirect);
        self.http_redirect = redirect;
        self
    }
    pub fn build(&self) -> Config {
        // we clone since the builder should only be called once.
        // the builder memory will be dropped after it goes out of scope, so i deem this
        // acceptable.
        let config = Config {
            host: self.host.clone().unwrap_or("localhost".to_string()),
            port: self.port.clone().unwrap_or(Port(80)),
            use_https: self.use_https,
            http_redirect: self.http_redirect,
            self_signed: self.self_signed,
            log_level: self.log_level.clone().unwrap_or_default(),
        };
        trace!("Built Config: {}", config);
        config
    }
}
