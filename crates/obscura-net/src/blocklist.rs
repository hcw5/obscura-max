use std::collections::HashSet;
use std::sync::OnceLock;

const PGL_LIST: &str = include_str!("pgl_domains.txt");

static EXTRA_DOMAINS: &[&str] = &[];

/// Infrastructure domains that are required for core product functionality.
///
/// These are checked before the tracker blocklist unless strict mode is enabled.
static REQUIRED_INFRASTRUCTURE_ALLOWLIST: &[&str] = &["cdn.jsdelivr.net"];

/// Optional domains that can be allowlisted when product policy permits it.
static OPTIONAL_POLICY_ALLOWLIST: &[&str] = &["googletagmanager.com"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlocklistConfig {
    /// Allow required infrastructure domains even when they appear on tracker lists.
    pub allow_required_infrastructure: bool,
    /// Allow optional policy-driven domains (e.g. GTM) when product policy permits it.
    pub allow_optional_policy_domains: bool,
    /// Ignore allowlists and apply strict tracker blocking.
    pub strict_mode: bool,
}

impl Default for BlocklistConfig {
    fn default() -> Self {
        Self {
            allow_required_infrastructure: true,
            allow_optional_policy_domains: false,
            strict_mode: false,
        }
    }
}

fn blocklist() -> &'static HashSet<&'static str> {
    static BLOCKLIST: OnceLock<HashSet<&str>> = OnceLock::new();
    BLOCKLIST.get_or_init(|| {
        let mut set = HashSet::with_capacity(4000);
        for line in PGL_LIST.lines() {
            let domain = line.trim();
            if !domain.is_empty() && !domain.starts_with('#') {
                set.insert(domain);
            }
        }
        for domain in EXTRA_DOMAINS {
            set.insert(*domain);
        }
        set
    })
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn is_allowlisted(host: &str, config: &BlocklistConfig) -> bool {
    if config.allow_required_infrastructure
        && REQUIRED_INFRASTRUCTURE_ALLOWLIST
            .iter()
            .any(|domain| domain_matches(host, domain))
    {
        return true;
    }

    if config.allow_optional_policy_domains
        && OPTIONAL_POLICY_ALLOWLIST
            .iter()
            .any(|domain| domain_matches(host, domain))
    {
        return true;
    }

    false
}

pub fn is_blocked(host: &str) -> bool {
    is_blocked_with_config(host, &BlocklistConfig::default())
}

pub fn is_blocked_with_config(host: &str, config: &BlocklistConfig) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');

    if !config.strict_mode && is_allowlisted(host, config) {
        return false;
    }

    let bl = blocklist();

    if bl.contains(host) {
        return true;
    }

    let mut domain = host;
    while let Some(pos) = domain.find('.') {
        domain = &domain[pos + 1..];
        if bl.contains(domain) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(is_blocked("google-analytics.com"));
        assert!(is_blocked("doubleclick.net"));
    }

    #[test]
    fn test_subdomain_match() {
        assert!(is_blocked("www.google-analytics.com"));
        assert!(is_blocked("ssl.google-analytics.com"));
    }

    #[test]
    fn test_not_blocked() {
        assert!(!is_blocked("google.com"));
        assert!(!is_blocked("example.com"));
        assert!(!is_blocked("github.com"));
    }

    #[test]
    fn test_pgl_domains() {
        assert!(is_blocked("adnxs.com"));
        assert!(is_blocked("criteo.com"));
    }

    #[test]
    fn test_blocklist_size() {
        assert!(blocklist().len() > 3500);
    }

    #[test]
    fn test_required_infrastructure_allowlist_precedes_blocklist() {
        assert!(!is_blocked("cdn.jsdelivr.net"));
        assert!(!is_blocked("assets.cdn.jsdelivr.net"));
    }

    #[test]
    fn test_optional_policy_allowlist_for_gtm() {
        let default_cfg = BlocklistConfig::default();
        assert!(is_blocked_with_config(
            "www.googletagmanager.com",
            &default_cfg
        ));

        let policy_enabled = BlocklistConfig {
            allow_optional_policy_domains: true,
            ..BlocklistConfig::default()
        };
        assert!(!is_blocked_with_config(
            "www.googletagmanager.com",
            &policy_enabled
        ));

        let strict_policy = BlocklistConfig {
            allow_optional_policy_domains: true,
            strict_mode: true,
            ..BlocklistConfig::default()
        };
        assert!(is_blocked_with_config(
            "www.googletagmanager.com",
            &strict_policy
        ));
    }

    #[test]
    fn test_allowlist_subdomain_matching_behavior() {
        assert!(domain_matches("cdn.jsdelivr.net", "cdn.jsdelivr.net"));
        assert!(domain_matches("a.b.cdn.jsdelivr.net", "cdn.jsdelivr.net"));
        assert!(!domain_matches(
            "cdn.jsdelivr.net.evil.com",
            "cdn.jsdelivr.net"
        ));
        assert!(!domain_matches("notcdn.jsdelivr.net", "cdn.jsdelivr.net"));
    }
}
