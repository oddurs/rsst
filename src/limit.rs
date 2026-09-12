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

/// Simultaneous fetches allowed to any one host.
///
/// Eight requests spread over eight sites is eight lanes of traffic; eight at
/// one site is a burst at one server, and it is how a feed reader earns a 429.
/// Two is enough to keep a slow server from holding up its own feeds.
pub const DEFAULT_PER_HOST: usize = 2;

/// Permission to fetch: globally, and from any one host.
///
/// The per-host permit is taken first. The other order would have a task hold
/// a global lane while queueing for its host, which wastes the capacity the
/// global limit exists to allocate.
pub struct Gate {
    global: Arc<Semaphore>,
    per_host: usize,
    hosts: std::sync::Mutex<std::collections::HashMap<String, Arc<Semaphore>>>,
}

impl Gate {
    pub fn new(global: usize, per_host: usize) -> Arc<Self> {
        Arc::new(Self {
            global: limiter(global),
            per_host: per_host.max(1),
            hosts: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Runs `work` once both a host lane and a global lane are free.
    pub async fn run<F>(self: &Arc<Self>, url: &str, work: F) -> F::Output
    where
        F: Future,
    {
        let host = self.host_semaphore(host_of(url));
        let _host_permit = host.acquire_owned().await.ok();
        limited(self.global.clone(), work).await
    }

    fn host_semaphore(&self, host: String) -> Arc<Semaphore> {
        let mut hosts = match self.hosts.lock() {
            Ok(hosts) => hosts,
            // A poisoned lock would mean a panic while holding it; running
            // unlimited beats refusing to fetch anything ever again.
            Err(poisoned) => poisoned.into_inner(),
        };
        hosts
            .entry(host)
            .or_insert_with(|| Arc::new(Semaphore::new(self.per_host)))
            .clone()
    }
}

/// The host and port a URL names, which is the server being asked.
///
/// The port is part of it: two ports on one machine are two servers, and the
/// politeness is owed to the server.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.to_ascii_lowercase()
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

    /// Runs `tasks` jobs spread over `hosts` hosts, reporting the highest
    /// number ever running at once overall and at any single host.
    async fn peaks(global: usize, per_host: usize, hosts: usize, tasks: usize) -> (usize, usize) {
        let gate = Gate::new(global, per_host);
        let overall = Arc::new(AtomicUsize::new(0));
        let overall_peak = Arc::new(AtomicUsize::new(0));
        let per: Arc<Vec<AtomicUsize>> =
            Arc::new((0..hosts).map(|_| AtomicUsize::new(0)).collect());
        let per_peak = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for n in 0..tasks {
            let (gate, overall, overall_peak, per, per_peak) = (
                gate.clone(),
                overall.clone(),
                overall_peak.clone(),
                per.clone(),
                per_peak.clone(),
            );
            let host = n % hosts;
            let url = format!("https://host{host}.example/feed.xml");
            handles.push(tokio::spawn(async move {
                gate.run(&url, async {
                    let now = overall.fetch_add(1, Ordering::SeqCst) + 1;
                    overall_peak.fetch_max(now, Ordering::SeqCst);
                    let here = per[host].fetch_add(1, Ordering::SeqCst) + 1;
                    per_peak.fetch_max(here, Ordering::SeqCst);

                    tokio::task::yield_now().await;
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

                    per[host].fetch_sub(1, Ordering::SeqCst);
                    overall.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            }));
        }
        for handle in handles {
            handle.await.expect("task should not panic");
        }
        (
            overall_peak.load(Ordering::SeqCst),
            per_peak.load(Ordering::SeqCst),
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn one_host_never_sees_more_than_its_share() {
        // Fifteen feeds on one site — the case this exists for.
        let (overall, per_host) = peaks(8, 2, 1, 15).await;
        assert!(per_host <= 2, "one host saw {per_host} at once");
        assert!(overall <= 8);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn feeds_on_different_hosts_still_go_at_once() {
        // The per-host limit must not become a global limit by accident.
        let (overall, per_host) = peaks(8, 2, 8, 64).await;
        assert!(per_host <= 2, "one host saw {per_host} at once");
        assert_eq!(overall, 8, "the global capacity went unused");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_global_limit_still_binds_when_hosts_are_plentiful() {
        let (overall, _) = peaks(3, 2, 20, 60).await;
        assert_eq!(overall, 3, "the global limit stopped binding");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_per_host_limit_of_zero_still_makes_progress() {
        let (overall, per_host) = peaks(4, 0, 2, 8).await;
        assert_eq!(per_host, 1, "zero must not deadlock");
        assert!(overall >= 1);
    }

    #[test]
    fn a_host_is_the_server_being_asked() {
        assert_eq!(host_of("https://example.com/feed.xml"), "example.com");
        assert_eq!(host_of("http://example.com/a/b?c=d"), "example.com");
        assert_eq!(host_of("https://EXAMPLE.com/x"), "example.com");
        // Two ports on one machine are two servers.
        assert_eq!(host_of("http://127.0.0.1:8787/a.xml"), "127.0.0.1:8787");
        assert_ne!(
            host_of("http://127.0.0.1:8787/a.xml"),
            host_of("http://127.0.0.1:8799/a.xml")
        );
        // Subdomains are separate servers, and are treated as such.
        assert_ne!(
            host_of("https://blog.example.com/f"),
            host_of("https://www.example.com/f")
        );
    }

    #[test]
    fn the_per_host_limit_is_configurable_and_zero_means_the_default() {
        use crate::config::Config;
        let mut config = Config::default();
        assert_eq!(config.per_host(), DEFAULT_PER_HOST);
        config.max_concurrent_per_host = Some(4);
        assert_eq!(config.per_host(), 4);
        config.max_concurrent_per_host = Some(0);
        assert_eq!(config.per_host(), DEFAULT_PER_HOST);
    }
}
