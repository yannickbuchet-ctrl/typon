import "@fontsource-variable/inter";
import "./style.css";
import { App } from "./ui/app";

const racine = document.getElementById("typon")!;
racine.replaceChildren();
racine.classList.add("app");
try {
  new App(racine);
} catch (e) {
  racine.textContent = (e as Error).message;
}

// Hors ligne : les fichiers du site sont gardés par le navigateur (service worker). Les fichiers ouverts ne passent
// jamais par lui : ils sont lus sur l'appareil.
if (import.meta.env.PROD && "serviceWorker" in navigator && (location.protocol === "https:" || location.hostname === "localhost" || location.hostname === "127.0.0.1")) {
  navigator.serviceWorker.register("./sw.js").catch(() => {});
}
