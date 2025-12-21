# bookedblocks

A Terminal User Iterface (TUI) for visualizing Kraken's order book using its WebSocket API.

[demo](https://youtu.be/2s7JDBGHi18?si=IH47iTb58EPFlFb8)

## Quick start

This project is a pure Rust project and conforms to the classic `cargo` build system:

```bash
cargo build # for building the project
cargo run -- Ticker/Pair # for running the executable
cargo test # for running unittesting
```

## Ideas

bookedblocks is an asynchronous application where separation of concerns is hopefully enforced through the separation of threads:

* A main dispatcher thread serves to orchestrate different parts of the application together. This is the first thread that gets launched in the entrypoint.
* A data thread takes care of keeping the websocket connection alive and appending to an internal cache.
* Pipeline threads process the cached data and prepares it for updating the UI.
* A UI thread renders the data and runs the screen update loop.

## Technology stack

The various capabilities of the application are based on different open source technologies:

* [tokio](https://tokio.rs/): for the asynchronous runtime and managing threads
* [ratatui](https://ratatui.rs/): as a TUI library for handling rendering data to the screen
* [kraken_async_rs](https://crates.io/crates/kraken-async-rs): for facilitating websocket connections to the Kraken API

## UI

The UI has very simple ambitions. When running, the application pull data from the Kraken API in the backgrounf and shows 4 elements for the selected ticker:
* **Order Map**: A main central heat map with time on the x axis and price on the y axis. Volume is encoded through color intensity.
* **Order Volumes**: A projection onto the time axis of the heat map reading as booked volume over time.
* **Depth**: A projection onto the price axis reading as current market depth.
* A snapshot of the current status using the ticker information.
