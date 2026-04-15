use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AcpSessionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcpStopReason {
    EndTurn,
    ToolUse,
    Cancelled,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpAgentInfo {
    pub name:         String,
    pub version:      String,
    pub capabilities: AcpCapabilities,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcpCapabilities {
    pub prompt:  bool,
    pub session: bool,
    pub auth:    bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpPromptResult {
    pub text:          String,
    pub stop_reason:   AcpStopReason,
    pub files_touched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpContentBlock {
    Text { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_roundtrip() {
        let id = AcpSessionId("sess-123".to_string());
        let json = serde_json::to_string(&id).unwrap();
        let decoded: AcpSessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn stop_reason_serialization() {
        let reason = AcpStopReason::EndTurn;
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("EndTurn") || json.contains("end_turn"));

        let reason = AcpStopReason::Error("oops".to_string());
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("oops"));
    }

    #[test]
    fn content_block_text_serialization() {
        let block = AcpContentBlock::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let decoded: AcpContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn prompt_result_default_fields() {
        let result = AcpPromptResult {
            text:          "response".to_string(),
            stop_reason:   AcpStopReason::EndTurn,
            files_touched: vec!["file.rs".to_string()],
        };
        assert_eq!(result.text, "response");
        assert_eq!(result.files_touched.len(), 1);
    }

    #[test]
    fn capabilities_default() {
        let caps = AcpCapabilities::default();
        assert!(!caps.prompt);
        assert!(!caps.session);
        assert!(!caps.auth);
    }
}
