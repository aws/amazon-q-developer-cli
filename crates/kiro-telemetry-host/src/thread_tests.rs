use std::io::Write;
use std::net::TcpListener;
use std::sync::mpsc as std_mpsc;
use std::time::Instant;

use kiro_telemetry::testing::{
    OtlpTestCollector,
    expect_otlp_metric,
};
use kiro_telemetry::{
    OtelMode,
    TelemetryConfig,
};

use super::*;
use crate::config::govcloud_partition;

fn start_one_shot_collector() -> (String, std_mpsc::Receiver<()>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (received_tx, received_rx) = std_mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        received_tx.send(()).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();
    });
    (endpoint, received_rx, server)
}

#[tokio::test]
async fn default_host_config_runs_without_sinks() {
    let thread = TelemetryThread::new(HostConfig::default()).await.unwrap();
    thread.send_user_logged_in().unwrap();
    thread.finish().await.unwrap();
}

#[derive(Debug)]
struct IdentityTestTranslator;

impl OtelEventTranslator for IdentityTestTranslator {
    fn metric_records(&self, _event: &Event) -> Vec<kiro_telemetry::MetricRecord> {
        vec![metric::record_model_invocation(metric::Engine::V2, None)]
    }
}

#[tokio::test]
async fn enqueue_epoch_is_an_identity_boundary() {
    use kiro_telemetry::testing::{
        OtlpTestCollector,
        expect_otlp_metric,
        otlp_metric_attribute_values,
    };

    let collector = OtlpTestCollector::start(1);
    let state = tempfile::tempdir().unwrap();
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(
            true,
            OtelMode::OtelOnly,
            Some(collector.endpoint()),
            state.path().to_path_buf(),
        ),
        otel_translator: Some(Arc::new(IdentityTestTranslator)),
        ..HostConfig::default()
    };
    let thread = TelemetryThread::new(config).await.unwrap();
    let epochs = thread.identity_epochs();
    thread.send_user_logged_in().unwrap();
    let observed = epochs.current();
    epochs.identify::<()>(observed, "late-user-id", |_| Ok(())).unwrap();
    thread.send_user_logged_in().unwrap();
    thread.finish().await.unwrap();

    let requests = collector.collect();
    let anonymous = metric::record_model_invocation(metric::Engine::V2, None);
    let identified = anonymous
        .clone()
        .with_attribute("user_id", kiro_telemetry::pseudonymous_user_id("late-user-id"));
    expect_otlp_metric(&requests, &identified);
    let user_ids = otlp_metric_attribute_values(&requests, &anonymous.name, "user_id");
    assert_eq!(user_ids.len(), 2);
    assert_eq!(user_ids.iter().filter(|user_id| user_id.is_none()).count(), 1);
    assert_eq!(
        user_ids
            .iter()
            .filter(|user_id| {
                user_id.as_deref() == Some(kiro_telemetry::pseudonymous_user_id("late-user-id").as_str())
            })
            .count(),
        1,
        "enqueue-time epochs must separate queued anonymous and identified events",
    );
}

#[derive(Debug)]
struct BlockingLegacySink {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

impl crate::config::LegacySink for BlockingLegacySink {
    fn send_event(&self, _event: Event) -> futures::future::BoxFuture<'_, ()> {
        Box::pin(async move {
            self.started.notify_one();
            self.release.notified().await;
        })
    }

    fn send_event_govcloud(&self, event: Event, _partition: &'static str) -> futures::future::BoxFuture<'_, ()> {
        self.send_event(event)
    }
}

#[tokio::test]
async fn identity_transition_does_not_wait_for_blocked_legacy_delivery() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let config = HostConfig {
        legacy_sink: Some(Arc::new(BlockingLegacySink {
            started: started.clone(),
            release: release.clone(),
        })),
        ..HostConfig::default()
    };
    let thread = TelemetryThread::new(config).await.unwrap();
    thread.send_user_logged_in().unwrap();
    tokio::time::timeout(Duration::from_secs(1), started.notified())
        .await
        .expect("legacy sink should receive the event");

    let epochs = thread.identity_epochs();
    let observed = epochs.current();
    assert_eq!(
        epochs.identify::<()>(observed, "late-user-id", |_| Ok(())),
        Ok(kiro_telemetry::IdentifyOutcome::Identified),
        "identity transition must not wait for legacy network I/O",
    );

    release.notify_one();
    thread.finish().await.unwrap();
}

#[tokio::test]
async fn cloned_thread_can_finish_before_original() {
    let thread = TelemetryThread::new(HostConfig::default()).await.unwrap();
    let clone = thread.clone();
    clone.finish().await.unwrap();
    thread.finish().await.unwrap();
}

#[tokio::test]
async fn startup_spawn_failure_retries_final_export() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let (endpoint, received_rx, server) = start_one_shot_collector();
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
        process_identity: Some(identity),
        ..HostConfig::default()
    };
    let spawn_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let thread = TelemetryThread::new_with_flush_worker(config, {
        let spawn_count = Arc::clone(&spawn_count);
        move || {
            spawn_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(std::io::Error::from_raw_os_error(libc::EAGAIN))
        }
    })
    .await
    .unwrap();

    thread.finish().await.unwrap();
    received_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    server.join().unwrap();
    assert_eq!(spawn_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn telemetry_thread_reuses_startup_worker_for_final_export() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let (endpoint, received_rx, server) = start_one_shot_collector();
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
        process_identity: Some(identity),
        ..HostConfig::default()
    };
    let spawn_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let thread = TelemetryThread::new_with_flush_worker(config, {
        let spawn_count = Arc::clone(&spawn_count);
        move || {
            FlushWorker::spawn_with(move |worker| {
                spawn_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::Builder::new()
                    .name("kiro-telemetry-flush-test".to_string())
                    .spawn(worker)
                    .map(drop)
            })
        }
    })
    .await
    .unwrap();

    thread.finish().await.unwrap();
    received_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    server.join().unwrap();
    assert_eq!(spawn_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dead_flush_worker_keeps_shutdown_bounded_and_run_clean() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let (endpoint, received_rx, server) = start_one_shot_collector();
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
        process_identity: Some(identity),
        ..HostConfig::default()
    };
    let thread = TelemetryThread::new_with_flush_worker(config, || {
        FlushWorker::spawn_with(|worker| {
            drop(worker);
            Ok(())
        })
    })
    .await
    .unwrap();

    tokio::time::timeout(Duration::from_secs(1), thread.finish())
        .await
        .unwrap()
        .unwrap();
    received_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    server.join().unwrap();
    assert_eq!(RunReceiptStore::new(state.path()).recover().records().count(), 0);
}

#[tokio::test]
async fn noop_fallback_preserves_recovery_receipt_for_later_launch() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let store = RunReceiptStore::new(state.path());
    drop(store.start(identity).unwrap());

    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(
            true,
            OtelMode::OtelOnly,
            Some("not a valid endpoint".to_string()),
            state.path().to_path_buf(),
        ),
        ..HostConfig::default()
    };
    let thread = TelemetryThread::new(config).await.unwrap();

    thread.finish().await.unwrap();
    assert_eq!(store.recover().records().count(), 1);
}

#[tokio::test]
async fn failed_accepted_flush_retries_and_acknowledges_only_success() {
    let flushes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let acknowledgements = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker = FlushWorker::spawn_with(|worker| {
        std::thread::Builder::new()
            .name("kiro-telemetry-flush-test".to_string())
            .spawn(worker)
            .map(drop)
    })
    .unwrap();

    let first = request_flush(
        &worker,
        {
            let flushes = Arc::clone(&flushes);
            move || {
                flushes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                false
            }
        },
        {
            let acknowledgements = Arc::clone(&acknowledgements);
            move || {
                acknowledgements.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        },
    )
    .await;
    let second = request_flush(
        &worker,
        {
            let flushes = Arc::clone(&flushes);
            move || {
                flushes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                true
            }
        },
        {
            let acknowledgements = Arc::clone(&acknowledgements);
            move || {
                acknowledgements.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        },
    )
    .await;

    assert_eq!(first, FlushOutcome::Failed);
    assert_eq!(second, FlushOutcome::Succeeded);
    assert_eq!(flushes.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(acknowledgements.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_recovery_flush_reemits_record_before_acknowledging() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let store = RunReceiptStore::new(state.path());
    drop(store.start(identity).unwrap());
    let collector = OtlpTestCollector::start_with_statuses(vec![400, 200]);
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(
            true,
            OtelMode::OtelOnly,
            Some(collector.endpoint()),
            state.path().to_path_buf(),
        ),
        ..HostConfig::default()
    };
    let thread = TelemetryThread::new(config).await.unwrap();
    let expected = metric::record_crash_for_version(
        env!("CARGO_PKG_VERSION"),
        metric::Engine::V2,
        metric::OsType::from_name(std::env::consts::OS),
        metric::ProcessRole::Host,
        metric::CrashKind::UncleanExit,
    );
    let failed_request = collector.receive_timeout(Duration::from_secs(5));
    expect_otlp_metric(std::slice::from_ref(&failed_request), &expected);

    thread.finish_with_timeout(Duration::from_secs(5)).await.unwrap();
    let successful_request = collector.receive_timeout(Duration::from_secs(5));
    expect_otlp_metric(std::slice::from_ref(&successful_request), &expected);
    assert_eq!(store.recover().records().count(), 0);
}

#[tokio::test]
async fn unavailable_startup_worker_exports_recovery_once_at_shutdown() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let store = RunReceiptStore::new(state.path());
    drop(store.start(identity).unwrap());
    let collector = OtlpTestCollector::start(1);
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(
            true,
            OtelMode::OtelOnly,
            Some(collector.endpoint()),
            state.path().to_path_buf(),
        ),
        ..HostConfig::default()
    };
    let thread =
        TelemetryThread::new_with_flush_worker(config, || Err(std::io::Error::from_raw_os_error(libc::EAGAIN)))
            .await
            .unwrap();

    thread.finish_with_timeout(Duration::from_secs(5)).await.unwrap();
    let requests = collector.collect();
    let expected = metric::record_crash_for_version(
        env!("CARGO_PKG_VERSION"),
        metric::Engine::V2,
        metric::OsType::from_name(std::env::consts::OS),
        metric::ProcessRole::Host,
        metric::CrashKind::UncleanExit,
    );
    expect_otlp_metric(&requests, &expected);
    assert_eq!(store.recover().records().count(), 0);
}

#[tokio::test]
async fn abandoned_flush_does_not_acknowledge() {
    let acknowledgements = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker = FlushWorker::spawn_with(|worker| {
        std::thread::Builder::new()
            .name("kiro-telemetry-flush-test".to_string())
            .spawn(worker)
            .map(drop)
    })
    .unwrap();

    let outcome = request_flush(&worker, || panic!("simulated abandoned flush"), {
        let acknowledgements = Arc::clone(&acknowledgements);
        move || {
            acknowledgements.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    })
    .await;

    assert_eq!(outcome, FlushOutcome::Abandoned);
    assert_eq!(acknowledgements.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn blocked_recovery_flush_does_not_hold_runtime_shutdown() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let store = RunReceiptStore::new(state.path());
    drop(store.start(identity).unwrap());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (accepted_tx, accepted_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        accepted_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();
    });
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
        process_identity: Some(identity),
        ..HostConfig::default()
    };
    let (finished_tx, finished_rx) = std_mpsc::channel();
    let runtime_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(async move {
            let thread = TelemetryThread::new(config).await.unwrap();
            thread.finish_with_timeout(Duration::from_millis(100)).await
        });
        drop(runtime);
        finished_tx.send(result).unwrap();
    });

    accepted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let shutdown_result = finished_rx.recv_timeout(Duration::from_secs(5));
    release_tx.send(()).unwrap();
    server.join().unwrap();
    runtime_thread.join().unwrap();

    match shutdown_result {
        Ok(result) => assert!(result.is_ok()),
        Err(err) => panic!("blocked recovery flush held runtime shutdown: {err}"),
    }

    let receipt_directory = state.path().join("run-receipts");
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline
        && std::fs::read_dir(&receipt_directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|extension| extension == "json"))
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        std::fs::read_dir(receipt_directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .all(|entry| entry.path().extension().is_none_or(|extension| extension != "json"))
    );
}

#[test]
fn blocked_final_flush_does_not_hold_runtime_shutdown() {
    let state = tempfile::tempdir().unwrap();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (accepted_tx, accepted_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        accepted_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();
    });
    let config = HostConfig {
        telemetry_enabled: true,
        otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
        process_identity: Some(identity),
        ..HostConfig::default()
    };
    let (finished_tx, finished_rx) = std_mpsc::channel();
    let runtime_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(async move {
            let thread = TelemetryThread::new(config).await.unwrap();
            thread.finish_with_timeout(Duration::from_millis(100)).await
        });
        drop(runtime);
        finished_tx.send(result).unwrap();
    });

    accepted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let shutdown_result = finished_rx.recv_timeout(Duration::from_secs(5));
    release_tx.send(()).unwrap();
    server.join().unwrap();
    runtime_thread.join().unwrap();

    match shutdown_result {
        Ok(result) => assert!(result.is_ok()),
        Err(err) => panic!("blocked final flush held runtime shutdown: {err}"),
    }
}

#[tokio::test]
async fn join_failure_still_completes_run_receipt() {
    let state = tempfile::tempdir().unwrap();
    let store = RunReceiptStore::new(state.path());
    let receipt = store
        .start(crate::process::ProcessIdentity::new(
            metric::Engine::V2,
            metric::ProcessRole::Host,
        ))
        .unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
    let thread = TelemetryThread {
        handle: Some(tokio::spawn(async { panic!("test worker failure") })),
        legacy_handle: Some(tokio::spawn(std::future::pending())),
        tx: TelemetrySender::strong(tx),
        identity_epochs: Arc::new(IdentityEpochs::from_persisted(None)),
        process_identity: Arc::new(Mutex::new(Some(identity))),
        run_receipt_store: Some(store.clone()),
        run_receipt: Some(Arc::new(Mutex::new(Some(receipt)))),
    };

    let result = thread.finish().await;

    assert!(matches!(result, Err(TelemetryError::Join(_))));
    assert_eq!(store.recover().records().count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finish_timeout_is_global_for_non_cooperative_workers() {
    let (started_tx, started_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let handle = tokio::task::spawn_blocking(move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let thread = TelemetryThread {
        handle: Some(handle),
        legacy_handle: None,
        tx: TelemetrySender::strong(tx),
        identity_epochs: Arc::new(IdentityEpochs::from_persisted(None)),
        process_identity: Arc::new(Mutex::new(None)),
        run_receipt_store: None,
        run_receipt: Some(Arc::new(Mutex::new(None))),
    };

    let timeout = Duration::from_millis(200);
    let started = Instant::now();
    thread.finish_with_timeout(timeout).await.unwrap();
    let elapsed = started.elapsed();
    release_tx.send(()).unwrap();

    assert!(
        elapsed < timeout + Duration::from_millis(75),
        "finish exceeded its global timeout: {elapsed:?}",
    );
}

#[tokio::test(start_paused = true)]
async fn post_abort_budget_observes_non_cancelled_worker_failure() {
    let (started_tx, started_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let handle = tokio::task::spawn_blocking(move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        panic!("test worker failure after abort request");
    });
    let completion = handle.abort_handle();
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let thread = TelemetryThread {
        handle: Some(handle),
        legacy_handle: None,
        tx: TelemetrySender::strong(tx),
        identity_epochs: Arc::new(IdentityEpochs::from_persisted(None)),
        process_identity: Arc::new(Mutex::new(None)),
        run_receipt_store: None,
        run_receipt: Some(Arc::new(Mutex::new(None))),
    };

    let finish = thread.finish_with_timeout(Duration::from_millis(200));
    tokio::pin!(finish);
    assert!(futures::poll!(&mut finish).is_pending());
    tokio::time::advance(Duration::from_millis(110)).await;
    assert!(futures::poll!(&mut finish).is_pending());
    release_tx.send(()).unwrap();
    while !completion.is_finished() {
        tokio::task::yield_now().await;
    }
    let result = finish.await;

    assert!(matches!(result, Err(TelemetryError::Join(err)) if err.is_panic()));
}

#[test]
fn host_defaults_do_not_override_event_attribution() {
    let mut event = Event::new(EventType::UserLoggedIn {});
    event.set_engine(metric::Engine::V3);
    event.set_client_application_kind(metric::ClientApplication::ChatCliV3);

    apply_event_defaults(
        &mut event,
        Some(metric::Engine::V2),
        Some(metric::ClientApplication::ChatCliV2),
    );

    assert_eq!(event.engine, Some(metric::Engine::V3));
    assert_eq!(event.client_application.as_deref(), Some("chat_cli_v3"));
}

#[test]
fn host_defaults_fill_missing_event_attribution() {
    let mut event = Event::new(EventType::UserLoggedIn {});

    apply_event_defaults(
        &mut event,
        Some(metric::Engine::V2),
        Some(metric::ClientApplication::ChatCliV2),
    );

    assert_eq!(event.engine, Some(metric::Engine::V2));
    assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
}

#[test]
fn govcloud_partition_detects_gov_regions() {
    assert_eq!(govcloud_partition("us-gov-east-1"), Some("aws-us-gov"));
    assert_eq!(govcloud_partition("us-gov-west-1"), Some("aws-us-gov"));
    assert_eq!(govcloud_partition("us-east-1"), None);
}
