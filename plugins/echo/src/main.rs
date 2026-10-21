// SPDX-License-Identifier: GPL-3.0-only
//! The echo plugin's binary entry: stdin/stdout are the whole wire.

use tokio::io::BufReader;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    astra_plugin_echo::run(BufReader::new(tokio::io::stdin()), tokio::io::stdout()).await
}
