// Télécharge les fichiers d'exemple externes (normes et fichiers de test publics) dans exemples-externes/, pour les
// essais du moteur (core/tests/exemples.rs). Ils ne sont pas dans le dépôt : leurs auteurs en gardent les droits.
// Sources détaillées dans exemples-externes/SOURCES.md.
//
//   node scripts/telecharger-exemples.mjs
import fs from "node:fs";
import path from "node:path";

const racine = path.resolve(import.meta.dirname, "..", "exemples-externes");
const UCAMCO = "https://www.ucamco.com/files/downloads/file_en";
const fichiers = [
  ...[
    "423/gerber-layer-format-test-files",
    "220/pcb-fabrication-data-test-file-1",
    "221/pcb-fabrication-data-test-file-2",
    "256/gerber-job-format-test-files",
    "357/gerber-job-format-sample-files",
    "306/xnc-format-test-files",
    "351/gerber-layer-format-x3-example-component-layer-from-spec",
    "502/gerber-job-format-x3-librepcb-sample-1",
    "503/gerber-job-format-x3-librepcb-sample-2",
    "513/gerber-layer-format-x3-kit-dev-coldfire-xilinx",
    "514/gerber-layer-format-x3-stickhub",
    "515/gerber-layer-format-x3-li-ion-charger",
  ].map((f) => [`ucamco/${path.basename(f)}.zip`, `${UCAMCO}/${f}_en.zip`]),
  ["odb/designodb_rigidflex.tgz", "https://odbplusplus.com/wp-content/resource-docs/designodb_rigidflex.tgz"],
  ...["flat_hierarchy-odb.tgz", "sandbox-odb_pc4.tgz", "sandbox-odb_wifi.tgz", "video-odb.tgz"].map((n) => [`odb/${n}`, `https://raw.githubusercontent.com/mcix/odbpp/main/examples/${n}`]),
  ["odb/minimal-test-odb.zip", "https://raw.githubusercontent.com/mcix/odbpp/main/odbpp-lib/src/test/resources/odb/minimal-test-odb.zip"],
  ["odb/sample_design.tgz", "https://media.githubusercontent.com/media/nam20485/OdbDesignTestData/development/TEST_DATA/sample_design.tgz"],
];

let echecs = 0;
for (const [chemin, url] of fichiers) {
  const cible = path.join(racine, chemin);
  if (fs.existsSync(cible)) {
    console.log(`déjà là   ${chemin}`);
    continue;
  }
  fs.mkdirSync(path.dirname(cible), { recursive: true });
  try {
    const r = await fetch(url, { headers: { "User-Agent": "Typon (essais)" } });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    fs.writeFileSync(cible, Buffer.from(await r.arrayBuffer()));
    console.log(`téléchargé ${chemin} (${Math.round(fs.statSync(cible).size / 1024)} Ko)`);
  } catch (e) {
    echecs++;
    console.log(`ÉCHEC     ${chemin} : ${e.message}`);
  }
}
// Les fichiers de test de la norme sont lus décompressés (avec leurs images de référence).
const zip = path.join(racine, "ucamco", "gerber-layer-format-test-files.zip");
const dossier = path.join(racine, "ucamco", "gerber-layer-format-test-files");
if (fs.existsSync(zip) && !fs.existsSync(dossier)) console.log(`Décompressez ${zip} dans ${dossier} pour l'essai de rendu.`);
process.exit(echecs ? 1 : 0);
