use std::sync::OnceLock;
use tracing_subscriber::{EnvFilter, fmt};

static LOGGING_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn init_logging() -> Result<(), String> {
    LOGGING_INIT
        .get_or_init(|| {
            let filter = EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("easymill=info,warn"))
                .map_err(|err| err.to_string())?;

            fmt()
                .with_env_filter(filter)
                .with_target(false)
                .compact()
                .try_init()
                .map_err(|err| err.to_string())
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::init_logging;

    #[test]
    fn logging_initialization_is_idempotent() {
        init_logging().unwrap();
        init_logging().unwrap();
    }
}
