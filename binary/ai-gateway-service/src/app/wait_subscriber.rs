/// 共享 Redis Pub/Sub 连接，多 wait 请求复用同一物理连接（设计文档 §连接数）。
struct WaitSubscriberHub {
    client: SubscriberClient,
    waiters: tokio::sync::Mutex<HashMap<String, Vec<oneshot::Sender<()>>>>,
}

impl WaitSubscriberHub {
    async fn new(redis_url: &str) -> Result<Arc<Self>, ServiceError> {
        let client = build_subscriber_client(redis_url).map_err(|e| ServiceError::internal(format!("wait subscriber: {e}")))?;
        client.init().await.map_err(|e| ServiceError::internal(format!("wait subscriber init: {e}")))?;
        let hub = Arc::new(Self {
            client,
            waiters: tokio::sync::Mutex::new(HashMap::new()),
        });
        let reader = hub.clone();
        tokio::spawn(async move {
            reader.run_dispatch_loop().await;
        });
        Ok(hub)
    }

    async fn wait_for_channel(self: &Arc<Self>, channel: &str, timeout: Duration) -> Result<(), ServiceError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut waiters = self.waiters.lock().await;
            waiters.entry(channel.to_string()).or_default().push(tx);
        }
        self.client
            .subscribe(channel)
            .await
            .map_err(|e| ServiceError::internal(format!("pubsub subscribe: {e}")))?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(ServiceError::internal("wait subscriber channel closed")),
            Err(_) => Err(ServiceError::gateway_timeout(format!("timed out waiting for channel {channel}"))),
        }
    }

    async fn run_dispatch_loop(self: Arc<Self>) {
        let mut messages = self.client.message_rx();
        loop {
            let message = match messages.recv().await {
                Ok(message) => message,
                Err(e) => {
                    tracing::warn!(error = %e, "wait subscriber message loop ended");
                    break;
                }
            };
            let channel = message.channel.to_string();
            let mut waiters = self.waiters.lock().await;
            if let Some(list) = waiters.remove(&channel) {
                for tx in list {
                    let _ = tx.send(());
                }
            }
        }
    }
}
