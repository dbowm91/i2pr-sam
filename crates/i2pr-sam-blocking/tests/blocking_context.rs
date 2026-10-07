use i2pr_sam_blocking::{BlockingClient, BlockingError};

#[tokio::test]
async fn blocking_facade_rejects_nested_runtime_calls() {
    let result = BlockingClient::connect_endpoint("127.0.0.1:1".parse().unwrap());
    assert!(matches!(result, Err(BlockingError::NestedRuntime)));
}
