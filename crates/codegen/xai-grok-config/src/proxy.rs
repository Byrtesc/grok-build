use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub proxy: Option<ProxyConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    pub all: Option<String>,
    pub http: Option<String>,
    pub https: Option<String>,
    pub no_proxy: Option<Vec<String>>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: ProxyMode::Auto,
            all: None,
            http: None,
            https: None,
            no_proxy: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    #[default]
    Auto,
    Config,
    Disabled,
}

pub trait EnvironmentProvider {
    fn var(&self, name: &str) -> Option<String>;
}

#[derive(Debug, Default)]
pub struct StdEnvironment;
impl EnvironmentProvider for StdEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.trim().is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapEnvironment(pub HashMap<String, String>);
impl EnvironmentProvider for MapEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned().filter(|v| !v.trim().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProxyConfig {
    pub http: Option<Url>,
    pub https: Option<Url>,
    pub no_proxy: Option<Vec<String>>,
    pub source: ProxyConfigSource,
    pub disabled: bool,
}

impl ResolvedProxyConfig {
    pub fn direct() -> Self {
        Self {
            http: None,
            https: None,
            no_proxy: None,
            source: ProxyConfigSource::Direct,
            disabled: false,
        }
    }
    pub fn disabled() -> Self {
        Self {
            disabled: true,
            source: ProxyConfigSource::Disabled,
            ..Self::direct()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyConfigSource {
    Config,
    Environment,
    Direct,
    Disabled,
    Mixed,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProxyConfigError {
    #[error("Invalid proxy URL in network.proxy.{field}: {reason}")]
    InvalidUrl { field: &'static str, reason: String },
    #[error("network.proxy.mode is \"config\", but no proxy URL was configured")]
    ConfigModeWithoutProxy,
    #[error("network.proxy.mode is \"disabled\", so all/http/https cannot be set")]
    DisabledModeWithProxy,
}

impl ProxyConfig {
    pub fn resolve(
        &self,
        env: &impl EnvironmentProvider,
    ) -> Result<ResolvedProxyConfig, ProxyConfigError> {
        match self.mode {
            ProxyMode::Disabled => {
                if self.all.is_some() || self.http.is_some() || self.https.is_some() {
                    return Err(ProxyConfigError::DisabledModeWithProxy);
                }
                Ok(ResolvedProxyConfig::disabled())
            }
            ProxyMode::Config => {
                if self.all.is_none() && self.http.is_none() && self.https.is_none() {
                    return Err(ProxyConfigError::ConfigModeWithoutProxy);
                }
                self.resolve_from_config_only()
            }
            ProxyMode::Auto => self.resolve_auto(env),
        }
    }

    fn resolve_from_config_only(&self) -> Result<ResolvedProxyConfig, ProxyConfigError> {
        let http = parse_opt(
            self.http.as_deref().or(self.all.as_deref()),
            if self.http.is_some() { "http" } else { "all" },
        )?;
        let https = parse_opt(
            self.https.as_deref().or(self.all.as_deref()),
            if self.https.is_some() { "https" } else { "all" },
        )?;
        Ok(ResolvedProxyConfig {
            http,
            https,
            no_proxy: self.no_proxy.clone(),
            source: ProxyConfigSource::Config,
            disabled: false,
        })
    }

    fn resolve_auto(
        &self,
        env: &impl EnvironmentProvider,
    ) -> Result<ResolvedProxyConfig, ProxyConfigError> {
        let env_http = env_first(env, "HTTP_PROXY", "http_proxy");
        let env_https = env_first(env, "HTTPS_PROXY", "https_proxy");
        let env_all = env_first(env, "ALL_PROXY", "all_proxy");
        let http_s = self
            .http
            .as_deref()
            .or(self.all.as_deref())
            .or(env_http.as_deref())
            .or(env_all.as_deref());
        let https_s = self
            .https
            .as_deref()
            .or(self.all.as_deref())
            .or(env_https.as_deref())
            .or(env_all.as_deref());
        let http = parse_opt(
            http_s,
            if self.http.is_some() {
                "http"
            } else if self.all.is_some() {
                "all"
            } else if env_http.is_some() {
                "HTTP_PROXY"
            } else {
                "ALL_PROXY"
            },
        )?;
        let https = parse_opt(
            https_s,
            if self.https.is_some() {
                "https"
            } else if self.all.is_some() {
                "all"
            } else if env_https.is_some() {
                "HTTPS_PROXY"
            } else {
                "ALL_PROXY"
            },
        )?;
        let no_proxy = self.no_proxy.clone().or_else(|| {
            env_first(env, "NO_PROXY", "no_proxy").map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
        });
        let source = match (
            self.http.is_some() || self.https.is_some() || self.all.is_some(),
            env_http.is_some() || env_https.is_some() || env_all.is_some(),
        ) {
            (true, true) => ProxyConfigSource::Mixed,
            (true, false) => ProxyConfigSource::Config,
            (false, true) => ProxyConfigSource::Environment,
            (false, false) => ProxyConfigSource::Direct,
        };
        Ok(ResolvedProxyConfig {
            http,
            https,
            no_proxy,
            source,
            disabled: false,
        })
    }
}

pub fn resolve_network_proxy(
    root: &toml::Value,
    env: &impl EnvironmentProvider,
) -> Result<ResolvedProxyConfig, ProxyConfigError> {
    let Some(network) = root.get("network") else {
        return ProxyConfig::default().resolve(env);
    };
    let cfg: NetworkConfig =
        network
            .clone()
            .try_into()
            .map_err(|e: toml::de::Error| ProxyConfigError::InvalidUrl {
                field: "network",
                reason: e.message().to_string(),
            })?;
    cfg.proxy.unwrap_or_default().resolve(env)
}

fn env_first(env: &impl EnvironmentProvider, upper: &str, lower: &str) -> Option<String> {
    env.var(upper).or_else(|| env.var(lower))
}
fn parse_opt(value: Option<&str>, field: &'static str) -> Result<Option<Url>, ProxyConfigError> {
    value.map(|v| parse_url(v, field)).transpose()
}
fn parse_url(value: &str, field: &'static str) -> Result<Url, ProxyConfigError> {
    let url = Url::parse(value).map_err(|e| ProxyConfigError::InvalidUrl {
        field,
        reason: e.to_string(),
    })?;
    match url.scheme() {
        "http" | "https" | "socks5" | "socks5h" => Ok(url),
        other => Err(ProxyConfigError::InvalidUrl {
            field,
            reason: format!("unsupported scheme \"{other}\""),
        }),
    }
}

pub fn redact_proxy_url(url: &Url) -> String {
    let mut redacted = url.clone();
    if !redacted.username().is_empty() {
        let username = redacted.username().to_string();
        let _ = redacted.set_username(&username);
        let _ = redacted.set_password(Some("***"));
    }
    redacted.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> MapEnvironment {
        MapEnvironment(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }
    fn cfg(t: &str, e: &MapEnvironment) -> Result<ResolvedProxyConfig, ProxyConfigError> {
        let root: toml::Value = toml::from_str(t).unwrap();
        resolve_network_proxy(&root, e)
    }

    #[test]
    fn missing_and_empty_network_keep_auto_env_compat() {
        let e = env(&[
            ("HTTP_PROXY", "http://upper:8080"),
            ("ALL_PROXY", "http://all:8080"),
        ]);
        assert_eq!(
            cfg("", &e).unwrap().http.unwrap().as_str(),
            "http://upper:8080/"
        );
        assert_eq!(
            cfg("[network]\n", &e).unwrap().http.unwrap().as_str(),
            "http://upper:8080/"
        );
    }

    #[test]
    fn config_modes_and_proxy_schemes_parse() {
        for proxy in [
            "http://127.0.0.1:1",
            "https://127.0.0.1:1",
            "socks5://127.0.0.1:1",
            "socks5h://127.0.0.1:1",
        ] {
            let r = cfg(
                &format!("[network.proxy]\nmode='config'\nall='{proxy}'\n"),
                &env(&[]),
            )
            .unwrap();
            assert!(r.http.unwrap().as_str().starts_with(proxy));
        }
    }

    #[test]
    fn priority_rules_are_stable() {
        let e = env(&[
            ("HTTP_PROXY", "http://env-http:1"),
            ("http_proxy", "http://lower:1"),
            ("HTTPS_PROXY", "http://env-https:1"),
            ("ALL_PROXY", "http://env-all:1"),
            ("NO_PROXY", "env.local"),
        ]);
        let r = cfg("[network.proxy]\nmode='auto'\nall='http://toml-all:1'\nhttp='http://toml-http:1'\nhttps='http://toml-https:1'\nno_proxy=['toml.local']\n", &e).unwrap();
        assert_eq!(r.http.unwrap().as_str(), "http://toml-http:1/");
        assert_eq!(r.https.unwrap().as_str(), "http://toml-https:1/");
        assert_eq!(r.no_proxy.unwrap(), vec!["toml.local"]);

        let r = cfg("[network.proxy]\nmode='auto'\n", &e).unwrap();
        assert_eq!(r.http.unwrap().as_str(), "http://env-http:1/");
        assert_eq!(r.https.unwrap().as_str(), "http://env-https:1/");
        assert_eq!(r.no_proxy.unwrap(), vec!["env.local"]);
    }

    #[test]
    fn config_and_disabled_ignore_environment() {
        let e = env(&[("HTTP_PROXY", "http://env:1")]);
        let r = cfg("[network.proxy]\nmode='config'\nall='http://toml:1'\n", &e).unwrap();
        assert_eq!(r.http.unwrap().as_str(), "http://toml:1/");
        let r = cfg("[network.proxy]\nmode='disabled'\n", &e).unwrap();
        assert!(r.disabled);
        assert!(r.http.is_none());
    }

    #[test]
    fn validation_and_redaction() {
        assert!(matches!(
            cfg("[network.proxy]\nmode='config'\n", &env(&[])),
            Err(ProxyConfigError::ConfigModeWithoutProxy)
        ));
        assert!(matches!(
            cfg(
                "[network.proxy]\nmode='disabled'\nall='http://x:1'\n",
                &env(&[])
            ),
            Err(ProxyConfigError::DisabledModeWithProxy)
        ));
        assert!(matches!(
            cfg("[network.proxy]\nmode='config'\nall='ftp://x'\n", &env(&[])),
            Err(ProxyConfigError::InvalidUrl { .. })
        ));
        let url = Url::parse("http://username:password@example.com:8080").unwrap();
        let redacted = redact_proxy_url(&url);
        assert!(!redacted.contains("password"));
        assert!(redacted.contains("username:***"));
    }
}
