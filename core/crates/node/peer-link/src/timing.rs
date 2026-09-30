//! Peer lifecycle timing belongs to the platform Host, not the Tokio timer driver.
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use std::future::Future;

pub async fn withHostTimeout<T>(
    timeoutMs: u64,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    raceHostDelay(
        operation,
        defaultHostRuntimeTaskSchedulerHost().waitForHostRuntimeDelay(timeoutMs),
    )
    .await
}

async fn raceHostDelay<T>(
    operation: impl Future<Output = Result<T, String>>,
    delay: impl Future<Output = operit_host_api::HostResult<()>>,
) -> Result<T, String> {
    tokio::select! {
        result = operation => result,
        result = delay => match result {
            Ok(()) => Err("Peer operation timed out".into()),
            Err(error) => Err(format!("Host timeout delay failed: {error}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // No time driver or wall-clock waits: the Host controls when the deadline fires.
    #[tokio::test]
    async fn completed_operation_does_not_wait_for_deadline() {
        assert_eq!(
            raceHostDelay(async { Ok(42) }, std::future::pending()).await,
            Ok(42)
        );
    }
    #[tokio::test]
    async fn host_deadline_cancels_pending_operation() {
        struct Dropped(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let operation = async move {
            let _guard = guard;
            std::future::pending::<Result<(), String>>().await
        };
        assert!(raceHostDelay(operation, async { Ok(()) })
            .await
            .unwrap_err()
            .contains("timed out"));
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }
    #[tokio::test]
    async fn host_delay_failure_is_not_reported_as_timeout() {
        let result = raceHostDelay(std::future::pending::<Result<(), String>>(), async {
            Err(operit_host_api::HostError::new("scheduler stopped"))
        })
        .await;
        assert!(result.unwrap_err().contains("scheduler stopped"));
    }
}
