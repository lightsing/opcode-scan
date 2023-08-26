use crate::consts::WS_PROVIDER;
use ethers::prelude::*;
use url::Url;

pub async fn ws_provider() -> Result<Provider<Ws>, ProviderError> {
    Provider::<Ws>::connect(WS_PROVIDER).await
}

pub async fn http_provider(url: &str) -> Provider<impl JsonRpcClient> {
    let base_client = Http::new(url.parse::<Url>().unwrap());
    let retry_client = RetryClientBuilder::default()
        .build(base_client, Box::new(HttpRateLimitRetryPolicy::default()));
    Provider::new(retry_client)
}
