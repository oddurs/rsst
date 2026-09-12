//! Keeping the number of simultaneous requests to something a network, a set of
//! servers, and a file-descriptor table can all live with.

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Semaphore;

/// Simultaneous fetches allowed when the config does not say.
///
/// Small on purpose. A few hundred feeds all handshaking at once trips rate
/// limits and can exhaust file descriptors, and the wall-clock gain over a
/// modest number of lanes is slight — the time is spent waiting on servers, not
/// on our own concurrency.
pub const DEFAULT_LIMIT: usize = 8;

/// A permit source for at most `limit` concurrent operations.
pub fn limiter(limit: usize) -> Arc<Semaphore> {
    // Zero would deadlock every fetch, so treat it as "no sensible limit given".
    Arc::new(Semaphore::new(limit.max(1)))
}

/// Runs `work` once a permit is free, releasing it afterwards.
pub async fn limited<F>(limiter: Arc<Semaphore>, work: F) -> F::Output
where
    F: Future,
{
    // The semaphore is never closed, so acquiring only fails if it were —
    // running unlimited is still better than dropping the fetch.
    match limiter.acquire_owned().await {
        Ok(permit) => {
            let output = work.await;
            drop(permit);
            output
        }
        Err(_) => work.await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Runs `tasks` concurrent jobs through a limiter and reports the highest
    /// number that were ever running at the same time.
    async fn peak_concurrency(limit: usize, tasks: usize) -> usize {
        let limiter = limiter(limit);
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..tasks {
            let limiter = limiter.clone();
            let in_flight = in_flight.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                limited(limiter, async {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::task::yield_now().await;
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            }));
        }
        for handle in handles {
            handle.await.expect("task should not panic");
        }
        peak.load(Ordering::SeqCst)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn many_feeds_never_exceed_the_limit() {
        // Stands in for the 200-feed config the item is about.
        assert!(peak_concurrency(8, 200).await <= 8);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_limit_is_actually_reached() {
        // Guards against the test passing because nothing ran concurrently.
        assert_eq!(peak_concurrency(4, 50).await, 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_limit_of_one_serialises_everything() {
        assert_eq!(peak_concurrency(1, 20).await, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_limit_of_zero_still_makes_progress() {
        // Misconfiguration must not deadlock the reader.
        assert_eq!(peak_concurrency(0, 5).await, 1);
    }

    #[tokio::test]
    async fn the_permit_is_released_even_when_the_work_panics_is_not_required() {
        // Documents the shape we rely on: output is returned unchanged.
        assert_eq!(limited(limiter(2), async { 41 + 1 }).await, 42);
    }
}
