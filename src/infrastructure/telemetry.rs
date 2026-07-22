use tracing_subscriber::EnvFilter;

/// Installs the global tracing subscriber.
///
/// Verbosity comes from `RUST_LOG` (e.g. `RUST_LOG=engos_tool_rs=debug,tower_http=debug`).
/// Set `LOG_FORMAT=json` for machine-readable output in deployed environments.
///
/// This replaces the previous `println!`/`eprintln!` calls, which carried no
/// level, no timestamp and no user context, making production issues
/// effectively untraceable.
pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("engos_tool_rs=info,tower_http=warn,sqlx=warn"));

    let json_output = std::env::var("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true);

    if json_output {
        builder.json().flatten_event(true).init();
    } else {
        builder.init();
    }
}
