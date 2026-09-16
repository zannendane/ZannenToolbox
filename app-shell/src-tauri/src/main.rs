// 防止在 0.0.0.0 等环境下控制台出现窗口的常规写法
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zannen_toolbox_lib::run()
}
