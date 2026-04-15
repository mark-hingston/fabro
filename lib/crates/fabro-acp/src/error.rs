use thiserror::Error;

#[derive(Debug, Error)]
pub enum AcpError {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Initialization error: {0}")]
    Initialization(String),
    #[error("Session error: {0}")]
    Session(String),
    #[error("Prompt error: {0}")]
    Prompt(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("Process error: {0}")]
    Process(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Agent error: {0}")]
    Agent(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_connection() {
        let err = AcpError::Connection("refused".to_string());
        assert_eq!(format!("{err}"), "Connection error: refused");
    }

    #[test]
    fn error_display_initialization() {
        let err = AcpError::Initialization("timeout".to_string());
        assert_eq!(format!("{err}"), "Initialization error: timeout");
    }

    #[test]
    fn error_display_session() {
        let err = AcpError::Session("not found".to_string());
        assert_eq!(format!("{err}"), "Session error: not found");
    }

    #[test]
    fn error_display_prompt() {
        let err = AcpError::Prompt("rejected".to_string());
        assert_eq!(format!("{err}"), "Prompt error: rejected");
    }

    #[test]
    fn error_display_timeout() {
        let err = AcpError::Timeout("30s".to_string());
        assert_eq!(format!("{err}"), "Timeout error: 30s");
    }

    #[test]
    fn error_display_process() {
        let err = AcpError::Process("killed".to_string());
        assert_eq!(format!("{err}"), "Process error: killed");
    }

    #[test]
    fn error_display_protocol() {
        let err = AcpError::Protocol("version mismatch".to_string());
        assert_eq!(format!("{err}"), "Protocol error: version mismatch");
    }

    #[test]
    fn error_display_agent() {
        let err = AcpError::Agent("refusal".to_string());
        assert_eq!(format!("{err}"), "Agent error: refusal");
    }
}
