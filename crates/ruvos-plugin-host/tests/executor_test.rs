use ruvos_plugin_host::{ExecutionRequest, PluginExecutor};

#[tokio::test]
async fn test_execute_simple_command() {
    let executor = PluginExecutor::new();
    let request = ExecutionRequest {
        plugin_name: "test_plugin".to_string(),
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
    };

    let result = executor.execute(&request).await.expect("execution failed");

    assert_eq!(result.status, 0);
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.stderr, "");
}

#[tokio::test]
async fn test_execute_nonexistent_command() {
    let executor = PluginExecutor::new();
    let request = ExecutionRequest {
        plugin_name: "test_plugin".to_string(),
        command: "nonexistent_command_12345".to_string(),
        args: vec![],
    };

    let result = executor.execute(&request).await;
    assert!(result.is_err(), "expected error for nonexistent command");
}

#[tokio::test]
async fn test_execute_command_with_failure() {
    let executor = PluginExecutor::new();
    let request = ExecutionRequest {
        plugin_name: "test_plugin".to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 1".to_string()],
    };

    let result = executor.execute(&request).await.expect("execution failed");

    assert_eq!(result.status, 1);
}
