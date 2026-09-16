//! 宿主冒烟测试：加载插件目录，对每个插件调用 `ping`。
//!
//! 用法：
//! ```sh
//! cargo run -p zannen-core --features mock-transport --example host_smoke -- app-shell/src-tauri/plugins
//! ```

use std::path::PathBuf;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("app-shell/src-tauri/plugins"));
    println!("== scanning {}", dir.display());

    let core = zannen_core::Core::new();
    let report = core.load_plugins_from(&dir);
    let mut failed = 0;
    for item in &report {
        match item {
            Ok(id) => println!("loaded: {id}"),
            Err(e) => {
                failed += 1;
                eprintln!("load error: {e}");
            }
        }
    }

    for manifest in core.plugins.list() {
        match core
            .plugins
            .invoke(&manifest.id, "ping", &serde_json::json!({}))
        {
            Ok(v) => println!("ping {} -> {}", manifest.id, v),
            Err(e) => {
                failed += 1;
                eprintln!("ping {} failed: {e}", manifest.id);
            }
        }
    }

    core.plugins.unload_all();
    if failed > 0 {
        std::process::exit(1);
    }
    println!("== smoke ok");
}
