# Fichiers d'exemple externes

Fichiers publics trouvés sur Internet, utilisés pour **essayer et tester** Typon. Ils ne sont **pas** dans le dépôt
(leurs conditions de diffusion ne le permettent pas toujours) : `node scripts/telecharger-exemples.mjs` les télécharge
dans ce dossier. Les exemples intégrés au site (`exemples/`) sont, eux, des cartes fictives créées pour Typon.

Téléchargés le 2026-11-08.

## Ucamco (éditeur de la norme Gerber) — copyright Ucamco

Page : <https://www.ucamco.com/en/gerber/downloads>

| Archive | Contenu |
|---|---|
| `gerber-layer-format-test-files` | Fichiers de test de la norme, chacun avec son **image de référence** (`.png`) : macros, blocs imbriqués, polarités, ouvertures |
| `pcb-fabrication-data-test-file-1`, `-2` | Jeux complets de fabrication en Gerber X2 (4 et 10 couches), avec netlist IPC-D-356 |
| `gerber-layer-format-x3-kit-dev-coldfire-xilinx`, `-stickhub`, `-li-ion-charger` | Cartes KiCad en Gerber X3 (couches composants), avec fichier de job |
| `gerber-job-format-x3-librepcb-sample-1`, `-2` | Cartes LibrePCB : Gerber X3, perçages Excellon |
| `gerber-job-format-test-files`, `-sample-files` | Fichiers de job `.gbrjob` |
| `xnc-format-test-files` | Perçages XNC (Ucamco, KiCad, LibrePCB) |
| `gerber-layer-format-x3-example-component-layer-from-spec` | Couche composants de la norme |

Normes (dossier `specs/`) : Gerber Layer Format révision 2026-05, XNC 2021-11, Gerber Job Format 2020-08.

## ODB++

| Fichier | Source | Remarque |
|---|---|---|
| `odb/designodb_rigidflex.tgz` | <https://odbplusplus.com/design/our-resources/> (Siemens) | Exemple officiel ODB++ 8.1, carte rigide-flexible |
| `odb/flat_hierarchy-odb.tgz`, `video-odb.tgz`, `sandbox-odb_pc4.tgz`, `sandbox-odb_wifi.tgz`, `minimal-test-odb.zip` | <https://github.com/mcix/odbpp> (dossier `examples/`) | Exports ODB++ de projets de démonstration KiCad ; licence non indiquée |
| `odb/sample_design.tgz` | <https://github.com/nam20485/OdbDesignTestData> | Licence non indiquée |

Norme (dossier `specs/`) : ODB++ Format Specification v8.1 update 4 (<https://odbplusplus.com>).
