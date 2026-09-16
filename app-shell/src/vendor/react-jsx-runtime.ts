// react/jsx-runtime 为 CJS，`export *` 经 rollup commonjs 插件无法静态具名
// （产物会退化成空的 side-effect import），必须显式列举。
export { Fragment, jsx, jsxs } from "react/jsx-runtime";
