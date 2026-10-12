import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { startPlaybackClock, startWatcher } from "./core/runtime";
import { App } from "./shell/App";

startWatcher();
startPlaybackClock();

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
