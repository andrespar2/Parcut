import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/globals.css";
import { applyTheme } from "./store/app";

applyTheme("dark");

class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            minHeight: "100vh",
            display: "grid",
            placeItems: "center",
            padding: 32,
            color: "#e9eef7",
            background: "#05060a",
            fontFamily: "Segoe UI, sans-serif",
          }}
        >
          <div style={{ maxWidth: 520, textAlign: "center" }}>
            <h1 style={{ fontSize: 22, marginBottom: 8 }}>ParCut encontró un problema</h1>
            <p style={{ color: "#9aa6bc", fontSize: 13, lineHeight: 1.6 }}>
              {this.state.error.message}
            </p>
            <button
              onClick={() => window.location.reload()}
              style={{
                marginTop: 16,
                padding: "10px 18px",
                borderRadius: 10,
                border: "1px solid rgba(255,255,255,0.2)",
                background: "linear-gradient(120deg,#5ee7ff,#7f8cff,#c86bff)",
                color: "#fff",
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              Volver a cargar
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
