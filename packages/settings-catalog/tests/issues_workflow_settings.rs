use ghostex_settings_catalog::{built_in_extensions, default_value, has_search_row, Platform, J};

#[test]
fn branch_titles_default_off_and_the_clean_up_offer_on() {
    assert_eq!(
        default_value("titleSessionsByBranch"),
        Some(&J::Bool(false))
    );
    assert_eq!(
        default_value("offerCleanupWhenPrMerges"),
        Some(&J::Bool(true))
    );
}

#[test]
fn both_settings_have_a_search_row_on_every_platform() {
    for platform in [Platform::MacOs, Platform::Linux, Platform::Windows] {
        assert!(has_search_row(platform, "titleSessionsByBranch"));
        assert!(has_search_row(platform, "offerCleanupWhenPrMerges"));
    }
}

#[test]
fn issues_workflow_rows_show_with_workspaces_off() {
    let hidden = built_in_extensions::hidden_setting_keys_with(|key| match key {
        "workspacesHidden" => Some(true),
        _ => None,
    });
    for key in [
        "projectLinearApiKey",
        "titleSessionsByBranch",
        "offerCleanupWhenPrMerges",
    ] {
        assert!(!hidden.contains(&key), "{key} hidden with Workspaces off");
    }
    assert!(!has_search_row(Platform::current(), "projectWorkMode"));
}
