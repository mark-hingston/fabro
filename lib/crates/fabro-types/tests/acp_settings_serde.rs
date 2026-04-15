use std::collections::HashMap;

use fabro_types::settings::run::{
    AcpEntryLayer, AcpServerSettings, AcpTransport, RunAgentSettings,
};

#[test]
fn acp_transport_stdio_roundtrip() {
    let transport = AcpTransport::Stdio {
        command: vec!["npx".into(), "-y".into(), "my-acp-agent".into()],
        env:     HashMap::from([("KEY".into(), "VAL".into())]),
    };
    let json = serde_json::to_string(&transport).unwrap();
    let deserialized: AcpTransport = serde_json::from_str(&json).unwrap();
    assert_eq!(transport, deserialized);
}

#[test]
fn acp_transport_http_roundtrip() {
    let transport = AcpTransport::Http {
        url:     "http://localhost:8080/acp".into(),
        headers: HashMap::from([("Authorization".into(), "Bearer tok".into())]),
    };
    let json = serde_json::to_string(&transport).unwrap();
    let deserialized: AcpTransport = serde_json::from_str(&json).unwrap();
    assert_eq!(transport, deserialized);
}

#[test]
fn acp_transport_sandbox_roundtrip() {
    let transport = AcpTransport::Sandbox {
        command: vec!["sh".into(), "-c".into(), "start-agent".into()],
        port:    9090,
        env:     HashMap::new(),
    };
    let json = serde_json::to_string(&transport).unwrap();
    let deserialized: AcpTransport = serde_json::from_str(&json).unwrap();
    assert_eq!(transport, deserialized);
}

#[test]
fn acp_transport_stdio_json_tag() {
    let transport = AcpTransport::Stdio {
        command: vec!["npx".into()],
        env:     HashMap::new(),
    };
    let json = serde_json::to_string(&transport).unwrap();
    assert!(
        json.contains("\"type\":\"stdio\""),
        "Expected snake_case tag: {json}"
    );
}

#[test]
fn acp_server_settings_default_timeouts() {
    let settings = AcpServerSettings {
        name:                 "test".into(),
        transport:            AcpTransport::Stdio {
            command: vec!["agent".into()],
            env:     HashMap::new(),
        },
        startup_timeout_secs: 10,
        prompt_timeout_secs:  120,
    };
    assert_eq!(settings.startup_timeout_secs, 10);
    assert_eq!(settings.prompt_timeout_secs, 120);
}

#[test]
fn acp_server_settings_duration_helpers() {
    let settings = AcpServerSettings {
        name:                 "test".into(),
        transport:            AcpTransport::Stdio {
            command: vec!["agent".into()],
            env:     HashMap::new(),
        },
        startup_timeout_secs: 15,
        prompt_timeout_secs:  200,
    };
    assert_eq!(
        settings.startup_timeout(),
        std::time::Duration::from_secs(15)
    );
    assert_eq!(
        settings.prompt_timeout(),
        std::time::Duration::from_secs(200)
    );
}

#[test]
fn acp_server_settings_json_roundtrip() {
    let settings = AcpServerSettings {
        name:                 "my-agent".into(),
        transport:            AcpTransport::Http {
            url:     "http://localhost:3000".into(),
            headers: HashMap::new(),
        },
        startup_timeout_secs: 10,
        prompt_timeout_secs:  120,
    };
    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: AcpServerSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(settings, deserialized);
}

#[test]
fn acp_entry_layer_deserialize_toml_stdin() {
    let toml_str = r#"
name = "copilot"
type = "stdio"
command = ["npx", "-y", "@copilot/acp-agent"]

[env]
API_KEY = "secret"
"#;
    let entry: AcpEntryLayer = toml::from_str(toml_str).unwrap();
    assert_eq!(entry.name, "copilot");
    assert!(entry.enabled.is_none());
    assert!(entry.startup_timeout_secs.is_none());
    assert!(entry.prompt_timeout_secs.is_none());
    match entry.transport {
        AcpTransport::Stdio { command, env } => {
            assert_eq!(command, vec!["npx", "-y", "@copilot/acp-agent"]);
            assert_eq!(env.get("API_KEY").unwrap(), "secret");
        }
        _ => panic!("Expected Stdio transport"),
    }
}

#[test]
fn acp_entry_layer_deserialize_toml_http() {
    let toml_str = r#"
name = "remote-agent"
type = "http"
url = "http://localhost:8080/acp"
"#;
    let entry: AcpEntryLayer = toml::from_str(toml_str).unwrap();
    assert_eq!(entry.name, "remote-agent");
    match entry.transport {
        AcpTransport::Http { url, headers } => {
            assert_eq!(url, "http://localhost:8080/acp");
            assert!(headers.is_empty());
        }
        _ => panic!("Expected Http transport"),
    }
}

#[test]
fn acp_entry_layer_deserialize_toml_sandbox() {
    let toml_str = r#"
name = "sandbox-agent"
type = "sandbox"
command = ["sh", "-c", "start"]
port = 9090
"#;
    let entry: AcpEntryLayer = toml::from_str(toml_str).unwrap();
    assert_eq!(entry.name, "sandbox-agent");
    match entry.transport {
        AcpTransport::Sandbox { command, port, env } => {
            assert_eq!(command, vec!["sh", "-c", "start"]);
            assert_eq!(port, 9090);
            assert!(env.is_empty());
        }
        _ => panic!("Expected Sandbox transport"),
    }
}

#[test]
fn acp_entry_layer_with_timeouts() {
    let toml_str = r#"
name = "slow-agent"
type = "stdio"
command = ["agent"]
startup_timeout_secs = 30
prompt_timeout_secs = 300
"#;
    let entry: AcpEntryLayer = toml::from_str(toml_str).unwrap();
    assert_eq!(entry.startup_timeout_secs, Some(30));
    assert_eq!(entry.prompt_timeout_secs, Some(300));
}

#[test]
fn from_acp_entry_layer_to_server_settings() {
    let entry = AcpEntryLayer {
        name:                 "test".into(),
        transport:            AcpTransport::Stdio {
            command: vec!["agent".into()],
            env:     HashMap::new(),
        },
        enabled:              None,
        startup_timeout_secs: Some(20),
        prompt_timeout_secs:  Some(180),
    };
    let settings: AcpServerSettings = entry.into();
    assert_eq!(settings.name, "test");
    assert_eq!(settings.startup_timeout_secs, 20);
    assert_eq!(settings.prompt_timeout_secs, 180);
    assert_eq!(settings.transport, AcpTransport::Stdio {
        command: vec!["agent".into()],
        env:     HashMap::new(),
    });
}

#[test]
fn from_acp_entry_layer_uses_defaults_when_none() {
    let entry = AcpEntryLayer {
        name:                 "test".into(),
        transport:            AcpTransport::Http {
            url:     "http://localhost:3000".into(),
            headers: HashMap::new(),
        },
        enabled:              None,
        startup_timeout_secs: None,
        prompt_timeout_secs:  None,
    };
    let settings: AcpServerSettings = entry.into();
    assert_eq!(settings.startup_timeout_secs, 10);
    assert_eq!(settings.prompt_timeout_secs, 120);
}

#[test]
fn run_agent_settings_has_acps_field() {
    let mut acps = HashMap::new();
    acps.insert("my-agent".into(), AcpServerSettings {
        name:                 "my-agent".into(),
        transport:            AcpTransport::Stdio {
            command: vec!["agent".into()],
            env:     HashMap::new(),
        },
        startup_timeout_secs: 10,
        prompt_timeout_secs:  120,
    });
    let settings = RunAgentSettings {
        permissions: None,
        mcps: HashMap::new(),
        acps,
    };
    assert!(settings.acps.contains_key("my-agent"));
    assert!(settings.mcps.is_empty());
}
