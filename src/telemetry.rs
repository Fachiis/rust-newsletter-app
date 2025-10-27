use tracing::dispatcher::set_global_default;
use tracing::Subscriber;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

// Tracing is a framework for instrumenting Rust programs to collect
// structured, event-based diagnostic information. This is like logging, but more advanced.
// It allows you to capture detailed information about the execution of your program,
// including spans (which represent periods of time) and events (which represent discrete occurrences).

/// Compose multiple layers into a `tracing`'s subscriber.
///
/// # Implementation Notes
///
/// We are using `impl Subscriber` as return type to avoid having to
/// spell out the actual type of the returned subscriber, which is
/// indeed quite complex.
/// We need to explicitly call out that the returned subscriber is
/// `Send` and `Sync` to make it possible to pass it to `init_subscriber`
/// later on.
pub fn get_subscriber<Sink>(
    name: String,
    env_filter: String,
    sink: Sink,
) -> impl Subscriber + Sync + Send
where
    Sink: for<'a> MakeWriter<'a> + Send + Sync + Clone + 'static,
{
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::from_env(env_filter));
    let formatting_layer = BunyanFormattingLayer::new(name, sink.clone());

    Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
        .with(
            fmt::layer()
                .with_writer(sink)
                .pretty()
                .with_target(true)
                .with_file(true)
                .with_line_number(true),
        )
}

/// Register a subscriber as global default to process span data.
///
/// It should only be called once!
pub fn init_subscriber(subscriber: impl Subscriber + Sync + Send) {
    LogTracer::init().expect("Failed to set logger");
    set_global_default(subscriber.into()).expect("Failed to set subscriber")
}
