import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./stil/stil.css";
import "./stil/konsole.css";
import "./stil/ingest.css";
import { App } from "./App";
import { gemerkteWahl, thema } from "./thema";

// Im normalen Browser (ohne Tauri): Vorschau mit Beispieldaten statt des Rust-Kerns.
if (!("__TAURI_INTERNALS__" in window)) {
  (await import("./vorschau")).einrichten();
}

// Vor dem ersten Bild setzen, damit nichts aufblitzt.
document.documentElement.dataset.thema = thema(gemerkteWahl());

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
