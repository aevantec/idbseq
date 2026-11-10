//! Blocking HTTP client for the v1 API: builds requests, unwraps the envelope.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use ureq::config::Config;
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};
use ureq::{Agent, RequestBuilder};

use crate::error::CliError;

const USER_AGENT: &str = concat!("idbseq-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

/// One API call. Paths are relative to the base URL and start with `/v1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    method: Method,
    path: String,
    query: Vec<(&'static str, String)>,
    body: Option<Value>,
    headers: Vec<(&'static str, String)>,
    authenticated: bool,
}

impl Request {
    fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: Vec::new(),
            body: None,
            headers: Vec::new(),
            authenticated: true,
        }
    }

    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::Get, path)
    }

    pub fn post(path: impl Into<String>) -> Self {
        Self::new(Method::Post, path)
    }

    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(Method::Patch, path)
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(Method::Delete, path)
    }

    /// Adds a query parameter when `value` is present.
    #[must_use]
    pub fn query(self, name: &'static str, value: Option<impl ToString>) -> Self {
        match value {
            Some(value) => Self {
                query: [self.query, vec![(name, value.to_string())]].concat(),
                ..self
            },
            None => self,
        }
    }

    #[must_use]
    pub fn body(self, body: Value) -> Self {
        Self {
            body: Some(body),
            ..self
        }
    }

    #[must_use]
    pub fn header(self, name: &'static str, value: Option<String>) -> Self {
        match value {
            Some(value) => Self {
                headers: [self.headers, vec![(name, value)]].concat(),
                ..self
            },
            None => self,
        }
    }

    /// Sends no `Authorization` header (login).
    #[must_use]
    pub fn anonymous(self) -> Self {
        Self {
            authenticated: false,
            ..self
        }
    }
}

/// A success envelope's `data` and `meta`.
#[derive(Debug, Clone, PartialEq)]
pub struct Success {
    pub data: Value,
    pub meta: Value,
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
#[must_use]
pub fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Validates a base URL and strips trailing slashes.
///
/// # Errors
///
/// [`CliError::Usage`] unless the URL is `http://` or `https://` with a host.
pub fn normalise_base_url(url: &str) -> Result<String, CliError> {
    let trimmed = url.trim().trim_end_matches('/');
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"));
    match rest {
        Some(host) if !host.is_empty() => Ok(trimmed.to_owned()),
        _ => Err(CliError::Usage(format!(
            "invalid server URL `{url}`: expected http://host[:port] or https://…"
        ))),
    }
}

/// Unwraps a response body into data and meta, or the matching error.
///
/// # Errors
///
/// [`CliError::Api`] for error envelopes, [`CliError::Unexpected`] for anything else.
pub fn interpret(status: u16, request_id: Option<String>, text: &str) -> Result<Success, CliError> {
    let unexpected = |message: &str| CliError::Unexpected {
        status,
        message: message.to_owned(),
    };
    let body: Value = serde_json::from_str(text).map_err(|_| unexpected("the body is not JSON"))?;
    match body.get("success").and_then(Value::as_bool) {
        Some(true) if (200..300).contains(&status) => Ok(Success {
            data: body.get("data").cloned().unwrap_or(Value::Null),
            meta: body.get("meta").cloned().unwrap_or(Value::Null),
        }),
        Some(true) => Err(unexpected("a success envelope with an error status")),
        Some(false) => {
            let error = body.get("error").unwrap_or(&Value::Null);
            let code = error
                .get("code")
                .and_then(Value::as_str)
                .ok_or_else(|| unexpected("an error envelope without a code"))?;
            Err(CliError::Api {
                code: code.to_owned(),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or(code)
                    .to_owned(),
                status,
                details: error.get("details").cloned().unwrap_or(Value::Null),
                request_id,
            })
        }
        None => Err(unexpected("the body is not an idbseq envelope")),
    }
}

fn native_roots() -> Result<RootCerts, CliError> {
    let found = rustls_native_certs::load_native_certs();
    if found.certs.is_empty() {
        return Err(CliError::Io(
            "no trusted root certificates found on this system".to_owned(),
        ));
    }
    let certs: Vec<Certificate<'static>> = found
        .certs
        .iter()
        .map(|der| Certificate::from_der(der.as_ref()).to_owned())
        .collect();
    Ok(RootCerts::new_with_certs(&certs))
}

fn tls_config(https: bool) -> Result<TlsConfig, CliError> {
    let builder = TlsConfig::builder()
        .provider(TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()));
    // Plain-HTTP servers never touch TLS, so skip reading the system trust store.
    let builder = if https {
        builder.root_certs(native_roots()?)
    } else {
        builder
    };
    Ok(builder.build())
}

fn with_headers<B>(builder: RequestBuilder<B>, headers: &[(&str, String)]) -> RequestBuilder<B> {
    headers.iter().fold(builder, |builder, (name, value)| {
        builder.header(*name, value)
    })
}

/// Client bound to one server and one credential.
pub struct Client {
    agent: Agent,
    base_url: String,
    credential: Option<String>,
}

impl Client {
    /// Builds a client for `base_url` sending `credential` as a bearer token.
    ///
    /// # Errors
    ///
    /// [`CliError::Usage`] for an invalid URL, [`CliError::Io`] when an `https`
    /// URL is given and the system has no trusted root certificates.
    pub fn new(
        base_url: &str,
        credential: Option<String>,
        timeout: Duration,
    ) -> Result<Self, CliError> {
        let base_url = normalise_base_url(base_url)?;
        let tls = tls_config(base_url.starts_with("https://"))?;
        let agent = Config::builder()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .user_agent(USER_AGENT)
            .tls_config(tls)
            .build()
            .new_agent();
        Ok(Self {
            agent,
            base_url,
            credential,
        })
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, request: &Request) -> String {
        let query: Vec<String> = request
            .query
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect();
        let separator = if query.is_empty() { "" } else { "?" };
        format!(
            "{}{}{separator}{}",
            self.base_url,
            request.path,
            query.join("&")
        )
    }

    fn headers(&self, request: &Request) -> Vec<(&'static str, String)> {
        let auth = self
            .credential
            .as_ref()
            .filter(|_| request.authenticated)
            .map(|token| ("Authorization", format!("Bearer {token}")));
        [
            vec![("Accept", "application/json".to_owned())],
            auth.into_iter().collect(),
            request.headers.clone(),
        ]
        .concat()
    }

    /// Sends a request and unwraps the envelope.
    ///
    /// # Errors
    ///
    /// [`CliError::Network`] without a response, else as [`interpret`].
    pub fn send(&self, request: &Request) -> Result<Success, CliError> {
        let url = self.url(request);
        let headers = self.headers(request);
        let body = request.body.as_ref().map(Value::to_string);
        let result = match request.method {
            Method::Get => with_headers(self.agent.get(&url), &headers).call(),
            Method::Delete => with_headers(self.agent.delete(&url), &headers).call(),
            Method::Post | Method::Patch => {
                let builder = if request.method == Method::Post {
                    self.agent.post(&url)
                } else {
                    self.agent.patch(&url)
                };
                let builder = with_headers(builder, &headers);
                match body {
                    Some(body) => builder.content_type("application/json").send(body),
                    None => builder.send_empty(),
                }
            }
        };
        let network = |error: ureq::Error| CliError::Network(format!("{url}: {error}"));
        let mut response = result.map_err(network)?;
        let status = response.status().as_u16();
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let text = response.body_mut().read_to_string().map_err(network)?;
        interpret(status, request_id, &text)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode("eu.orders-1_~"), "eu.orders-1_~");
        assert_eq!(encode("a b/c&d=é"), "a%20b%2Fc%26d%3D%C3%A9");
    }

    #[test]
    fn normalises_and_validates_base_urls() {
        assert_eq!(
            normalise_base_url("http://ids:7878/").unwrap(),
            "http://ids:7878"
        );
        assert_eq!(
            normalise_base_url("https://ids/x//").unwrap(),
            "https://ids/x"
        );
        assert!(matches!(
            normalise_base_url("ids:7878"),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            normalise_base_url("http://"),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn interprets_a_success_envelope() {
        let success = interpret(
            200,
            None,
            r#"{"success":true,"data":{"id":9223372036854775807},"error":null,"meta":{"total":1}}"#,
        )
        .unwrap();
        assert_eq!(success.data, json!({ "id": 9_223_372_036_854_775_807_i64 }));
        assert_eq!(success.meta, json!({ "total": 1 }));
    }

    #[test]
    fn interprets_an_error_envelope() {
        let body = r#"{"success":false,"data":null,"error":{"code":"FORBIDDEN","message":"no","details":{"a":1}}}"#;

        let error = interpret(403, Some("r1".into()), body).unwrap_err();

        assert_eq!(
            error,
            CliError::Api {
                code: "FORBIDDEN".into(),
                message: "no".into(),
                status: 403,
                details: json!({ "a": 1 }),
                request_id: Some("r1".into()),
            }
        );
    }

    #[test]
    fn a_missing_error_message_defaults_to_the_code() {
        let error = interpret(
            500,
            None,
            r#"{"success":false,"error":{"code":"INTERNAL"}}"#,
        )
        .unwrap_err();
        assert!(matches!(error, CliError::Api { ref message, .. } if message == "INTERNAL"));
    }

    #[test]
    fn rejects_bodies_that_are_not_envelopes() {
        for (status, body) in [
            (502, "<html>"),
            (200, r#"{"id":1}"#),
            (500, r#"{"success":true,"data":null}"#),
            (400, r#"{"success":false,"error":null}"#),
        ] {
            let error = interpret(status, None, body).unwrap_err();
            assert!(
                matches!(error, CliError::Unexpected { status: s, .. } if s == status),
                "{body}"
            );
        }
    }

    #[test]
    fn builds_query_strings_and_skips_absent_values() {
        let client = Client::new("http://ids:1/", None, Duration::from_secs(1)).unwrap();
        let request = Request::get("/v1/collections")
            .query("limit", Some(5))
            .query("cursor", None::<String>)
            .query("prefix", Some("a b"));

        assert_eq!(
            client.url(&request),
            "http://ids:1/v1/collections?limit=5&prefix=a%20b"
        );
        assert_eq!(client.base_url(), "http://ids:1");
    }

    #[test]
    fn anonymous_requests_carry_no_credential() {
        let client =
            Client::new("http://ids:1", Some("tok".into()), Duration::from_secs(1)).unwrap();
        let signed =
            client.headers(&Request::post("/v1/x").header("Idempotency-Key", Some("k".into())));
        let anonymous = client.headers(&Request::post("/v1/auth/login").anonymous());

        assert!(signed.contains(&("Authorization", "Bearer tok".to_owned())));
        assert!(signed.contains(&("Idempotency-Key", "k".to_owned())));
        assert!(!anonymous.iter().any(|(name, _)| *name == "Authorization"));
    }

    #[test]
    fn https_clients_load_the_system_trust_store() {
        // Either the system has roots (normal) or we report it clearly.
        match Client::new("https://ids.example", None, Duration::from_secs(1)) {
            Ok(client) => assert_eq!(client.base_url(), "https://ids.example"),
            Err(error) => assert!(matches!(error, CliError::Io(_))),
        }
    }

    #[test]
    fn unreachable_servers_are_network_errors() {
        // Port 9 (discard) on localhost is closed on CI runners and dev machines.
        let client = Client::new("http://127.0.0.1:9", None, Duration::from_secs(2)).unwrap();
        let error = client.send(&Request::get("/healthz")).unwrap_err();
        assert!(matches!(error, CliError::Network(_)));
    }
}
