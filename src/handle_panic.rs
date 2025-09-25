pub fn handle_panic() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("Unknown panic message");

        let location = info
            .location()
            .map(|l| format!("at {}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());

        eprintln!("💥 Panic occurred: '{message}' {location}");

        std::process::exit(1);
    }));
}
