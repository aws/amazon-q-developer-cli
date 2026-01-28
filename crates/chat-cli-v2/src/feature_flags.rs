/// Centralized feature flags for quick enable/disable of features
pub struct FeatureFlags;

impl FeatureFlags {
    /// Code intelligence - LSP-based code analysis and navigation
    pub const CODE_INTELLIGENCE_ENABLED: bool = true;
    /// Regions where web_search is disabled
    pub const WEB_SEARCH_BLOCKED_REGIONS: &'static [&'static str] = &[];
    /// Web search and fetch tools - global toggle
    pub const WEB_SEARCH_ENABLED: bool = true;

    /// Check if web_search is enabled for a specific region
    pub fn is_web_search_enabled_for_region(region: &str) -> bool {
        Self::WEB_SEARCH_ENABLED && !Self::WEB_SEARCH_BLOCKED_REGIONS.contains(&region)
    }
}
