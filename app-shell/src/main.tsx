import { createRoot } from "react-dom/client";
import { App } from "./App";
import { initLocale } from "./shell/locale";
import { initTheme } from "./shell/theme";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/components.css";

initTheme();
initLocale();

createRoot(document.getElementById("root")!).render(<App />);
