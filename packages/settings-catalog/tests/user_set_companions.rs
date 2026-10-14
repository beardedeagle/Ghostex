use ghostex_settings_catalog::{default_value, user_set_companion, Platform, J};

#[test]
fn previous_question_is_off_and_records_who_chose_it() {
    assert!(matches!(
        default_value("chatPreviousQuestion"),
        Some(J::Bool(false))
    ));
    let companion = user_set_companion("chatPreviousQuestion").expect("a companion key");
    assert!(
        matches!(default_value(companion), Some(J::Bool(false))),
        "nobody has chosen it on a fresh install"
    );
    assert!(ghostex_settings_catalog::has_search_row(
        Platform::current(),
        "chatPreviousQuestion"
    ));
    assert!(
        !ghostex_settings_catalog::has_search_row(Platform::current(), companion),
        "the companion is app state, not a setting"
    );
    assert_eq!(user_set_companion("chatCatchMeUp"), None);
}
