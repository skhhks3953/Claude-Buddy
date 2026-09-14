import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";

// The dev harness is a source, not a UI toggle (§8.1). `import.meta.env.DEV`
// is a compile-time constant, so this whole branch — and the module it
// imports — is absent from a release bundle (§8.3).
if (import.meta.env.DEV) {
  void import("./dev/keys").then(({ bindDevKeys }) => bindDevKeys());
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
