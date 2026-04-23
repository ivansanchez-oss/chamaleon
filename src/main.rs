use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use chameleon::{KeyboardFilter, KeyboardLayout, Watcher};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use serde::Deserialize;

const DEFAULT_CONFIG: &str = r#"# chameleon configuration
# Layouts accepted: EnglishUS, EnglishUK, SpanishLatinAmerica, SpanishSpain,
# French, German, PortugueseBrazil, Italian, or a raw 8-digit KLID string.

default_layout = "SpanishLatinAmerica"

# Add one [[keyboards]] block per device you want to map.
# `id` is the `VID_xxxx&PID_xxxx` substring of the device's symbolic link.
# `alias` is optional and only used in logs.
#
# [[keyboards]]
# id = "VID_258A&PID_002A"
# alias = "Akko"
# layout = "EnglishUS"
"#;

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
struct Config {
    default_layout: KeyboardLayout,
    #[serde(default)]
    keyboards: Vec<KeyboardSpec>,
}

#[derive(Debug, Deserialize)]
struct KeyboardSpec {
    id: String,
    #[serde(default)]
    alias: Option<String>,
    layout: KeyboardLayout,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("chameleon")
        .join("config.toml")
}

fn ensure_config_exists(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, DEFAULT_CONFIG)?;
    tracing::info!("config created at {}", path.display());
    Ok(())
}

fn build_filter(path: &Path) -> Result<KeyboardFilter, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&text)?;

    let mut builder = KeyboardFilter::builder().default_layout(config.default_layout);
    for kb in config.keyboards {
        builder = builder.on_connect(kb.id, kb.alias, kb.layout);
    }
    Ok(builder.build()?)
}

// Watch the parent directory so we survive save-via-rename (VS Code, Vim, etc).
// Filter events to only those touching the config file.
fn spawn_config_watcher(path: &Path) -> notify::Result<(RecommendedWatcher, Receiver<()>)> {
    let (tx, rx) = channel();
    let target = path.to_path_buf();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res
            && event.paths.iter().any(|p| p == &target)
        {
            let _ = tx.send(());
        }
    })?;

    let parent = path.parent().unwrap_or(Path::new("."));
    watcher.watch(parent, RecursiveMode::NonRecursive)?;
    Ok((watcher, rx))
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let path = config_path();
    ensure_config_exists(&path)?;

    let mut watcher: Watcher = build_filter(&path)?.watch()?;
    let (_config_watcher, rx) = spawn_config_watcher(&path)?;

    tracing::info!(path = %path.display(), "watching config for changes");

    while rx.recv().is_ok() {
        // Collapse a burst of events from a single save into one reload.
        std::thread::sleep(RELOAD_DEBOUNCE);
        while rx.try_recv().is_ok() {}

        match build_filter(&path) {
            Ok(filter) => {
                drop(watcher);
                watcher = filter.watch()?;
                tracing::info!("config reloaded");
            }
            Err(e) => tracing::error!(error = %e, "failed to reload config, keeping previous"),
        }
    }

    Ok(())
}
