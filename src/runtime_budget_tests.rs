use std::time::Duration;

use serde_json::json;

use super::{
    append_lossy_bounded, cap_service_value, with_deadline, SERVICE_PROGRESS_ITEM_CAP,
    SERVICE_TEXT_FIELD_BYTE_CAP,
};

#[tokio::test]
async fn with_deadline_times_out_pending_future() {
    let result = with_deadline("test op", Duration::from_millis(5), async {
        std::future::pending::<Result<(), &'static str>>().await
    })
    .await;

    let message = result.unwrap_err().to_string();
    assert!(message.contains("test op timed out"), "{message}");
}

#[test]
fn cap_service_value_truncates_large_output_fields_with_metadata() {
    let value = json!({
        "stdout": "x".repeat(SERVICE_TEXT_FIELD_BYTE_CAP + 10),
        "stderr": "small",
    });

    let capped = cap_service_value(value);

    assert_eq!(capped["truncated"], true);
    assert_eq!(
        capped["stdout"].as_str().unwrap().len(),
        SERVICE_TEXT_FIELD_BYTE_CAP
    );
    assert_eq!(capped["truncation"][0]["field"], "stdout");
    assert_eq!(
        capped["truncation"][0]["original_bytes"],
        SERVICE_TEXT_FIELD_BYTE_CAP + 10
    );
}

#[test]
fn cap_service_value_truncates_progress_arrays() {
    let progress = (0..SERVICE_PROGRESS_ITEM_CAP + 5)
        .map(|i| json!({"id": i}))
        .collect::<Vec<_>>();
    let value = json!({ "progress": progress });

    let capped = cap_service_value(value);

    assert_eq!(capped["truncated"], true);
    assert_eq!(
        capped["progress"].as_array().unwrap().len(),
        SERVICE_PROGRESS_ITEM_CAP
    );
    assert_eq!(capped["truncation"][0]["field"], "progress");
}

#[test]
fn append_lossy_bounded_stops_at_cap() {
    let mut target = String::from("abc");
    let truncated = append_lossy_bounded(&mut target, b"defghi", 5);

    assert!(truncated);
    assert_eq!(target, "abcde");
}
