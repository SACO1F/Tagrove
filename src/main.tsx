import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { GlobalTooltips } from "./WarmTooltip";
import Floating from "./Floating";
import "./style.css";
import "./apple-style.css";
import "./tag-colors.css";
import "./micro-interactions.css";
import "./beta-support.css";
import "./control-consistency.css";
import "./beta8-controls.css";
import "./list-refinements.css";
import "./content-reader.css";
import "./reader-controls.css";
import "./reader-zoom.css";
import "./appearance.css";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {new URLSearchParams(location.search).has("floating") ? (
      <Floating />
    ) : (
      <App />
    )}
    <GlobalTooltips />
  </React.StrictMode>,
);
