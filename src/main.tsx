import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./style.css";
if (import.meta.env.VITE_E2E === "true") await import("@wdio/tauri-plugin");
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
