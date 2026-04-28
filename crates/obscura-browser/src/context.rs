use std::sync::Arc;

use obscura_net::{BlocklistConfig, CookieJar, ObscuraHttpClient, RobotsCache};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct BrowserProfile {
    pub webdriver: bool,
    pub hardware_concurrency: u8,
    pub device_memory: u8,
    pub language: String,
    pub languages: Vec<String>,
    pub platform: String,
    pub ua_data_platform: String,
    pub plugins: Vec<serde_json::Value>,
    pub mime_types: Vec<serde_json::Value>,
    pub permissions_default: String,
    pub permissions_notifications: String,
    pub permissions_geolocation: String,
}

impl Default for BrowserProfile {
    fn default() -> Self {
        Self {
            webdriver: false,
            hardware_concurrency: 8,
            device_memory: 8,
            language: "en-US".to_string(),
            languages: vec!["en-US".to_string(), "en".to_string()],
            platform: "Linux x86_64".to_string(),
            ua_data_platform: "Linux".to_string(),
            plugins: vec![
                json!({"name":"PDF Viewer","filename":"internal-pdf-viewer","description":"Portable Document Format","length":1}),
                json!({"name":"Chrome PDF Viewer","filename":"internal-pdf-viewer","description":"Portable Document Format","length":1}),
                json!({"name":"Chromium PDF Viewer","filename":"internal-pdf-viewer","description":"Portable Document Format","length":1}),
                json!({"name":"Microsoft Edge PDF Viewer","filename":"internal-pdf-viewer","description":"Portable Document Format","length":1}),
                json!({"name":"WebKit built-in PDF","filename":"internal-pdf-viewer","description":"Portable Document Format","length":1}),
            ],
            mime_types: vec![
                json!({"type":"application/pdf","description":"Portable Document Format","suffixes":"pdf","enabledPlugin":null}),
                json!({"type":"text/pdf","description":"Portable Document Format","suffixes":"pdf","enabledPlugin":null}),
            ],
            permissions_default: "granted".to_string(),
            permissions_notifications: "default".to_string(),
            permissions_geolocation: "prompt".to_string(),
        }
    }
}

impl BrowserProfile {
    pub fn to_js_object_literal(&self) -> String {
        let obj = json!({
            "webdriver": self.webdriver,
            "hardwareConcurrency": self.hardware_concurrency,
            "deviceMemory": self.device_memory,
            "language": self.language,
            "languages": self.languages,
            "platform": self.platform,
            "uaDataPlatform": self.ua_data_platform,
            "plugins": self.plugins,
            "mimeTypes": self.mime_types,
            "permissions": {
                "default": self.permissions_default,
                "notifications": self.permissions_notifications,
                "geolocation": self.permissions_geolocation
            }
        });
        obj.to_string()
    }
}

pub struct BrowserContext {
    pub id: String,
    pub cookie_jar: Arc<CookieJar>,
    pub http_client: Arc<ObscuraHttpClient>,
    pub user_agent: String,
    pub proxy_url: Option<String>,
    pub robots_cache: Arc<RobotsCache>,
    pub obey_robots: bool,
    pub stealth: bool,
    pub tracker_blocklist_config: BlocklistConfig,
    pub profile: BrowserProfile,
}

impl BrowserContext {
    pub fn new(id: String) -> Self {
        let cookie_jar = Arc::new(CookieJar::new());
        let http_client = Arc::new(ObscuraHttpClient::with_cookie_jar(cookie_jar.clone()));
        BrowserContext {
            id,
            cookie_jar,
            http_client,
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36".to_string(),
            proxy_url: None,
            robots_cache: Arc::new(RobotsCache::new()),
            obey_robots: false,
            stealth: false,
            tracker_blocklist_config: BlocklistConfig::default(),
            profile: BrowserProfile::default(),
        }
    }

    pub fn with_options(id: String, proxy_url: Option<String>, stealth: bool) -> Self {
        Self::with_options_and_blocklist(id, proxy_url, stealth, None)
    }

    pub fn with_options_and_blocklist(
        id: String,
        proxy_url: Option<String>,
        stealth: bool,
        tracker_blocklist_config: Option<BlocklistConfig>,
    ) -> Self {
        let cookie_jar = Arc::new(CookieJar::new());
        let mut client = ObscuraHttpClient::with_options(cookie_jar.clone(), proxy_url.as_deref());
        client.block_trackers = stealth;
        let tracker_blocklist_config = tracker_blocklist_config.unwrap_or_default();
        client.tracker_blocklist_config = tracker_blocklist_config;

        let http_client = Arc::new(client);
        BrowserContext {
            id,
            cookie_jar,
            http_client,
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36".to_string(),
            proxy_url,
            robots_cache: Arc::new(RobotsCache::new()),
            obey_robots: false,
            stealth,
            tracker_blocklist_config,
            profile: BrowserProfile::default(),
        }
    }

    pub fn with_proxy(id: String, proxy_url: Option<String>) -> Self {
        Self::with_options(id, proxy_url, false)
    }
}
