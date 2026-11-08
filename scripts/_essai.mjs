import { chromium } from "playwright";
import { spawn } from "node:child_process";
import path from "node:path";
const racine = path.resolve(import.meta.dirname, "..");
const serveur = spawn(process.execPath, [path.join(racine, "node_modules/vite/bin/vite.js"), "preview", "--port", "4190", "--strictPort"], { cwd: racine, stdio: "pipe" });
await new Promise((ok) => serveur.stdout.on("data", (d) => String(d).includes("4190") && ok()));
const b = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
try {
  const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  p.on("console", (m) => m.type() === "error" && console.log("console:", m.text()));
  p.on("pageerror", (e) => console.log("pageerror:", e.message));
  await p.goto("http://localhost:4190/");
  await p.waitForTimeout(800);
  await p.screenshot({ path: "essais/accueil.png" });
  const [ch] = await Promise.all([p.waitForEvent("filechooser"), p.getByRole("button", { name: "Choisir des fichiers" }).click()]);
  await ch.setFiles(process.argv[2]);
  await p.waitForSelector(".liste-couches", { timeout: 60000 });
  await p.waitForTimeout(2500);
  await p.screenshot({ path: "essais/vue.png" });
  console.log("état :", await p.locator(".etat").textContent());
} finally {
  await b.close();
  serveur.kill();
}
