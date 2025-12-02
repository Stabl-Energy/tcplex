//! Configuration routing patterns tests

mod common;

use tcplex::config::Config;

#[tokio::test]
async fn test_bidirectional_targets() {
    let config_content = r#"
[server.nodeA]
port = 9001
target = ["nodeB"]

[server.nodeB]
port = 9002
target = ["nodeA"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();

    // Verify bidirectional references
    let node_a = config.server.get("nodeA").unwrap();
    let node_b = config.server.get("nodeB").unwrap();

    assert_eq!(node_a.target, vec!["nodeB"]);
    assert_eq!(node_b.target, vec!["nodeA"]);
}

#[tokio::test]
async fn test_config_with_multiple_targets() {
    let config_content = r#"
[server.hub]
port = 9000
target = ["spoke1", "spoke2", "spoke3"]

[client.spoke1]
addr = "10.0.0.1:8000"
target = ["hub"]

[client.spoke2]
addr = "10.0.0.2:8000"
target = ["hub"]

[client.spoke3]
addr = "10.0.0.3:8000"
target = ["hub"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();

    let hub = config.server.get("hub").unwrap();
    assert_eq!(hub.target.len(), 3);
    assert!(hub.target.contains(&"spoke1".to_string()));
    assert!(hub.target.contains(&"spoke2".to_string()));
    assert!(hub.target.contains(&"spoke3".to_string()));

    for spoke in ["spoke1", "spoke2", "spoke3"] {
        let client = config.client.get(spoke).unwrap();
        assert_eq!(client.target, vec!["hub"]);
    }
}

#[tokio::test]
async fn test_config_circular_reference() {
    let config_content = r#"
[server.node1]
port = 9001
target = ["node2"]

[server.node2]
port = 9002
target = ["node3"]

[server.node3]
port = 9003
target = ["node1"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path());

    // Circular references are allowed (validation only checks existence)
    assert!(config.is_ok());
}

#[tokio::test]
async fn test_mixed_server_client_targets() {
    let config_content = r#"
[server.server1]
port = 9001
target = ["client1", "server2"]

[server.server2]
port = 9002
target = ["client1"]

[client.client1]
addr = "127.0.0.1:9003"
target = ["server1"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();

    let server1 = config.server.get("server1").unwrap();
    assert!(server1.target.contains(&"client1".to_string()));
    assert!(server1.target.contains(&"server2".to_string()));
}
