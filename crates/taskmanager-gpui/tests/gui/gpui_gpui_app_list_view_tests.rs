use super::format_action_feedback;

#[test]
fn action_feedback_template_allows_locale_specific_word_order() {
    assert_eq!(
        format_action_feedback(
            "{action} succeeded: {target}",
            "Start",
            "demo.service",
            None,
        ),
        "Start succeeded: demo.service"
    );
    assert_eq!(
        format_action_feedback(
            "对 {target} 执行“{action}”失败：{detail}",
            "启动",
            "demo.service",
            Some("权限不足"),
        ),
        "对 demo.service 执行“启动”失败：权限不足"
    );
}
