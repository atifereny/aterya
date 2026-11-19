import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Preview } from "./preview/Preview";
import { Mixer } from "./mixer/Mixer";
import { Settings } from "./settings/Settings";
import { Float } from "./float/Float";
import { LangPopup } from "./lang/LangPopup";
import { Tip } from "./tip/Tip";
import { Note } from "./notes/Note";
import "./styles/glass.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{location.hash === "#preview" ? <Preview /> : location.hash === "#mixer" ? <Mixer /> : location.hash === "#settings" ? <Settings /> : location.hash === "#lang" ? <LangPopup /> : location.hash === "#tip" ? <Tip /> : location.hash.startsWith("#note=") ? <Note /> : location.hash.startsWith("#float=") ? <Float /> : <App />}</React.StrictMode>,
);
