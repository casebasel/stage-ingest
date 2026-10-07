import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./stil/stil.css";
import "./stil/konsole.css";
import "./stil/ingest.css";
import { App } from "./App";
import { gemerkteWahl, thema } from "./thema";

// Vor dem ersten Bild setzen, damit nichts aufblitzt.
document.documentElement.dataset.thema = thema(gemerkteWahl());

createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
