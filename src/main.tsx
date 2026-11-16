import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Preview } from "./preview/Preview";
import "./styles/glass.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{location.hash === "#preview" ? <Preview /> : <App />}</React.StrictMode>,
);
