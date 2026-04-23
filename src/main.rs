use chameleon::{KeyboardFilter, KeyboardLayout};

fn main() -> Result<(), chameleon::Error> {
    tracing_subscriber::fmt::init();

    let filter = KeyboardFilter::builder(KeyboardLayout::SpanishLatinAmerica)
        .on_connect(KeyboardLayout::EnglishUS)
        .build();

    let _watcher = filter.watch()?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
