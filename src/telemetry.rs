//! Telemetry: a typed event emitter feeding Sentry through two channels —
//!
//! - **OTLP logs** (the event/log pipeline): events are mapped to OTLP log records
//!   and exported to `SENTRY_OTLP_URL` (the project's `.../integration/otlp`
//!   endpoint), authenticated with the DSN's public key in the `x-sentry-auth`
//!   header. Sentry maps ERROR/FATAL records to issues; the rest land in Log
//!   Explorer.
//! - **Native metrics** via the `sentry` crate: Sentry does not ingest OTLP metrics,
//!   so counters/distributions (download speed, byte counters, cache hits) go
//!   through `sentry::metrics` with the DSN the SDK already uses.
//!
//! [`emit`] is a cheap, non-blocking channel send — call it from the UI thread, the
//! Telegram background thread, or a panic hook. All mapping, batching, retries and
//! HTTP happen on a dedicated `ammini-telemetry` thread so telemetry never delays
//! the app.
//!
//! Credentials resolve in the same order as the Telegram ones (see
//! `telegram/config.rs`): live environment → `.env` → baked into the binary by
//! `build.rs`. If `SENTRY_DSN` or `SENTRY_OTLP_URL` is missing, telemetry disables
//! itself with a single log line and [`emit`] becomes a no-op.

use std::collections::HashMap;
use std::panic::{self, PanicHookInfo};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use opentelemetry::InstrumentationScope;
use opentelemetry::logs::{AnyValue, LogRecord as _, Logger as _, LoggerProvider as _, Severity};
use opentelemetry::{KeyValue, Value};
use opentelemetry_otlp::{
    LogExporter as OtlpLogExporter, WithExportConfig as _, WithHttpConfig as _,
};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::logs::{LogBatch, LogExporter, SdkLogRecord, SdkLogger, SdkLoggerProvider};

const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const FLUSH_MAX_RECORDS: usize = 64;
const RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_EXPORT_ATTEMPTS: u32 = 3;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Where a playback came from — used to attribute playback events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackSource {
    LocalFile,
    Url,
    Telegram,
}

/// Why a Telegram session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOutReason {
    Manual,
    SessionExpired,
}

/// The typed set of "important events" the app emits. Keep this small and curated —
/// per-frame noise (poll reads, property warnings) stays in `tracing` only.
#[derive(Debug, Clone)]
pub enum TelemetryEvent {
    AppStarted {
        version: String,
    },
    TelegramSignedIn,
    TelegramSignedOut {
        reason: SignOutReason,
    },
    TelegramLoginFailed {
        reason: String,
    },
    TelegramVideoPlayed {
        file_name: String,
        size_bytes: Option<u64>,
    },
    PlaybackStarted {
        source: PlaybackSource,
        file_name: String,
    },
    PlaybackFailed {
        source: PlaybackSource,
        file_name: Option<String>,
        error: String,
    },
    Error {
        component: &'static str,
        message: String,
    },
    Panic {
        message: String,
        location: Option<String>,
    },
    /// Metric records — mapped to `sentry::metrics` (Sentry does not ingest OTLP
    /// metrics, so these never become log records).
    Metric(Metric),
}

/// Native-metric records; name/values surface in Sentry's Metrics product.
#[derive(Debug, Clone, Copy)]
pub enum Metric {
    DownloadSpeed { bytes_per_sec: f64 },
    BytesDownloaded { bytes: u64 },
    BlocksDownloaded { count: u64 },
    CacheHits { count: u64 },
    CacheMisses { count: u64 },
    VideosPlayed { count: u64 },
    PlaybackErrors { count: u64 },
}

// ---------------------------------------------------------------------------
// Global emitter
// ---------------------------------------------------------------------------

struct Emitter {
    tx: Sender<TelemetryEvent>,
}

static EMITTER: Mutex<Option<Arc<Emitter>>> = Mutex::new(None);

/// Send an event to the telemetry thread. No-op (and cheap) while telemetry is
/// disabled or shut down.
pub fn emit(event: TelemetryEvent) {
    let emitter = match EMITTER.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return, // poisoned during shutdown — drop the event
    };
    if let Some(emitter) = emitter {
        let _ = emitter.tx.send(event);
    }
}

/// Keep-alive + shutdown handle returned by [`start`]. Dropping it closes the
/// event channel, flushes pending records/metrics and joins the telemetry thread.
pub struct TelemetryGuard {
    handle: Option<JoinHandle<()>>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // Detach the emitter so late `emit` calls become no-ops, then close the
        // channel: the thread drains, flushes and exits.
        if let Ok(mut guard) = EMITTER.lock() {
            *guard = None;
        }
        if let Some(handle) = self.handle.take() {
            let deadline = Instant::now() + SHUTDOWN_JOIN_TIMEOUT;
            while Instant::now() < deadline {
                if handle.is_finished() {
                    let _ = handle.join();
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            tracing::warn!("telemetry: shutdown timed out, dropping thread");
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TelemetryConfig {
    dsn: String,
    otlp_url: String,
}

impl TelemetryConfig {
    /// Same resolution order as the Telegram credentials: live env → `.env`
    /// (already loaded by `dotenvy` at startup) → baked at build time. Empty
    /// values count as missing; missing ⇒ telemetry disabled.
    fn from_env() -> Option<Self> {
        Some(Self {
            dsn: resolve("SENTRY_DSN", option_env!("SENTRY_DSN"))?,
            otlp_url: resolve("SENTRY_OTLP_URL", option_env!("SENTRY_OTLP_URL"))?,
        })
    }
}

/// Resolve one credential: live environment → build-time baked value. An empty
/// live value counts as missing and never falls back, so `VAR=` in the
/// environment deliberately opts out of a baked value.
fn resolve(literal: &str, baked: Option<&'static str>) -> Option<String> {
    std::env::var(literal)
        .ok()
        .or_else(|| baked.map(str::to_owned))
        .filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Start the telemetry thread. Returns `None` when credentials are missing or
/// malformed — callers should treat that as "telemetry disabled" and keep going.
pub fn start() -> Option<TelemetryGuard> {
    let config = TelemetryConfig::from_env()?;
    if dsn_public_key(&config.dsn).is_none() {
        tracing::warn!(
            "telemetry: SENTRY_DSN does not look like a DSN (expected https://<key>@<host>/<project>), disabling"
        );
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("ammini-telemetry".into())
        .spawn(move || telemetry_thread(config, rx))
        .expect("failed to spawn telemetry thread");
    *EMITTER.lock().expect("emitter mutex poisoned at startup") = Some(Arc::new(Emitter { tx }));
    Some(TelemetryGuard {
        handle: Some(thread),
    })
}

/// The public key embedded in a Sentry DSN (`https://<key>@<host>/<project>`).
/// Sentry's OTLP endpoint authenticates with this key, not the whole DSN.
fn dsn_public_key(dsn: &str) -> Option<&str> {
    let rest = dsn.split_once("://")?.1; // after the scheme
    let key = rest.split('@').next()?;
    (!key.is_empty() && !key.contains('/')).then_some(key)
}

/// Install a panic hook that forwards panics to telemetry and then re-raises
/// through the previous hook. Call once at startup, from `main`.
pub fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        emit(TelemetryEvent::Panic {
            message: panic_message(info),
            location: info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column())),
        });
        previous(info);
    }));
}

fn panic_message(info: &PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

// ---------------------------------------------------------------------------
// The telemetry thread
// ---------------------------------------------------------------------------

struct LogPipeline {
    otlp: OtlpLogExporter,
    logger: SdkLogger,
}

fn telemetry_thread(config: TelemetryConfig, rx: Receiver<TelemetryEvent>) {
    let _sentry_guard = init_sentry(&config);

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            tracing::warn!("telemetry: tokio runtime unavailable, disabled: {e}");
            return;
        }
    };
    let pipeline = match build_logger(&config) {
        Ok(pipeline) => pipeline,
        Err(e) => {
            tracing::warn!("telemetry: OTLP exporter unavailable, logs disabled: {e}");
            drain_metrics_only(&runtime, rx);
            return;
        }
    };

    let mut batch: Vec<Box<(SdkLogRecord, InstrumentationScope)>> =
        Vec::with_capacity(FLUSH_MAX_RECORDS);
    let scope = InstrumentationScope::builder("ammini").build();
    let mut last_flush = Instant::now();

    loop {
        let timeout = FLUSH_INTERVAL.saturating_sub(last_flush.elapsed());
        match rx.recv_timeout(timeout) {
            Ok(TelemetryEvent::Metric(metric)) => capture_metric(metric),
            Ok(event) => {
                // Derive video-count counters from the playback lifecycle events.
                match &event {
                    TelemetryEvent::PlaybackStarted { .. } => {
                        capture_metric(Metric::VideosPlayed { count: 1 })
                    }
                    TelemetryEvent::PlaybackFailed { .. } => {
                        capture_metric(Metric::PlaybackErrors { count: 1 })
                    }
                    _ => {}
                }
                if let Some(record) = event.to_record(&pipeline.logger) {
                    batch.push(Box::new((record, scope.clone())));
                    if batch.len() >= FLUSH_MAX_RECORDS {
                        flush_batch(
                            &mut batch,
                            &pipeline.otlp,
                            &mut last_flush,
                            RETRY_DELAY,
                            &runtime,
                        );
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                flush_batch(
                    &mut batch,
                    &pipeline.otlp,
                    &mut last_flush,
                    RETRY_DELAY,
                    &runtime,
                );
            }
            Err(RecvTimeoutError::Disconnected) => {
                flush_batch(
                    &mut batch,
                    &pipeline.otlp,
                    &mut last_flush,
                    RETRY_DELAY,
                    &runtime,
                );
                break;
            }
        }
    }

    if let Err(e) = pipeline.otlp.shutdown() {
        tracing::warn!("telemetry: OTLP shutdown error: {e}");
    }
    flush_sentry();
}

/// Fallback loop when the OTLP exporter failed to build: keep consuming events
/// so metrics still flow and `emit` never blocks.
fn drain_metrics_only(runtime: &tokio::runtime::Runtime, rx: Receiver<TelemetryEvent>) {
    let _ = runtime; // kept for symmetry; the loop is synchronous
    while let Ok(event) = rx.recv() {
        if let TelemetryEvent::Metric(metric) = event {
            capture_metric(metric);
        }
    }
    flush_sentry();
}

/// Drain the sentry SDK's buffered metric envelopes before the process exits.
/// Returns whether the flush reported success (false = no client or network error).
fn flush_sentry() -> bool {
    match sentry::Hub::current().client() {
        Some(client) => client.flush(Some(Duration::from_secs(2))),
        None => false,
    }
}

fn build_logger(
    config: &TelemetryConfig,
) -> Result<LogPipeline, Box<dyn std::error::Error + Send + Sync>> {
    // Sentry's OTLP endpoint requires the full signal path (`/v1/logs`), and the
    // exporter only appends it to bare base URLs — so build the URL explicitly.
    let endpoint = if config.otlp_url.ends_with("/v1/logs") {
        config.otlp_url.clone()
    } else {
        format!("{}/v1/logs", config.otlp_url.trim_end_matches('/'))
    };
    let exporter = OtlpLogExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_headers(HashMap::from([(
            "x-sentry-auth".to_owned(),
            // Sentry's OTLP endpoints expect `sentry sentry_key=<public key>`;
            // the public key is the part of the DSN before the first `@`.
            format!(
                "sentry sentry_key={}",
                dsn_public_key(&config.dsn).unwrap_or_default()
            ),
        )]))
        .build()?;
    // The provider is only used as a record factory (`create_log_record`);
    // records are exported manually via `flush_batch`, so a no-op processor is fine.
    let provider = SdkLoggerProvider::builder()
        .with_log_processor(NoopLogProcessor)
        .build();
    Ok(LogPipeline {
        otlp: exporter,
        logger: provider.logger("ammini"),
    })
}

#[derive(Debug)]
struct NoopLogProcessor;

impl opentelemetry_sdk::logs::LogProcessor for NoopLogProcessor {
    fn emit(&self, _record: &mut SdkLogRecord, _instrumentation: &InstrumentationScope) {}
    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }
    fn shutdown(&self) -> OTelSdkResult {
        Ok(())
    }
}

fn init_sentry(config: &TelemetryConfig) -> sentry::ClientInitGuard {
    let environment = if cfg!(debug_assertions) {
        "development"
    } else {
        "production"
    };
    sentry::init(
        sentry::ClientOptions::new()
            .dsn(config.dsn.as_str())
            .environment(environment)
            .release(format!("ammini@{}", env!("CARGO_PKG_VERSION")))
            .default_integrations(false), // panics/errors go through the OTLP pipeline only
    )
}

// ---------------------------------------------------------------------------
// Event → OTLP log record mapping
// ---------------------------------------------------------------------------

impl TelemetryEvent {
    /// Map this event to an OTLP log record. Returns `None` for metric events
    /// (those go to `sentry::metrics` instead). Runs on the telemetry thread.
    fn to_record(&self, logger: &SdkLogger) -> Option<SdkLogRecord> {
        let mut record = logger.create_log_record();
        record.set_observed_timestamp(SystemTime::now());

        let (event_name, severity, body, attrs): (
            &'static str,
            Severity,
            &'static str,
            Vec<KeyValue>,
        ) = match self {
            TelemetryEvent::Metric(_) => return None,
            TelemetryEvent::AppStarted { version } => (
                "app.started",
                Severity::Info,
                "Ammini started",
                vec![KeyValue::new("version", version.clone())],
            ),
            TelemetryEvent::TelegramSignedIn => (
                "telegram.signed_in",
                Severity::Info,
                "Telegram signed in",
                vec![],
            ),
            TelemetryEvent::TelegramSignedOut { reason } => {
                let reason = match reason {
                    SignOutReason::Manual => "manual",
                    SignOutReason::SessionExpired => "session_expired",
                };
                (
                    "telegram.signed_out",
                    Severity::Info,
                    "Telegram signed out",
                    vec![KeyValue::new("reason", reason)],
                )
            }
            TelemetryEvent::TelegramLoginFailed { reason } => (
                "telegram.login_failed",
                Severity::Warn,
                "Telegram login failed",
                vec![KeyValue::new("reason", reason.clone())],
            ),
            TelemetryEvent::TelegramVideoPlayed {
                file_name,
                size_bytes,
            } => {
                let mut attrs = vec![KeyValue::new("file_name", file_name.clone())];
                if let Some(size) = size_bytes {
                    attrs.push(KeyValue::new("size_bytes", *size as i64));
                }
                (
                    "telegram.video_played",
                    Severity::Info,
                    "Telegram video played",
                    attrs,
                )
            }
            TelemetryEvent::PlaybackStarted { source, file_name } => (
                "playback.started",
                Severity::Info,
                "Playback started",
                vec![
                    KeyValue::new("source", source_name(*source)),
                    KeyValue::new("file_name", file_name.clone()),
                ],
            ),
            TelemetryEvent::PlaybackFailed {
                source,
                file_name,
                error,
            } => {
                let mut attrs = vec![
                    KeyValue::new("source", source_name(*source)),
                    KeyValue::new("error", error.clone()),
                ];
                if let Some(name) = file_name {
                    attrs.push(KeyValue::new("file_name", name.clone()));
                }
                ("playback.failed", Severity::Error, "Playback failed", attrs)
            }
            TelemetryEvent::Error { component, message } => (
                "error",
                Severity::Error,
                "Ammini error",
                vec![
                    KeyValue::new("component", *component),
                    KeyValue::new("error", message.clone()),
                ],
            ),
            TelemetryEvent::Panic { message, location } => {
                let mut attrs = vec![KeyValue::new("error", message.clone())];
                if let Some(loc) = location {
                    attrs.push(KeyValue::new("location", loc.clone()));
                }
                ("panic", Severity::Fatal, "Ammini panic", attrs)
            }
        };

        // `KeyValue.value` is a `Value`; the log record API wants `AnyValue`, and no
        // `From<Value>` exists — map the primitives this emitter produces.
        for kv in attrs {
            let value = match kv.value {
                Value::String(s) => AnyValue::String(s),
                Value::I64(i) => AnyValue::Int(i),
                Value::F64(f) => AnyValue::Double(f),
                Value::Bool(b) => AnyValue::Boolean(b),
                Value::Array(_) => continue, // arrays are never produced here
                _ => continue,               // future/unknown variants: drop the attribute
            };
            record.add_attribute(kv.key, value);
        }
        record.set_event_name(event_name);
        record.set_severity_number(severity);
        record.set_severity_text(severity_text(severity));
        record.set_body(body.into());
        Some(record)
    }
}

fn source_name(source: PlaybackSource) -> &'static str {
    match source {
        PlaybackSource::LocalFile => "local",
        PlaybackSource::Url => "url",
        PlaybackSource::Telegram => "telegram",
    }
}

fn severity_text(severity: Severity) -> &'static str {
    match severity {
        Severity::Fatal => "FATAL",
        Severity::Error => "ERROR",
        Severity::Warn => "WARN",
        _ => "INFO",
    }
}

// ---------------------------------------------------------------------------
// Batching + export
// ---------------------------------------------------------------------------

type OwnedRecord = Box<(SdkLogRecord, InstrumentationScope)>;

/// Flush the accumulated batch to the OTLP exporter, retrying failures with a
/// backoff. The batch is cleared once the exporter accepted it (or gave up).
fn flush_batch<E: LogExporter + ?Sized>(
    batch: &mut Vec<OwnedRecord>,
    exporter: &E,
    last_flush: &mut Instant,
    retry_delay: Duration,
    runtime: &tokio::runtime::Runtime,
) {
    *last_flush = Instant::now();
    if batch.is_empty() {
        return;
    }

    // The exporter holds the batch only for the duration of the request — build
    // a borrowed LogBatch instead of moving the records out.
    let refs: Vec<(&SdkLogRecord, &InstrumentationScope)> =
        batch.iter().map(|b| (&b.0, &b.1)).collect();

    let mut failures = 0u32;
    let result = loop {
        let log_batch = LogBatch::new(&refs);
        let result = runtime.block_on(exporter.export(log_batch));
        if result.is_ok() || failures >= MAX_EXPORT_ATTEMPTS - 1 {
            break result;
        }
        failures += 1;
        let delay = retry_delay * failures;
        tracing::debug!(
            "telemetry: export failed ({e:?}), retrying in {delay:?}",
            e = result
        );
        std::thread::sleep(delay);
    };
    if let Err(e) = result {
        tracing::warn!(
            "telemetry: dropping {n} records after {attempts} failed export attempts: {e}",
            n = batch.len(),
            attempts = failures + 1
        );
    }
    batch.clear();
}

// ---------------------------------------------------------------------------
// Metrics → sentry::metrics
// ---------------------------------------------------------------------------

fn capture_metric(metric: Metric) {
    use sentry::metrics;
    match metric {
        Metric::DownloadSpeed { bytes_per_sec } => {
            metrics::distribution("telegram.download.speed", bytes_per_sec)
                .attribute("unit", "bytes_per_sec")
                .capture();
        }
        Metric::BytesDownloaded { bytes } => {
            metrics::counter("telegram.bytes.downloaded", bytes as f64).capture();
        }
        Metric::BlocksDownloaded { count } => {
            metrics::counter("telegram.blocks.downloaded", count as f64).capture();
        }
        Metric::CacheHits { count } => {
            metrics::counter("cache.hits", count as f64).capture();
        }
        Metric::CacheMisses { count } => {
            metrics::counter("cache.misses", count as f64).capture();
        }
        Metric::VideosPlayed { count } => {
            metrics::counter("videos.played", count as f64).capture();
        }
        Metric::PlaybackErrors { count } => {
            metrics::counter("playback.errors", count as f64).capture();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — no network)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering as AtomicOrdering};

    /// Fake exporter recording batch sizes per export call, with a scripted
    /// number of failures before succeeding.
    #[derive(Debug)]
    struct RecordingExporter {
        exports: Arc<Mutex<Vec<usize>>>,
        fail_first: AtomicU32,
        attempts: AtomicUsize,
    }

    impl LogExporter for RecordingExporter {
        async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
            let count = batch.iter().count();
            self.exports.lock().unwrap().push(count);
            self.attempts.fetch_add(1, AtomicOrdering::SeqCst);
            if self.fail_first.load(AtomicOrdering::SeqCst) > 0 {
                self.fail_first.fetch_sub(1, AtomicOrdering::SeqCst);
                Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                    "boom".into(),
                ))
            } else {
                Ok(())
            }
        }
    }

    fn test_logger() -> SdkLogger {
        let provider = SdkLoggerProvider::builder()
            .with_log_processor(NoopLogProcessor)
            .build();
        provider.logger("test")
    }

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn attr(record: &SdkLogRecord, key: &str) -> Option<opentelemetry::logs::AnyValue> {
        record
            .attributes_iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v.clone())
    }

    #[test]
    fn event_maps_to_log_record_fields() {
        let logger = test_logger();

        let record = TelemetryEvent::TelegramSignedIn
            .to_record(&logger)
            .expect("event should map to a record");
        assert_eq!(record.event_name(), Some("telegram.signed_in"));
        assert_eq!(record.severity_number(), Some(Severity::Info));
        assert_eq!(record.severity_text(), Some("INFO"));

        let record = TelemetryEvent::PlaybackStarted {
            source: PlaybackSource::Telegram,
            file_name: "clip.mp4".into(),
        }
        .to_record(&logger)
        .expect("event should map to a record");
        assert_eq!(record.event_name(), Some("playback.started"));
        assert_eq!(attr(&record, "source"), Some("telegram".into()));
        assert_eq!(
            attr(&record, "file_name"),
            Some("clip.mp4".to_string().into())
        );

        let record = TelemetryEvent::Panic {
            message: "kaboom".into(),
            location: Some("src/main.rs:42:1".into()),
        }
        .to_record(&logger)
        .expect("event should map to a record");
        assert_eq!(record.event_name(), Some("panic"));
        assert_eq!(record.severity_number(), Some(Severity::Fatal));
        assert_eq!(attr(&record, "location"), Some("src/main.rs:42:1".into()));

        let record = TelemetryEvent::TelegramVideoPlayed {
            file_name: "screencast.mkv".into(),
            size_bytes: Some(1_000_000),
        }
        .to_record(&logger)
        .expect("event should map to a record");
        assert_eq!(attr(&record, "size_bytes"), Some(1_000_000i64.into()));
    }

    #[test]
    fn metric_events_do_not_become_records() {
        let logger = test_logger();
        assert!(
            TelemetryEvent::Metric(Metric::DownloadSpeed {
                bytes_per_sec: 12.5
            })
            .to_record(&logger)
            .is_none()
        );
    }

    #[test]
    fn flush_exports_whole_batch_and_clears() {
        let logger = test_logger();
        let _runtime = test_runtime(); // provides the Handle flush_batch blocks on
        let exporter = RecordingExporter {
            exports: Arc::new(Mutex::new(vec![])),
            fail_first: AtomicU32::new(0),
            attempts: AtomicUsize::new(0),
        };
        let mut batch: Vec<OwnedRecord> = (0..3)
            .map(|_| {
                Box::new((
                    TelemetryEvent::TelegramSignedIn.to_record(&logger).unwrap(),
                    InstrumentationScope::builder("test").build(),
                ))
            })
            .collect();
        let mut last_flush = Instant::now();

        flush_batch(
            &mut batch,
            &exporter,
            &mut last_flush,
            Duration::ZERO,
            &_runtime,
        );

        assert!(batch.is_empty(), "batch must be cleared after export");
        assert_eq!(*exporter.exports.lock().unwrap(), vec![3]);

        // Empty batch → exporter must not be called again.
        flush_batch(
            &mut batch,
            &exporter,
            &mut last_flush,
            Duration::ZERO,
            &_runtime,
        );
        assert_eq!(exporter.attempts.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn flush_retries_then_succeeds() {
        let logger = test_logger();
        let _runtime = test_runtime();
        let exporter = RecordingExporter {
            exports: Arc::new(Mutex::new(vec![])),
            fail_first: AtomicU32::new(2),
            attempts: AtomicUsize::new(0),
        };
        let mut batch: Vec<OwnedRecord> = vec![Box::new((
            TelemetryEvent::Error {
                component: "test",
                message: "nope".into(),
            }
            .to_record(&logger)
            .unwrap(),
            InstrumentationScope::builder("test").build(),
        ))];
        let mut last_flush = Instant::now();

        flush_batch(
            &mut batch,
            &exporter,
            &mut last_flush,
            Duration::ZERO,
            &_runtime,
        );

        assert!(batch.is_empty());
        assert_eq!(exporter.attempts.load(AtomicOrdering::SeqCst), 3);
        assert_eq!(*exporter.exports.lock().unwrap(), vec![1, 1, 1]);
    }

    #[test]
    fn flush_gives_up_after_max_attempts() {
        let logger = test_logger();
        let _runtime = test_runtime();
        let exporter = RecordingExporter {
            exports: Arc::new(Mutex::new(vec![])),
            fail_first: AtomicU32::new(u32::MAX), // always fail
            attempts: AtomicUsize::new(0),
        };
        let mut batch: Vec<OwnedRecord> = vec![Box::new((
            TelemetryEvent::AppStarted {
                version: "0.1.0".into(),
            }
            .to_record(&logger)
            .unwrap(),
            InstrumentationScope::builder("test").build(),
        ))];
        let mut last_flush = Instant::now();

        flush_batch(
            &mut batch,
            &exporter,
            &mut last_flush,
            Duration::ZERO,
            &_runtime,
        );

        assert!(batch.is_empty(), "records are dropped after giving up");
        assert_eq!(
            exporter.attempts.load(AtomicOrdering::SeqCst),
            MAX_EXPORT_ATTEMPTS as usize
        );
    }

    /// Serializes env-mutating tests (env is process-global).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Live delivery check against the real Sentry project: sends one OTLP log
    /// record and one metric through the actual exporters, using the `.env`
    /// credentials. Run deliberately, it is NOT part of CI:
    /// `cargo test --lib live_smoke_test -- --ignored`
    #[test]
    #[ignore = "requires real Sentry credentials in .env and network access"]
    fn live_smoke_test() {
        let Some(config) = TelemetryConfig::from_env() else {
            eprintln!("live_smoke_test: skipping, SENTRY_DSN / SENTRY_OTLP_URL not configured");
            return;
        };
        let _sentry = init_sentry(&config);
        let runtime = test_runtime();
        let pipeline = build_logger(&config).expect("OTLP exporter should build");
        let scope = InstrumentationScope::builder("ammini").build();
        let record = TelemetryEvent::AppStarted {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
        .to_record(&pipeline.logger)
        .expect("event should map to a record");
        let batch = vec![(&record, &scope)];
        let result = runtime.block_on(pipeline.otlp.export(LogBatch::new(&batch)));
        assert!(
            result.is_ok(),
            "OTLP export to Sentry failed (is the DSN/endpoint right?): {result:?}"
        );
        let _ = pipeline.otlp.shutdown();
        capture_metric(Metric::CacheHits { count: 1 });
        assert!(
            flush_sentry(),
            "sentry metrics flush reported failure (network/auth?)"
        );
        eprintln!("live_smoke_test: OTLP record + metric accepted — check the Sentry project");
    }

    #[test]
    fn dsn_public_key_parses_standard_dsn() {
        assert_eq!(
            dsn_public_key(
                "https://a3f344a9c15e807bb0b9d5bbaa465212@o907492.ingest.us.sentry.io/4512136920236032"
            ),
            Some("a3f344a9c15e807bb0b9d5bbaa465212")
        );
        assert_eq!(dsn_public_key("http://key@host/1"), Some("key"));
        // Malformed DSNs yield no key (telemetry then disables itself).
        assert_eq!(dsn_public_key("not-a-dsn"), None);
        assert_eq!(dsn_public_key("https://@host/1"), None);
        assert_eq!(dsn_public_key(""), None);
    }

    #[test]
    fn config_resolution_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Env vars always win, so these asserts are independent of the local
        // `.env` file (which carries the real Sentry credentials on dev machines).
        unsafe {
            std::env::set_var("SENTRY_DSN", "from-env");
            std::env::set_var("SENTRY_OTLP_URL", "https://from-env/otlp");
        }
        let config = TelemetryConfig::from_env().expect("both vars set");
        assert_eq!(config.dsn, "from-env");
        assert_eq!(config.otlp_url, "https://from-env/otlp");

        // The resolver itself: env beats baked; an explicitly empty env var opts
        // out — it must NOT fall back to the baked value.
        unsafe {
            std::env::set_var("SENTRY_DSN", "");
            std::env::set_var("SENTRY_OTLP_URL", "");
        }
        assert_eq!(resolve("SENTRY_DSN", Some("baked")), None);
        assert_eq!(resolve("SENTRY_OTLP_URL", None), None);
        unsafe {
            std::env::set_var("SENTRY_DSN", "from-env");
        }
        assert_eq!(
            resolve("SENTRY_DSN", Some("baked")),
            Some("from-env".to_owned())
        );
        unsafe {
            std::env::remove_var("SENTRY_DSN");
            std::env::remove_var("SENTRY_OTLP_URL");
        }
        assert_eq!(
            resolve("SENTRY_DSN", Some("baked")),
            Some("baked".to_owned())
        );
        assert_eq!(resolve("SENTRY_DSN", None), None);
    }
}
