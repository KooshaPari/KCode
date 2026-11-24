use super::*;
use anyhow::Result;

#[test]
fn session_with_file_activity_display_role_loads_without_losing_messages() -> Result<()> {
    let mut session = Session::create_with_id("file-activity-session".into(), None, None);
    session.add_message_with_display_role(
        Role::User,
        vec![ContentBlock::Text {
            text: "a file changed".into(),
            cache_control: None,
        }],
        Some(StoredDisplayRole::FileActivity),
    );
    session.add_message(
        Role::Assistant,
        vec![ContentBlock::Text {
            text: "I see it".into(),
            cache_control: None,
        }],
    );

    let original = serde_json::to_value(&session)?;
    let mut json = original.clone();
    json["messages"][0]["display_role"] = serde_json::Value::String("file_activity".into());
    json["messages"][1]["display_role"] = serde_json::Value::String("some_future_role".into());

    let restored: Session = serde_json::from_value(json)?;
    assert_eq!(restored.messages.len(), 2);
    assert_eq!(
        restored.messages[0].display_role,
        Some(StoredDisplayRole::FileActivity)
    );
    assert_eq!(
        restored.messages[1].display_role,
        Some(StoredDisplayRole::Other("some_future_role".into()))
    );
    assert_eq!(restored.messages[0].content_preview(), "a file changed");
    assert_eq!(restored.messages[1].content_preview(), "I see it");

    let reserialized = serde_json::to_value(&restored)?;
    assert_eq!(
        reserialized["messages"][0]["display_role"],
        serde_json::Value::String("file_activity".into())
    );
    assert_eq!(
        reserialized["messages"][1]["display_role"],
        serde_json::Value::String("some_future_role".into())
    );
    assert_ne!(
        reserialized["messages"][1]["display_role"],
        serde_json::Value::String("unknown".into())
    );
    Ok(())
}
