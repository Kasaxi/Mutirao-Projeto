import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./estilo.css";

const raiz = document.getElementById("raiz");
if (!raiz) throw new Error("elemento #raiz não encontrado no index.html");

ReactDOM.createRoot(raiz).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
