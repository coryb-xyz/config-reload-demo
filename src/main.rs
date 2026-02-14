use std::sync::Arc;
use std::time::SystemTime;

use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;

struct Config {
    app_name: String,
    environment: String,
    log_level: String,
    feature_flags: String,
    database_url: String,
    api_key: String,
    hostname: String,
    startup_time: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "index.html")]
struct IndexTemplate {
    app_name: String,
    environment: String,
    log_level: String,
    feature_flags: String,
    database_url: String,
    api_key: String,
    hostname: String,
    startup_time: String,
}

impl From<&Config> for IndexTemplate {
    fn from(c: &Config) -> Self {
        Self {
            app_name: c.app_name.clone(),
            environment: c.environment.clone(),
            log_level: c.log_level.clone(),
            feature_flags: c.feature_flags.clone(),
            database_url: c.database_url.clone(),
            api_key: c.api_key.clone(),
            hostname: c.hostname.clone(),
            startup_time: c.startup_time.clone(),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn format_startup_time() -> String {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Calculate year/month/day from days since epoch
    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days_since_epoch + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

async fn index(State(config): State<Arc<Config>>) -> IndexTemplate {
    IndexTemplate::from(config.as_ref())
}

async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

#[tokio::main]
async fn main() {
    let config = Arc::new(Config {
        app_name: env_or("APP_NAME", "config-reload-demo"),
        environment: env_or("ENVIRONMENT", "local"),
        log_level: env_or("LOG_LEVEL", "info"),
        feature_flags: env_or("FEATURE_FLAGS", ""),
        database_url: env_or("DATABASE_URL", ""),
        api_key: env_or("API_KEY", ""),
        hostname: env_or("HOSTNAME", "unknown"),
        startup_time: format_startup_time(),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .with_state(config);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("failed to bind to 0.0.0.0:8080");
    println!("listening on http://0.0.0.0:8080");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
    println!("shutdown complete");
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
    println!("shutdown signal received, draining connections");
}
