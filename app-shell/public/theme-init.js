/* 主题 FOUC 防护：模块加载前先落 data-theme（与 src/shell/theme.ts 同步逻辑）。
   以 classic script 方式在 <head> 同步执行，CSP 无需放开 inline。 */
(function () {
  var m = localStorage.getItem("zannen-theme") || "auto";
  var d =
    m === "auto"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : m;
  document.documentElement.dataset.theme = d;
  document.documentElement.style.colorScheme = d;
})();
