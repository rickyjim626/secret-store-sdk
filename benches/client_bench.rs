use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use secret_store_sdk::{Auth, BatchOp, ClientBuilder, GetOpts, PutOpts};
use serde_json::json;
use std::time::Duration;
use tokio::runtime::Runtime;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

/// Create a mock server with basic endpoints
async fn setup_mock_server() -> MockServer {
    let server = MockServer::start().await;

    // Mock for get_secret
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v2/secrets/[^/]+/[^/]+$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "benchmark",
                    "key": "test-key",
                    "value": "test-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
                .set_delay(Duration::from_millis(10)), // Simulate network latency
        )
        .mount(&server)
        .await;

    // Mock for put_secret
    Mock::given(method("PUT"))
        .and(path_regex(r"^/api/v2/secrets/[^/]+/[^/]+$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "message": "Secret created",
                    "namespace": "benchmark",
                    "key": "test-key",
                    "created_at": "2024-01-01T00:00:00Z",
                    "request_id": "bench-123"
                }))
                .set_delay(Duration::from_millis(15)),
        )
        .mount(&server)
        .await;

    // Mock for batch operations
    Mock::given(method("POST"))
        .and(path_regex(r"^/api/v2/secrets/[^/]+/batch$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "benchmark",
                    "results": {
                        "succeeded": [
                            {"key": "key1", "action": "put", "success": true},
                            {"key": "key2", "action": "put", "success": true}
                        ],
                        "failed": [],
                        "total": 2
                    },
                    "success_rate": 1.0
                }))
                .set_delay(Duration::from_millis(25)),
        )
        .mount(&server)
        .await;

    server
}

fn path_regex(pattern: &str) -> wiremock::matchers::PathRegexMatcher {
    wiremock::matchers::path_regex(pattern)
}

fn bench_get_secret(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let server = rt.block_on(setup_mock_server());

    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("bench-token"))
        .timeout_ms(30000)
        .retries(0) // No retries for benchmarks
        .enable_cache(false) // Test without cache first
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");

    c.bench_function("get_secret_no_cache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get_secret(
                        black_box("benchmark"),
                        black_box("test-key"),
                        black_box(GetOpts::default()),
                    )
                    .await
                    .expect("Failed to get secret");
            });
        });
    });
}

fn bench_get_secret_with_cache(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let server = rt.block_on(setup_mock_server());

    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("bench-token"))
        .timeout_ms(30000)
        .retries(0)
        .enable_cache(true) // Enable caching
        .cache_ttl_secs(300)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");

    // Warm up the cache
    rt.block_on(async {
        let _ = client
            .get_secret("benchmark", "cached-key", GetOpts::default())
            .await;
    });

    c.bench_function("get_secret_with_cache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get_secret(
                        black_box("benchmark"),
                        black_box("cached-key"),
                        black_box(GetOpts::default()),
                    )
                    .await
                    .expect("Failed to get secret");
            });
        });
    });
}

fn bench_put_secret(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let server = rt.block_on(setup_mock_server());

    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("bench-token"))
        .timeout_ms(30000)
        .retries(0)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");

    c.bench_function("put_secret", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .put_secret(
                        black_box("benchmark"),
                        black_box("test-key"),
                        black_box("test-value"),
                        black_box(PutOpts::default()),
                    )
                    .await
                    .expect("Failed to put secret");
            });
        });
    });
}

fn bench_batch_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let server = rt.block_on(setup_mock_server());

    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("bench-token"))
        .timeout_ms(30000)
        .retries(0)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");

    let mut group = c.benchmark_group("batch_operations");

    for size in [10, 50, 100].iter() {
        let ops: Vec<BatchOp> = (0..*size)
            .map(|i| BatchOp::put(format!("key-{}", i), format!("value-{}", i)))
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = client
                        .batch_operate(
                            black_box("benchmark"),
                            black_box(ops.clone()),
                            black_box(false),
                            black_box(None),
                        )
                        .await
                        .expect("Failed to perform batch operation");
                });
            });
        });
    }

    group.finish();
}

fn bench_concurrent_requests(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let server = rt.block_on(setup_mock_server());

    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("bench-token"))
        .timeout_ms(30000)
        .retries(0)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");

    let client = std::sync::Arc::new(client);

    let mut group = c.benchmark_group("concurrent_requests");

    for concurrency in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut tasks = Vec::new();

                        for i in 0..concurrency {
                            let client = client.clone();
                            let task = tokio::spawn(async move {
                                client
                                    .get_secret(
                                        "benchmark",
                                        &format!("key-{}", i),
                                        GetOpts::default(),
                                    )
                                    .await
                                    .expect("Failed to get secret")
                            });
                            tasks.push(task);
                        }

                        // Wait for all tasks to complete
                        for task in tasks {
                            let _ = task.await.expect("Task panicked");
                        }
                    });
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_get_secret,
    bench_get_secret_with_cache,
    bench_put_secret,
    bench_batch_operations,
    bench_concurrent_requests
);
criterion_main!(benches);
