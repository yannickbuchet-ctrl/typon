# Carte d'exemple : défauts de fabrication volontaires

La carte d'exemple du site (`public/exemples/typon-demo-revA.zip`, sa révision B et sa version ODB++) est un nœud
capteur IoT fictif, dessiné par `scripts/generer_exemples.py`. Elle est conçue comme une vraie carte : aucun
court-circuit, aucun net coupé, isolement d'au moins 0,15 mm entre cuivres de nets différents (0,3 mm autour des
zones de cuivre), anneaux d'au moins 0,15 mm, cuivre à au moins 0,4 mm du contour et des découpes, ponts de vernis
d'au moins 0,1 mm, sérigraphie en traits de 0,15 mm, hors des ouvertures du vernis et à l'intérieur de la carte.

Six défauts y ont été glissés **volontairement**, pour montrer ce que trouve un contrôle de fabrication (DFM). Ils sont
présents dans les révisions A et B et dans la version ODB++. Les seuils sont ceux du contrôleur de Typon ; les
positions sont en millimètres, origine au coin inférieur gauche de la carte, vue de dessus.

| # | Règle | Couche | Position X ; Y (mm) | Valeur mesurée | Seuil | Où |
|---|---|---|---|---|---|---|
| 1 | Largeur de piste | Cuivre dessus | 52,35 ; 30,94 | 0,10 mm | ≥ 0,127 mm | Liaison 3V3 entre la broche 8 du capteur U3 et son condensateur C11, segment de (52,30 ; 30,475) à (52,40 ; 31,40) |
| 2 | Isolement cuivre-cuivre | Cuivre dessous | 35,00 ; 5,55 | 0,09 mm | ≥ 0,127 mm | Via de masse (Ø 0,6 mm, centre en 35,00 ; 5,89) trop près de la piste NRST (0,2 mm, y = 5,40) qui longe la barrette J2 |
| 3 | Anneau annulaire | Cuivre dessus et dessous | 22,80 ; 19,00 | 0,05 mm | ≥ 0,13 mm | Via de masse du condensateur C6 (quartz) : pastille de 0,40 mm pour un trou de 0,30 mm |
| 4 | Cuivre au bord de la carte | Cuivre dessus et dessous | 30,00 ; 0,55 | 0,25 mm | ≥ 0,3 mm | Via de couture de masse (Ø 0,6 mm) sous la barrette J2, entre ses broches 6 et 7, à 0,25 mm du bord inférieur |
| 5 | Diamètre de perçage | Perçage métallisé | 40,50 ; 14,50 | 0,25 mm | ≥ 0,3 mm | Via de couture de masse percé à 0,25 mm (pastille de 0,6 mm : l'anneau, 0,175 mm, reste correct) |
| 6 | Sérigraphie sur une ouverture du vernis | Sérigraphie dessus | 14,20 ; 6,85 | recouvrement de 0,2 mm | aucun | Étiquette « RST » du bouton SW1, posée sur le haut de la pastille 3 (NRST) |

## Vérifier

`scripts/verifier_exemple.py` relit les fichiers Gerber et Excellon produits, sans rien emprunter au générateur, et
contrôle les règles ci-dessus, la connectivité de chaque net (à travers les vias et les trous métallisés) et l'absence
de court-circuit. Il doit trouver exactement ces six défauts, et rien d'autre :

```
pip install shapely pillow
python scripts/generer_exemples.py
python scripts/verifier_exemple.py                    # révisions A et B
python scripts/verifier_exemple.py --images essais    # et une image de chaque couche de cuivre
```

## La carte

- 60 × 38 mm, coins arrondis, 4 couches (signaux dessus, plan de masse, plans 3V3 et VSYS, signaux dessous),
  1,6 mm, finition ENIG ; découpe en L de 1 mm autour du capteur pour l'isoler thermiquement du reste de la carte.
- 47 composants en révision A : microcontrôleur STM32G0B1 en QFN-32 (pas de 0,5 mm, pastille centrale et vias
  thermiques), quartz 8 MHz, embase USB-C avec protection ESD (USBLC6-2SC6), fusible réarmable, diode Schottky et
  transistor de commutation batterie, régulateur 3,3 V, chargeur Li-ion MCP73831 et connecteur de batterie JST PH,
  capteur BME280 (LGA-8), connecteur Qwiic, boutons RESET et BOOT, trois LED, pastilles de programmation
  Tag-Connect, deux barrettes au pas de 2,54 mm, trois mires, résistances et condensateurs 0402, 0603 et 0805.
- Nets nommés comme dans un logiciel de CAO : GND, 3V3, VBUS, VUSB, VSYS, VBAT, USB_DP, USB_DM, CC1, CC2, SDA, SCL,
  SWDIO, SWCLK, NRST, BOOT0, OSC_IN, OSC_OUT…, et `Net-(D2-A)` pour les nets sans nom.
- Révision B : piste SCL déroutée sur le dessus, rappels I²C R9 et R10 passés de 4k7 à 2k2, LED d'alimentation D2 et
  sa résistance R11 retirées, condensateur C13 ajouté près du connecteur Qwiic, via de couture déplacé.
