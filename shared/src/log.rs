pub fn init() {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_current_span(false)
        .with_span_list(false)
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_line_number(true)
        .init();
}
