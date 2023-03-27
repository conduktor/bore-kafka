import React from "react";
import ReactDOM from "react-dom/client";
import { AuthProvider } from "@conduktor/auth";
import App from "./App";
import "./styles.css";

const env = import.meta.env;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AuthProvider mode={env.VITE_MODE}
                  domain={env.VITE_AUTH0_DOMAIN}
                  clientId={env.VITE_AUTH0_CLIENT_ID}
                  redirectUri={window.location.origin}>
      <App />
    </AuthProvider>
  </React.StrictMode>
);
