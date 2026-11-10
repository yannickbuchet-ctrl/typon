// Vue 3D dans Chromium sans fenêtre (rendu logiciel SwiftShader) : carte d'exemple en ODB++ (boîtiers), en Gerber
// (sans composants) et, s'ils ont été téléchargés, des exemples externes ODB++ et IPC-2581. Vérifie que l'image
// n'est pas vide, que la rotation et le zoom changent l'image, et que le clic sur un boîtier l'identifie.
// Captures dans essais/3d/ (dossier ignoré par git).
//
// Lancer après « npm run build » : node scripts/essai-3d.mjs
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..");
const captures = path.join(racine, "essais", "3d");
mkdirSync(captures, { recursive: true });
const PORT = Number(process.env.PORT_ESSAI ?? 4193);
const verifier = (c, m) => {
  if (!c) throw new Error(m);
};
const serveur = spawn(process.execPath, [path.join(racine, "node_modules/vite/bin/vite.js"), "preview", "--port", String(PORT), "--strictPort"], { cwd: racine, stdio: "pipe" });
await new Promise((ok, ko) => {
  serveur.stdout.on("data", (d) => String(d).includes(String(PORT)) && ok());
  setTimeout(() => ko(new Error("serveur muet")), 20000);
});
const navigateur = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const erreurs = [];
const bilan = [];

/** Part des pixels de la vue 3D qui ne sont pas le fond, et empreinte de l'image. */
const analyse = (page) => page.evaluate(() => {
  const c = document.querySelector(".vue-3d canvas");
  const t = document.createElement("canvas");
  t.width = 160;
  t.height = 100;
  const g = t.getContext("2d");
  g.drawImage(c, 0, 0, 160, 100);
  const d = g.getImageData(0, 0, 160, 100).data;
  let plein = 0;
  let h = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (Math.abs(d[i] - 10) + Math.abs(d[i + 1] - 14) + Math.abs(d[i + 2] - 19) > 24) plein++;
    h = (h * 31 + d[i] + d[i + 1] * 7 + d[i + 2] * 13) >>> 0;
  }
  return { plein: plein / (160 * 100), h };
});

async function essayer(page, nom, ouvrir, { composants = false } = {}) {
  await page.goto(`http://localhost:${PORT}/`);
  await page.locator("h1").waitFor();
  await ouvrir();
  await page.waitForFunction(() => /lue/.test(document.querySelector(".etat")?.textContent ?? ""), null, { timeout: 120000 });
  await page.getByRole("button", { name: "Voir la carte en 3D (V)" }).click();
  await page.locator(".legende-3d span").first().waitFor({ timeout: 120000 });
  await page.waitForTimeout(800);
  const a = await analyse(page);
  verifier(a.plein > 0.05, `${nom} : image 3D vide (${a.plein})`);
  await page.screenshot({ path: path.join(captures, `3d-${nom}.png`) });
  // Rotation
  const boite = await page.locator(".vue-3d").boundingBox();
  const [cx, cy] = [boite.x + boite.width / 2, boite.y + boite.height / 2];
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 160, cy - 60, { steps: 8 });
  await page.mouse.up();
  await page.waitForTimeout(400);
  const b = await analyse(page);
  verifier(b.h !== a.h, `${nom} : la rotation ne change pas l'image`);
  await page.screenshot({ path: path.join(captures, `3d-${nom}-rotation.png`) });
  // Dessous
  await page.getByRole("button", { name: "Voir le dessous" }).click();
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(captures, `3d-${nom}-dessous.png`) });
  await page.getByRole("button", { name: "Voir le dessus" }).click();
  // Zoom
  await page.mouse.move(cx, cy);
  for (let i = 0; i < 4; i++) await page.mouse.wheel(0, -120);
  await page.waitForTimeout(400);
  const z = await analyse(page);
  verifier(z.h !== a.h, `${nom} : le zoom ne change pas l'image`);
  await page.screenshot({ path: path.join(captures, `3d-${nom}-zoom.png`) });
  const legende = (await page.locator(".legende-3d").innerText()).replace(/\s+/g, " ");
  let choix = "";
  if (composants) {
    verifier(/boîtier/.test(legende), `${nom} : pas de boîtiers (${legende})`);
    // Cherche un boîtier en cliquant sur une grille de points, carte recadrée.
    await page.keyboard.press("f");
    await page.waitForTimeout(300);
    for (let i = 0; i < 400 && !choix; i++) {
      const x = boite.x + boite.width * (0.05 + 0.9 * (i % 20) / 19);
      const y = boite.y + boite.height * (0.1 + 0.8 * Math.floor(i / 20) / 19);
      await page.mouse.click(x, y);
      await page.waitForTimeout(60);
      if (await page.locator(".bulle-3d").isVisible()) choix = (await page.locator(".bulle-3d").innerText()).replace(/\s+/g, " ");
    }
    verifier(choix, `${nom} : aucun boîtier trouvé au clic`);
    await page.screenshot({ path: path.join(captures, `3d-${nom}-choix.png`) });
  }
  // Retour en 2D
  await page.keyboard.press("v");
  await page.locator(".visionneuse:not(.vue-3d)").waitFor();
  bilan.push(`${nom} : ${Math.round(a.plein * 100)} % de l'image occupée ; ${legende}${choix ? ` ; clic : ${choix}` : ""}`);
}

try {
  const page = await navigateur.newPage({ viewport: { width: 1400, height: 900 }, locale: "fr-FR" });
  page.on("pageerror", (e) => erreurs.push(e.message));
  page.on("console", (m) => m.type() === "error" && erreurs.push(`console : ${m.text()}`));
  await essayer(page, "odb", () => page.getByRole("button", { name: /La même en ODB\+\+/ }).click(), { composants: true });
  await essayer(page, "gerber", () => page.getByRole("button", { name: /Carte d'exemple/ }).click());
  for (const [nom, fichier, composants] of [
    ["ipc2581", "exemples-externes/ipc2581/royalblue54l-feather.xml", true],
    ["odb-rigideflex", "exemples-externes/odb/designodb_rigidflex.tgz", true],
    ["odb-wifi", "exemples-externes/odb/sandbox-odb_wifi.tgz", true],
    ["gerber-x3-librepcb", "exemples-externes/ucamco/gerber-job-format-x3-librepcb-sample-2.zip", true],
    ["gerber-x3-li-ion", "exemples-externes/ucamco/gerber-layer-format-x3-li-ion-charger.zip", true],
  ]) {
    const chemin = path.join(racine, fichier);
    if (!existsSync(chemin)) {
      bilan.push(`${nom} : exemple externe absent`);
      continue;
    }
    await essayer(page, nom, async () => {
      const [ch] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: /Choisir des fichiers/ }).click()]);
      await ch.setFiles(chemin);
    }, { composants });
  }
} catch (e) {
  erreurs.push(`ÉCHEC : ${e.message}`);
} finally {
  await navigateur.close();
  serveur.kill();
}
console.log(bilan.join("\n"));
if (erreurs.length) {
  console.log("\nErreurs :\n" + erreurs.join("\n"));
  process.exit(1);
}
console.log("\nTout est conforme.");
