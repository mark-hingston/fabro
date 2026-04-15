use std::collections::HashMap;
use std::rc::Rc;

use tracing::{error, info};

use crate::client::AcpClient;
use crate::config::AcpServerSettings;
use crate::error::AcpError;
use crate::protocol::{AcpAgentInfo, AcpContentBlock, AcpPromptResult, AcpSessionId};

const ACP_AGENT_NAME_DELIMITER: &str = "__";

#[must_use]
pub fn qualified_agent_name(server: &str, agent: &str) -> String {
    format!(
        "acp{delim}{server}{delim}{agent}",
        delim = ACP_AGENT_NAME_DELIMITER,
        server = sanitize_name(server),
        agent = sanitize_name(agent),
    )
}

#[must_use]
pub fn parse_qualified_name(qualified: &str) -> Option<(String, String)> {
    let rest = qualified.strip_prefix("acp")?;
    let rest = rest.strip_prefix(ACP_AGENT_NAME_DELIMITER)?;
    let idx = rest.find(ACP_AGENT_NAME_DELIMITER)?;
    let server = &rest[..idx];
    let agent = &rest[idx + ACP_AGENT_NAME_DELIMITER.len()..];
    if server.is_empty() || agent.is_empty() {
        return None;
    }
    Some((server.to_string(), agent.to_string()))
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub struct AcpConnectionManager {
    clients: HashMap<String, Rc<AcpClient>>,
    agents:  HashMap<String, Rc<AcpAgentInfo>>,
}

impl AcpConnectionManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            agents:  HashMap::new(),
        }
    }

    pub async fn start_servers(
        &mut self,
        configs: &[AcpServerSettings],
    ) -> Vec<(String, Result<AcpAgentInfo, AcpError>)> {
        let mut results = Vec::new();

        for config in configs {
            match self.start_one_server(config).await {
                Ok(info) => {
                    info!(server = %config.name, agent = %info.name, "ACP server ready");
                    results.push((config.name.clone(), Ok(info)));
                }
                Err(e) => {
                    error!(server = %config.name, error = %e, "ACP server failed to start");
                    results.push((config.name.clone(), Err(e)));
                }
            }
        }

        results
    }

    async fn start_one_server(
        &mut self,
        config: &AcpServerSettings,
    ) -> Result<AcpAgentInfo, AcpError> {
        let client = AcpClient::new(config)?;
        let info = client.initialize(config.startup_timeout()).await?;

        let qualified = qualified_agent_name(&config.name, &info.name);
        self.agents.insert(qualified, Rc::new(info.clone()));
        self.clients.insert(config.name.clone(), Rc::new(client));

        Ok(info)
    }

    pub async fn prompt(
        &self,
        server_name: &str,
        session_id: &AcpSessionId,
        content: Vec<AcpContentBlock>,
    ) -> Result<AcpPromptResult, AcpError> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| AcpError::Agent(format!("no client for ACP server: {server_name}")))?;

        client.prompt(session_id, content).await
    }

    pub async fn close_all(&self) -> Vec<Result<(), AcpError>> {
        let mut results = Vec::new();
        for client in self.clients.values() {
            results.push(client.close().await);
        }
        results
    }

    #[must_use]
    pub fn agent_names(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }
}

impl Default for AcpConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_agent_name_basic() {
        assert_eq!(
            qualified_agent_name("copilot", "codegen"),
            "acp__copilot__codegen"
        );
    }

    #[test]
    fn qualified_agent_name_sanitizes_special_chars() {
        assert_eq!(
            qualified_agent_name("my-agent", "code.gen"),
            "acp__my_agent__code_gen"
        );
    }

    #[test]
    fn qualified_agent_name_preserves_underscores() {
        assert_eq!(
            qualified_agent_name("my_agent", "code_gen"),
            "acp__my_agent__code_gen"
        );
    }

    #[test]
    fn parse_qualified_name_roundtrip() {
        let qualified = qualified_agent_name("copilot", "codegen");
        let (server, agent) = parse_qualified_name(&qualified).unwrap();
        assert_eq!(server, "copilot");
        assert_eq!(agent, "codegen");
    }

    #[test]
    fn parse_qualified_name_with_sanitized_input() {
        let qualified = qualified_agent_name("my-agent", "code.gen");
        let (server, agent) = parse_qualified_name(&qualified).unwrap();
        assert_eq!(server, "my_agent");
        assert_eq!(agent, "code_gen");
    }

    #[test]
    fn parse_qualified_name_invalid_prefix() {
        assert!(parse_qualified_name("not_acp__server__agent").is_none());
    }

    #[test]
    fn parse_qualified_name_missing_delimiter() {
        assert!(parse_qualified_name("acp__serveronly").is_none());
    }

    #[test]
    fn parse_qualified_name_empty_parts() {
        assert!(parse_qualified_name("acp____agent").is_none());
    }

    #[test]
    fn connection_manager_new_has_empty_agents() {
        let mgr = AcpConnectionManager::new();
        assert!(mgr.agent_names().is_empty());
    }
}
