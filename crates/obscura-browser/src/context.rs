use std::sync::Arc;

use obscura_net::{BlocklistConfig, CookieJar, ObscuraHttpClient, RobotsCache};
use obscura_stealth::load_profile_by_id;

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
        }
    }

    pub fn with_profile(
        id: String,
        profile_id: &str,
        proxy_url: Option<String>,
        stealth: bool,
    ) -> Self {
        let profile = load_profile_by_id(profile_id);
        let mut context = Self::with_options(id, proxy_url, stealth);
        context.user_agent = profile.user_agent;
        context
    }

    pub fn with_proxy(id: String, proxy_url: Option<String>) -> Self {
        Self::with_options(id, proxy_url, false)
    }
}
