use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Client, Method};
use tokio::sync::RwLock;
use url::Url;

use crate::blocklist::BlocklistConfig;
use crate::cookies::CookieJar;
use crate::interceptor::{InterceptAction, RequestInterceptor};

const REQUIRED_HTTP2_WINDOW_UPDATE_INCREMENT: u32 = 15_663_105;
const REQUIRED_HTTP2_SETTINGS_ORDER: [&str; 5] = [
    "HEADER_TABLE_SIZE",
    "ENABLE_PUSH",
    "MAX_CONCURRENT_STREAMS",
    "INITIAL_WINDOW_SIZE",
    "MAX_HEADER_LIST_SIZE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsFingerprintParams {
    pub cipher_suite_order: Vec<String>,
    pub extension_order: Vec<String>,
    pub grease_injection_positions: Vec<usize>,
    pub supported_groups_order: Vec<String>,
    pub expected_ja3_hash: Option<String>,
    pub expected_ja4_hash: Option<String>,
}

impl TlsFingerprintParams {
    fn validate(&self) -> Result<(), ObscuraNetError> {
        if self.cipher_suite_order.is_empty() {
            return Err(ObscuraNetError::FingerprintConfig(
                "TLS fingerprint must include cipher suite ordering".to_string(),
            ));
        }
        if self.extension_order.is_empty() {
            return Err(ObscuraNetError::FingerprintConfig(
                "TLS fingerprint must include extension ordering".to_string(),
            ));
        }
        if self.supported_groups_order.is_empty() {
            return Err(ObscuraNetError::FingerprintConfig(
                "TLS fingerprint must include supported group ordering".to_string(),
            ));
        }
        for &pos in &self.grease_injection_positions {
            if pos > self.extension_order.len() {
                return Err(ObscuraNetError::FingerprintConfig(format!(
                    "GREASE insertion index {} is outside extension order length {}",
                    pos,
                    self.extension_order.len()
                )));
            }
        }
        for (label, hash) in [
            ("JA3", self.expected_ja3_hash.as_deref()),
            ("JA4", self.expected_ja4_hash.as_deref()),
        ] {
            if let Some(value) = hash {
                if value.trim().is_empty() {
                    return Err(ObscuraNetError::FingerprintConfig(format!(
                        "{label} expected hash cannot be empty"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Http2FingerprintConfig {
    pub settings_in_order: Vec<(String, u32)>,
    pub window_update_increment: u32,
}

impl Http2FingerprintConfig {
    pub fn chrome_like_default() -> Self {
        Self {
            settings_in_order: vec![
                ("HEADER_TABLE_SIZE".to_string(), 65_536),
                ("ENABLE_PUSH".to_string(), 0),
                ("MAX_CONCURRENT_STREAMS".to_string(), 1_000),
                ("INITIAL_WINDOW_SIZE".to_string(), 6_291_456),
                ("MAX_HEADER_LIST_SIZE".to_string(), 262_144),
            ],
            window_update_increment: REQUIRED_HTTP2_WINDOW_UPDATE_INCREMENT,
        }
    }

    fn validate_preface_behavior(&self) -> Result<(), ObscuraNetError> {
        let setting_names: Vec<&str> = self
            .settings_in_order
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        if setting_names != REQUIRED_HTTP2_SETTINGS_ORDER {
            return Err(ObscuraNetError::FingerprintConfig(format!(
                "HTTP/2 SETTINGS order must be {:?}, got {:?}",
                REQUIRED_HTTP2_SETTINGS_ORDER, setting_names
            )));
        }
        if self.window_update_increment != REQUIRED_HTTP2_WINDOW_UPDATE_INCREMENT {
            return Err(ObscuraNetError::FingerprintConfig(format!(
                "HTTP/2 WINDOW_UPDATE increment must be {}",
                REQUIRED_HTTP2_WINDOW_UPDATE_INCREMENT
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserHeaderProfile {
    pub sec_ch_ua: String,
    pub sec_ch_ua_platform: String,
    pub accept_language: String,
}

impl BrowserHeaderProfile {
    fn ordered_pairs(&self) -> [(&'static str, &str); 3] {
        [
            ("sec-ch-ua", self.sec_ch_ua.as_str()),
            ("sec-ch-ua-platform", self.sec_ch_ua_platform.as_str()),
            ("accept-language", self.accept_language.as_str()),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintedTransportConfig {
    pub tls: TlsFingerprintParams,
    pub http2: Http2FingerprintConfig,
    pub headers: BrowserHeaderProfile,
}

impl FingerprintedTransportConfig {
    fn validate(&self) -> Result<(), ObscuraNetError> {
        self.tls.validate()?;
        self.http2.validate_preface_behavior()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportMode {
    NonFingerprintable,
    Fingerprinted(FingerprintedTransportConfig),
}

#[derive(Debug, Clone)]
pub struct Response {
    pub url: Url,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub redirected_from: Vec<Url>,
}

impl Response {
    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    pub fn is_html(&self) -> bool {
        self.content_type()
            .map(|ct| ct.contains("text/html"))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub struct RequestInfo {
    pub url: Url,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub resource_type: ResourceType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceType {
    Document,
    Script,
    Stylesheet,
    Image,
    Font,
    Xhr,
    Fetch,
    Other,
}

pub type RequestCallback = Arc<dyn Fn(&RequestInfo) + Send + Sync>;
pub type ResponseCallback = Arc<dyn Fn(&RequestInfo, &Response) + Send + Sync>;

fn non_fingerprintable_base_headers(ua: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(ua).unwrap_or_else(|_| {
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36",
        )
    }));
    // This reqwest path is intentionally non-browser-fingerprinted.
    // Browser-fidelity traffic must use the StealthHttpClient (wreq).
    headers.insert(reqwest::header::ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(
        HeaderName::from_static("x-obscura-client-profile"),
        HeaderValue::from_static("non-fingerprintable"),
    );
    headers
}

#[cfg(test)]
mod header_tests {
    use super::*;

    #[test]
    fn non_fingerprintable_profile_headers_are_stable() {
        let headers = non_fingerprintable_base_headers("test-ua");
        assert_eq!(
            headers
                .get("x-obscura-client-profile")
                .and_then(|v| v.to_str().ok()),
            Some("non-fingerprintable")
        );
        assert!(headers.get("sec-ch-ua").is_none());
        assert!(headers.get("sec-fetch-mode").is_none());
    }

    #[test]
    fn http2_preface_validation_requires_expected_window_update() {
        let mut http2 = Http2FingerprintConfig::chrome_like_default();
        http2.window_update_increment = 1;
        assert!(http2.validate_preface_behavior().is_err());
    }

    #[tokio::test]
    async fn fingerprinted_mode_emits_profile_headers_in_browser_order() {
        let client = ObscuraHttpClient::new();
        let tls = TlsFingerprintParams {
            cipher_suite_order: vec!["TLS_AES_128_GCM_SHA256".to_string()],
            extension_order: vec!["server_name".to_string(), "supported_groups".to_string()],
            grease_injection_positions: vec![0],
            supported_groups_order: vec!["X25519".to_string()],
            expected_ja3_hash: Some("abc123".to_string()),
            expected_ja4_hash: Some("def456".to_string()),
        };
        let config = FingerprintedTransportConfig {
            tls,
            http2: Http2FingerprintConfig::chrome_like_default(),
            headers: BrowserHeaderProfile {
                sec_ch_ua: "\"Chromium\";v=\"145\"".to_string(),
                sec_ch_ua_platform: "\"Linux\"".to_string(),
                accept_language: "en-US,en;q=0.9".to_string(),
            },
        };
        client
            .set_transport_mode(TransportMode::Fingerprinted(config.clone()))
            .await
            .unwrap();

        let headers = client
            .base_headers_for_mode("test-agent", &TransportMode::Fingerprinted(config))
            .unwrap();
        let mut observed = Vec::new();
        for (name, _) in headers.iter() {
            if name == "sec-ch-ua" || name == "sec-ch-ua-platform" || name == "accept-language" {
                observed.push(name.as_str().to_string());
            }
        }
        assert_eq!(
            observed,
            vec!["sec-ch-ua", "sec-ch-ua-platform", "accept-language"]
        );
    }
}

fn validate_url(url: &Url) -> Result<(), ObscuraNetError> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" && scheme != "file" {
        return Err(ObscuraNetError::Network(format!(
            "Forbidden URL scheme '{}' - only http, https, and file are allowed",
            scheme
        )));
    }

    if scheme == "file" {
        return Ok(());
    }

    if let Some(host) = url.host() {
        match host {
            url::Host::Ipv4(ip) => {
                if ip.is_loopback()
                    || ip.is_private()
                    || ip.is_link_local()
                    || ip.is_broadcast()
                    || ip.is_documentation()
                {
                    return Err(ObscuraNetError::Network(format!(
                        "Access to private/internal IP address {} is not allowed",
                        ip
                    )));
                }
            }
            url::Host::Ipv6(ip) => {
                if ip.is_loopback() || ip.is_unicast_link_local() {
                    return Err(ObscuraNetError::Network(format!(
                        "Access to private/internal IPv6 address {} is not allowed",
                        ip
                    )));
                }
            }
            url::Host::Domain(domain) => {
                let lower_domain = domain.to_lowercase();
                if lower_domain == "localhost"
                    || lower_domain.ends_with(".localhost")
                    || lower_domain == "127.0.0.1"
                    || lower_domain == "::1"
                {
                    return Err(ObscuraNetError::Network(format!(
                        "Access to localhost domain '{}' is not allowed",
                        domain
                    )));
                }
            }
        }
    }

    Ok(())
}

async fn fetch_file_url(url: &Url) -> Result<Response, ObscuraNetError> {
    let path = url
        .to_file_path()
        .map_err(|_| ObscuraNetError::Network("Invalid file URL".to_string()))?;
    let body = tokio::fs::read(&path)
        .await
        .map_err(|e| ObscuraNetError::Network(format!("Failed to read file: {}", e)))?;

    let mut headers = HashMap::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ct = match ext.to_lowercase().as_str() {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" | "mjs" => "application/javascript",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            _ => "application/octet-stream",
        };
        headers.insert("content-type".to_string(), ct.to_string());
    }

    Ok(Response {
        url: url.clone(),
        status: 200,
        headers,
        body,
        redirected_from: Vec::new(),
    })
}

fn synthetic_blocked_response(url: &Url) -> Response {
    let mut headers = HashMap::new();
    let (content_type, body): (&str, Vec<u8>) = if url.path().ends_with(".json") {
        ("application/json; charset=utf-8", b"{}".to_vec())
    } else {
        ("application/javascript; charset=utf-8", Vec::new())
    };
    headers.insert("content-type".to_string(), content_type.to_string());
    headers.insert("cache-control".to_string(), "no-store".to_string());

    Response {
        url: url.clone(),
        status: 200,
        headers,
        body,
        redirected_from: Vec::new(),
    }
}

pub struct ObscuraHttpClient {
    client: tokio::sync::OnceCell<Client>,
    proxy_url: Option<String>,
    pub cookie_jar: Arc<CookieJar>,
    pub user_agent: RwLock<String>,
    pub extra_headers: RwLock<HashMap<String, String>>,
    pub interceptor: RwLock<Option<Box<dyn RequestInterceptor + Send + Sync>>>,
    pub on_request: RwLock<Vec<RequestCallback>>,
    pub on_response: RwLock<Vec<ResponseCallback>>,
    pub timeout: Duration,
    pub in_flight: Arc<std::sync::atomic::AtomicU32>,
    pub block_trackers: bool,
    pub tracker_blocklist_config: BlocklistConfig,
    pub transport_mode: RwLock<TransportMode>,
}

impl ObscuraHttpClient {
    pub fn new() -> Self {
        Self::with_cookie_jar(Arc::new(CookieJar::new()))
    }

    pub fn with_cookie_jar(cookie_jar: Arc<CookieJar>) -> Self {
        Self::with_options(cookie_jar, None)
    }

    pub fn with_options(cookie_jar: Arc<CookieJar>, proxy_url: Option<&str>) -> Self {
        ObscuraHttpClient {
            client: tokio::sync::OnceCell::new(),
            proxy_url: proxy_url.map(|s| s.to_string()),
            cookie_jar,
            user_agent: RwLock::new(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36".to_string(),
            ),
            extra_headers: RwLock::new(HashMap::new()),
            interceptor: RwLock::new(None),
            on_request: RwLock::new(Vec::new()),
            on_response: RwLock::new(Vec::new()),
            in_flight: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            timeout: Duration::from_secs(30),
            block_trackers: false,
            tracker_blocklist_config: BlocklistConfig::default(),
            transport_mode: RwLock::new(TransportMode::NonFingerprintable),
        }
    }

    async fn get_client(&self) -> &Client {
        self.client
            .get_or_init(|| async {
                let mut builder = Client::builder()
                    .redirect(Policy::none())
                    .timeout(Duration::from_secs(30))
                    .danger_accept_invalid_certs(false);

                if let Some(ref proxy) = self.proxy_url {
                    if let Ok(p) = reqwest::Proxy::all(proxy.as_str()) {
                        builder = builder.proxy(p);
                    }
                }

                builder.build().expect("failed to build HTTP client")
            })
            .await
    }

    pub async fn fetch(&self, url: &Url) -> Result<Response, ObscuraNetError> {
        self.fetch_with_method(Method::GET, url, None).await
    }

    pub async fn post_form(&self, url: &Url, body: &str) -> Result<Response, ObscuraNetError> {
        self.fetch_with_method(Method::POST, url, Some(body.as_bytes().to_vec()))
            .await
    }

    pub async fn fetch_with_method(
        &self,
        initial_method: Method,
        url: &Url,
        initial_body: Option<Vec<u8>>,
    ) -> Result<Response, ObscuraNetError> {
        validate_url(url)?;

        if url.scheme() == "file" {
            return fetch_file_url(url).await;
        }

        let mut method = initial_method;
        let mut body = initial_body;
        if self.block_trackers {
            if let Some(host) = url.host_str() {
                if crate::blocklist::is_blocked_with_config(host, &self.tracker_blocklist_config) {
                    tracing::debug!("Blocked tracker: {}", url);
                    return Ok(synthetic_blocked_response(url));
                }
            }
        }

        let mut current_url = url.clone();
        let mut redirects = Vec::new();
        let max_redirects = 20;

        for _redirect_count in 0..max_redirects {
            let request_info = RequestInfo {
                url: current_url.clone(),
                method: method.to_string(),
                headers: self.extra_headers.read().await.clone(),
                resource_type: ResourceType::Document,
            };

            if let Some(interceptor) = self.interceptor.read().await.as_ref() {
                match interceptor.intercept(&request_info).await {
                    InterceptAction::Continue => {}
                    InterceptAction::Block => {
                        return Err(ObscuraNetError::Blocked(current_url.to_string()));
                    }
                    InterceptAction::Fulfill(response) => {
                        return Ok(response);
                    }
                    InterceptAction::ModifyHeaders(headers) => {
                        let mut extra = self.extra_headers.write().await;
                        extra.extend(headers);
                    }
                }
            }

            for cb in self.on_request.read().await.iter() {
                cb(&request_info);
            }

            let ua = self.user_agent.read().await.clone();
            let transport_mode = self.transport_mode.read().await.clone();
            let mut headers = self.base_headers_for_mode(&ua, &transport_mode)?;

            let cookie_header = self.cookie_jar.get_cookie_header(&current_url);
            if !cookie_header.is_empty() {
                if let Ok(val) = HeaderValue::from_str(&cookie_header) {
                    headers.insert(reqwest::header::COOKIE, val);
                }
            }

            for (k, v) in self.extra_headers.read().await.iter() {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_str(v),
                ) {
                    headers.insert(name, val);
                }
            }

            let mut req_builder = self
                .get_client()
                .await
                .request(method.clone(), current_url.as_str())
                .headers(headers);

            if let Some(ref b) = body {
                if method == Method::POST {
                    req_builder = req_builder.header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    );
                }
                req_builder = req_builder.body(b.clone());
            }

            self.in_flight
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let resp = req_builder.send().await.map_err(|e| {
                self.in_flight
                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                ObscuraNetError::Network(format!("{}: {}", current_url, e))
            })?;
            self.in_flight
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

            let status = resp.status();

            for val in resp.headers().get_all(reqwest::header::SET_COOKIE) {
                if let Ok(s) = val.to_str() {
                    self.cookie_jar.set_cookie(s, &current_url);
                }
            }

            let response_headers: HashMap<String, String> = resp
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_lowercase(),
                        v.to_str().unwrap_or("").to_string(),
                    )
                })
                .collect();

            if status.is_redirection() {
                if let Some(location) = resp.headers().get(reqwest::header::LOCATION) {
                    let location_str = location.to_str().map_err(|_| {
                        ObscuraNetError::Network("Invalid redirect Location header".into())
                    })?;
                    let next_url = current_url.join(location_str).map_err(|e| {
                        ObscuraNetError::Network(format!("Invalid redirect URL: {}", e))
                    })?;
                    validate_url(&next_url)?;
                    redirects.push(current_url.clone());
                    current_url = next_url;
                    if status == reqwest::StatusCode::MOVED_PERMANENTLY
                        || status == reqwest::StatusCode::FOUND
                        || status == reqwest::StatusCode::SEE_OTHER
                    {
                        method = Method::GET;
                        body = None;
                    }
                    continue;
                }
            }

            let body_bytes = resp
                .bytes()
                .await
                .map_err(|e| ObscuraNetError::Network(format!("Failed to read body: {}", e)))?
                .to_vec();

            let response = Response {
                url: current_url,
                status: status.as_u16(),
                headers: response_headers,
                body: body_bytes,
                redirected_from: redirects,
            };

            for cb in self.on_response.read().await.iter() {
                cb(&request_info, &response);
            }

            return Ok(response);
        }

        Err(ObscuraNetError::TooManyRedirects(current_url.to_string()))
    }

    pub async fn set_user_agent(&self, ua: &str) {
        *self.user_agent.write().await = ua.to_string();
    }

    pub async fn set_extra_headers(&self, headers: HashMap<String, String>) {
        *self.extra_headers.write().await = headers;
    }

    pub async fn set_transport_mode(&self, mode: TransportMode) -> Result<(), ObscuraNetError> {
        if let TransportMode::Fingerprinted(cfg) = &mode {
            cfg.validate()?;
        }
        *self.transport_mode.write().await = mode;
        Ok(())
    }

    fn base_headers_for_mode(
        &self,
        ua: &str,
        mode: &TransportMode,
    ) -> Result<HeaderMap, ObscuraNetError> {
        match mode {
            TransportMode::NonFingerprintable => Ok(non_fingerprintable_base_headers(ua)),
            TransportMode::Fingerprinted(cfg) => {
                let mut headers = HeaderMap::new();
                headers.insert(USER_AGENT, HeaderValue::from_str(ua).unwrap_or_else(|_| {
                    HeaderValue::from_static(
                        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36",
                    )
                }));
                headers.insert(
                    HeaderName::from_static("x-obscura-client-profile"),
                    HeaderValue::from_static("fingerprinted"),
                );
                self.enforce_handshake_fingerprint(cfg)?;
                self.insert_profile_headers(&mut headers, &cfg.headers)?;
                Ok(headers)
            }
        }
    }

    fn insert_profile_headers(
        &self,
        headers: &mut HeaderMap,
        profile: &BrowserHeaderProfile,
    ) -> Result<(), ObscuraNetError> {
        for (name, value) in profile.ordered_pairs() {
            let header_name = HeaderName::from_static(name);
            let header_value = HeaderValue::from_str(value).map_err(|e| {
                ObscuraNetError::FingerprintConfig(format!(
                    "Invalid profile header {} value: {}",
                    name, e
                ))
            })?;
            headers.insert(header_name, header_value);
        }
        Ok(())
    }

    fn enforce_handshake_fingerprint(
        &self,
        cfg: &FingerprintedTransportConfig,
    ) -> Result<(), ObscuraNetError> {
        cfg.validate()?;
        let mut ordered_settings = BTreeMap::new();
        for (idx, (name, value)) in cfg.http2.settings_in_order.iter().enumerate() {
            ordered_settings.insert(idx, (name, value));
        }
        for (idx, required_name) in REQUIRED_HTTP2_SETTINGS_ORDER.iter().enumerate() {
            let present = ordered_settings.get(&idx).map(|(name, _)| name.as_str());
            if present != Some(*required_name) {
                return Err(ObscuraNetError::FingerprintConfig(format!(
                    "Handshake creation rejected: setting at index {} must be {}",
                    idx, required_name
                )));
            }
        }
        Ok(())
    }

    pub fn active_requests(&self) -> u32 {
        self.in_flight.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn is_network_idle(&self) -> bool {
        self.active_requests() == 0
    }
}

impl Default for ObscuraHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;

    #[tokio::test]
    async fn blocked_tracker_script_gets_synthetic_success_response() {
        let mut client = ObscuraHttpClient::new();
        client.block_trackers = true;

        let url = Url::parse("https://www.google-analytics.com/ga.js").unwrap();
        let response = client.fetch(&url).await.unwrap();

        assert_eq!(response.status, 200);
        assert_ne!(response.status, 0);
        assert_eq!(
            response.header("content-type"),
            Some("application/javascript; charset=utf-8")
        );
        assert_eq!(response.header("cache-control"), Some("no-store"));
        assert!(response.body.is_empty());
    }

    #[tokio::test]
    async fn blocked_tracker_json_gets_minimal_json_body() {
        let mut client = ObscuraHttpClient::new();
        client.block_trackers = true;

        let url = Url::parse("https://doubleclick.net/events/config.json").unwrap();
        let response = client.fetch(&url).await.unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(
            response.header("content-type"),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(response.header("cache-control"), Some("no-store"));
        assert_eq!(response.body, b"{}");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ObscuraNetError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Too many redirects: {0}")]
    TooManyRedirects(String),

    #[error("Request blocked: {0}")]
    Blocked(String),

    #[error("Fingerprint configuration error: {0}")]
    FingerprintConfig(String),
}
