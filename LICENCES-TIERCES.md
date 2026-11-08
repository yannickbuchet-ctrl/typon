# Composants tiers

Typon est sous licence MIT (fichier `LICENSE`). Il intègre les composants suivants, tous sous des licences libres
compatibles avec la licence MIT.

## Dans le moteur WebAssembly (Rust)

| Composant | Rôle | Licence |
|---|---|---|
| [serde](https://serde.rs/), [serde_json](https://github.com/serde-rs/json) | Description des fichiers en JSON pour la page | MIT ou Apache 2.0 |
| [miniz_oxide](https://github.com/Frommi/miniz_oxide) et adler2 | Décompression « deflate » (ZIP, gzip) | MIT, Zlib ou Apache 2.0 |
| [earcutr](https://github.com/frewsxcv/earcutr) | Triangulation des polygones | ISC |
| [i_overlay](https://github.com/iShape-Rust/iOverlay), i_float, i_shape, i_tree, i_key_sort | Opérations booléennes sur les polygones (trous des macros d'ouverture) | MIT (i_overlay : MIT ou Apache 2.0) |
| num-traits, itertools, either, libm, itoa, memchr, zmij | Dépendances des précédents | MIT, Apache 2.0 ou Unlicense |

## Dans la page

| Composant | Rôle | Licence |
|---|---|---|
| [pdf-lib](https://pdf-lib.js.org/) | Rapport de comparaison en PDF | MIT |
| [Inter](https://rsms.me/inter/) (paquet @fontsource-variable/inter) | Police de l'interface, servie par le site | SIL Open Font License 1.1 |

## Fichiers d'essai

Les fichiers d'exemple externes (Ucamco, exemples ODB++) ne sont pas distribués avec Typon : voir
`exemples-externes/SOURCES.md`. Les cartes d'exemple du site (`public/exemples/`) sont fictives et générées par
`scripts/generer_exemples.py`, sous la licence de Typon.
