use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use config::manager::ManagerConfig;
use manager::{slot, Manager};
use opentelemetry::{trace::TracerProvider as _, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{logs::LoggerProvider, metrics::SdkMeterProvider, trace::TracerProvider};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser)]
#[command(author, version, about = "Actions runner manager")]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the manager: reconcile LVM, spawn slot processes
    Run(RunArgs),

    /// Run a single slot loop (spawned by the manager)
    Slot(SlotArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    #[arg(short, long)]
    config: Utf8PathBuf,
}

#[derive(Parser, Debug)]
struct SlotArgs {
    #[arg(short, long)]
    config: Utf8PathBuf,

    #[arg(long)]
    role: String,

    #[arg(long)]
    idx: usize,
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    let _otel_guard = init_tracing()?;

    let args = Args::parse();
    match args.command {
        Commands::Run(args) => {
            let config = ManagerConfig::from_file(&args.config)?;
            Manager::new(config).run()
        }
        Commands::Slot(args) => {
            let config = ManagerConfig::from_file(&args.config)?;
            let role = config
                .roles
                .iter()
                .find(|r| r.name == args.role || r.slug() == args.role)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("role '{}' not found in config", args.role))?;
            slot::run_slot(std::sync::Arc::new(config), &role, args.idx)
        }
    }
}

struct OtelGuard {
    tracer_provider: TracerProvider,
    meter_provider: SdkMeterProvider,
    logger_provider: LoggerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        let _ = self.tracer_provider.shutdown();
        let _ = self.meter_provider.shutdown();
        let _ = self.logger_provider.shutdown();
    }
}

fn init_tracing() -> Result<Option<OtelGuard>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true);

    if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        let service_name =
            std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "actions-runner".to_string());
        let environment =
            std::env::var("APPSIGNAL_APP_ENV").unwrap_or_else(|_| "production".to_string());

        let mut attributes = vec![
            KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                service_name.clone(),
            ),
            KeyValue::new("appsignal.config.revision", env!("CARGO_PKG_VERSION")),
            KeyValue::new("host.name", util::hostname()),
            KeyValue::new("appsignal.config.language_integration", "rust"),
            KeyValue::new("appsignal.config.name", service_name),
            KeyValue::new("appsignal.config.environment", environment),
        ];

        if let Ok(api_key) = std::env::var("APPSIGNAL_PUSH_API_KEY") {
            attributes.push(KeyValue::new("appsignal.config.push_api_key", api_key));
        }

        let resource = opentelemetry_sdk::Resource::new(attributes);

        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(&endpoint)
            .build()?;

        let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(span_exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(resource.clone())
            .build();

        opentelemetry::global::set_tracer_provider(tracer_provider.clone());
        let tracer = tracer_provider.tracer("actions-runner");
        let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        let metrics_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(&endpoint)
            .build()?;

        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(
            metrics_exporter,
            opentelemetry_sdk::runtime::Tokio,
        )
        .build();

        let meter_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource.clone())
            .build();

        opentelemetry::global::set_meter_provider(meter_provider.clone());

        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_endpoint(&endpoint)
            .build()?;

        let logger_provider = opentelemetry_sdk::logs::LoggerProvider::builder()
            .with_batch_exporter(log_exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(resource)
            .build();

        let otel_log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_trace_layer)
            .with(otel_log_layer)
            .init();

        tracing::info!(
            otlp_endpoint = endpoint.as_str(),
            "OpenTelemetry OTLP export enabled (traces, metrics, logs)"
        );

        Ok(Some(OtelGuard {
            tracer_provider,
            meter_provider,
            logger_provider,
        }))
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!("tracing initialised (stdout JSON, no OTLP)");
        Ok(None)
    }
}
