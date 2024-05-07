use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use criterion::{Criterion, criterion_main};
use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions, MokaManager};
use hyper::body::Bytes;
use hyper::Request;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde_json::json;
use tailcall::async_graphql_hyper::GraphQLRequest;
use tailcall::blueprint::Upstream;
use tailcall::cache::InMemoryCache;
use tailcall::cli::javascript;
use tailcall::http::showcase::create_app_ctx;
use tailcall::http::{handle_request, Response};
use tailcall::runtime::TargetRuntime;
use tailcall::{blueprint, EnvIO, FileIO, HttpIO};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
struct TestHttp {
    client: ClientWithMiddleware,
}

impl Default for TestHttp {
    fn default() -> Self {
        Self { client: ClientBuilder::new(Client::new()).build() }
    }
}

impl TestHttp {
    fn init(upstream: &Upstream) -> Arc<Self> {
        let mut builder = Client::builder()
            .tcp_keepalive(Some(Duration::from_secs(upstream.tcp_keep_alive)))
            .timeout(Duration::from_secs(upstream.timeout))
            .connect_timeout(Duration::from_secs(upstream.connect_timeout))
            .http2_keep_alive_interval(Some(Duration::from_secs(upstream.keep_alive_interval)))
            .http2_keep_alive_timeout(Duration::from_secs(upstream.keep_alive_timeout))
            .http2_keep_alive_while_idle(upstream.keep_alive_while_idle)
            .pool_idle_timeout(Some(Duration::from_secs(upstream.pool_idle_timeout)))
            .pool_max_idle_per_host(upstream.pool_max_idle_per_host)
            .user_agent(upstream.user_agent.clone());

        // Add Http2 Prior Knowledge
        if upstream.http2_only {
            builder = builder.http2_prior_knowledge();
        }

        // Add Http Proxy
        if let Some(ref proxy) = upstream.proxy {
            builder = builder.proxy(
                reqwest::Proxy::http(proxy.url.clone())
                    .expect("Failed to set proxy in http client"),
            );
        }

        let mut client = ClientBuilder::new(builder.build().expect("Failed to build client"));

        if upstream.http_cache {
            client = client.with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: MokaManager::default(),
                options: HttpCacheOptions::default(),
            }))
        }
        Arc::new(Self { client: client.build() })
    }
}

#[async_trait::async_trait]
impl HttpIO for TestHttp {
    async fn execute(&self, request: reqwest::Request) -> anyhow::Result<Response<Bytes>> {
        let response = self.client.execute(request).await;
        Response::from_reqwest(
            response?
                .error_for_status()
                .map_err(|err| err.without_url())?,
        )
        .await
    }
}

#[derive(Clone)]
struct TestFileIO {}

impl TestFileIO {
    fn init() -> Self {
        TestFileIO {}
    }
}

#[async_trait::async_trait]
impl FileIO for TestFileIO {
    async fn write<'a>(&'a self, path: &'a str, content: &'a [u8]) -> anyhow::Result<()> {
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(content)
            .await
            .map_err(|e| anyhow!("{}", e))?;
        Ok(())
    }

    async fn read<'a>(&'a self, path: &'a str) -> anyhow::Result<String> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .map_err(|e| anyhow!("{}", e))?;
        Ok(String::from_utf8(buffer)?)
    }
}

#[derive(Clone)]
struct TestEnvIO {
    vars: HashMap<String, String>,
}

impl EnvIO for TestEnvIO {
    fn get(&self, key: &str) -> Option<Cow<'_, str>> {
        self.vars.get(key).map(Cow::from)
    }
}

impl TestEnvIO {
    pub fn init() -> Self {
        Self { vars: std::env::vars().collect() }
    }
}

pub fn init(script: Option<blueprint::Script>) -> TargetRuntime {
    let http = if let Some(script) = script.clone() {
        javascript::init_http(TestHttp::init(&Default::default()), script)
    } else {
        TestHttp::init(&Default::default())
    };

    let http2 = if let Some(script) = script {
        javascript::init_http(
            TestHttp::init(&Upstream::default().http2_only(true)),
            script,
        )
    } else {
        TestHttp::init(&Upstream::default().http2_only(true))
    };

    let file = TestFileIO::init();
    let env = TestEnvIO::init();

    TargetRuntime {
        http,
        http2_only: http2,
        env: Arc::new(env),
        file: Arc::new(file),
        cache: Arc::new(InMemoryCache::new()),
        extensions: Arc::new(vec![]),
    }
}
pub fn benchmark_handle_request(c: &mut Criterion) {
    c.bench_function("test_handle_request", |b| {
        let runtime = init(None);

        b.iter(|| {
            let runtime = runtime.clone();

            let req = Request::builder()
                .method("POST")
                .uri("http://upstream/graphql?config=.%2Fci-benchmark%2Fbenchmark.graphql")
                .body(hyper::Body::from(
                    json!({
                        "query": "query { user { name } }"
                    })
                    .to_string(),
                ))
                .unwrap();

            let fut = async move {
                let app = create_app_ctx::<GraphQLRequest>(&req, runtime, true)
                    .await
                    .unwrap()
                    .unwrap();

                let _ = handle_request::<GraphQLRequest>(req, Arc::new(app))
                    .await
                    .unwrap();
            };

            tokio::runtime::Runtime::new().unwrap().block_on(fut);
        })
    });
}
