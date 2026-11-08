"""Cartes d'exemple de Typon : un module capteur fictif à 4 couches, en Gerber X2/X3 (révisions A et B) et en ODB++
(révision A). Aucune donnée réelle : tout est dessiné ici. Licence MIT, comme Typon.

    python scripts/generer_exemples.py

Produit dans public/exemples/ :
    typon-demo-revA.zip      Gerber X2/X3, perçages XNC, fichier de job
    typon-demo-revB.zip      la révision B (quelques modifications à retrouver avec la comparaison)
    typon-demo-revA-odb.tgz  la révision A en ODB++ 8.1 (avec un code-barres de numéro de série)

et dans core/tests/donnees/ (essais du moteur) :
    rs274d-eagle.zip         cuivre et sérigraphie du dessus en Gerber RS-274D, avec une roue Eagle (.whl, pouces)
    rs274d-protel.zip        les mêmes avec une liste d'ouvertures Protel (.apr, mils, zéros de fin omis)
"""
import io
import json
import math
import os
import tarfile
import time
import zipfile

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SORTIE = os.path.join(RACINE, "public", "exemples")
DATE = "2026-11-08T10:00:00+01:00"
LARGEUR, HAUTEUR = 64.0, 42.0

# ---------------------------------------------------------------------------------------------------------------------
# Police vectorielle (traits dans une case 4 × 6), pour la sérigraphie et le fichier fonts/standard de l'ODB++.
# ---------------------------------------------------------------------------------------------------------------------
POLICE = {
    "A": [[(0, 0), (0, 4), (2, 6), (4, 4), (4, 0)], [(0, 3), (4, 3)]],
    "B": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(3, 3), (4, 2), (4, 1), (3, 0), (0, 0)]],
    "C": [[(4, 1), (3, 0), (1, 0), (0, 1), (0, 5), (1, 6), (3, 6), (4, 5)]],
    "D": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 1), (3, 0), (0, 0)]],
    "E": [[(4, 0), (0, 0), (0, 6), (4, 6)], [(0, 3), (3, 3)]],
    "F": [[(0, 0), (0, 6), (4, 6)], [(0, 3), (3, 3)]],
    "G": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1), (4, 3), (2, 3)]],
    "H": [[(0, 0), (0, 6)], [(4, 0), (4, 6)], [(0, 3), (4, 3)]],
    "I": [[(1, 0), (3, 0)], [(2, 0), (2, 6)], [(1, 6), (3, 6)]],
    "J": [[(4, 6), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "K": [[(0, 0), (0, 6)], [(4, 6), (0, 2)], [(1, 3), (4, 0)]],
    "L": [[(0, 6), (0, 0), (4, 0)]],
    "M": [[(0, 0), (0, 6), (2, 3), (4, 6), (4, 0)]],
    "N": [[(0, 0), (0, 6), (4, 0), (4, 6)]],
    "O": [[(1, 0), (0, 1), (0, 5), (1, 6), (3, 6), (4, 5), (4, 1), (3, 0), (1, 0)]],
    "P": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)]],
    "Q": [[(1, 0), (0, 1), (0, 5), (1, 6), (3, 6), (4, 5), (4, 1), (3, 0), (1, 0)], [(2, 2), (4, 0)]],
    "R": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(2, 3), (4, 0)]],
    "S": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "T": [[(0, 6), (4, 6)], [(2, 6), (2, 0)]],
    "U": [[(0, 6), (0, 1), (1, 0), (3, 0), (4, 1), (4, 6)]],
    "V": [[(0, 6), (2, 0), (4, 6)]],
    "W": [[(0, 6), (1, 0), (2, 3), (3, 0), (4, 6)]],
    "X": [[(0, 0), (4, 6)], [(0, 6), (4, 0)]],
    "Y": [[(0, 6), (2, 3), (4, 6)], [(2, 3), (2, 0)]],
    "Z": [[(0, 6), (4, 6), (0, 0), (4, 0)]],
    "0": [[(1, 0), (0, 1), (0, 5), (1, 6), (3, 6), (4, 5), (4, 1), (3, 0), (1, 0)], [(0, 1), (4, 5)]],
    "1": [[(1, 5), (2, 6), (2, 0)], [(1, 0), (3, 0)]],
    "2": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (0, 0), (4, 0)]],
    "3": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)], [(1, 3), (3, 3)]],
    "4": [[(3, 0), (3, 6), (0, 2), (4, 2)]],
    "5": [[(4, 6), (0, 6), (0, 3), (3, 3), (4, 2), (4, 1), (3, 0), (0, 0)]],
    "6": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2), (3, 3), (0, 3)]],
    "7": [[(0, 6), (4, 6), (1, 0)]],
    "8": [[(1, 3), (0, 4), (0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (3, 3), (1, 3), (0, 2), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2), (3, 3)]],
    "9": [[(0, 1), (1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (4, 3)]],
    "-": [[(1, 3), (3, 3)]],
    ".": [[(2, 0), (2, 0.4)]],
    "+": [[(0, 3), (4, 3)], [(2, 1), (2, 5)]],
    "/": [[(0, 0), (4, 6)]],
}


# Code 39 (comme le lit Typon) : 1 = élément large (3 modules), 0 = étroit ; caractère de début et de fin « * ».
CODE39 = dict(zip("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%*", [
    "000110100", "100100001", "001100001", "101100000", "000110001", "100110000", "001110000", "000100101", "100100100", "001100100",
    "100001001", "001001001", "101001000", "000011001", "100011000", "001011000", "000001101", "100001100", "001001100", "000011100",
    "100000011", "001000011", "101000010", "000010011", "100010010", "001010010", "000000111", "100000110", "001000110", "000010110",
    "110000001", "011000001", "111000000", "010010001", "110010000", "011010000", "010000101", "110000100", "011000100", "010101000",
    "010100010", "010001010", "000101010", "010010100"]))
ALPHABET39 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%"
CODE_BARRES = (21.65, 5.3, "TD-0001", 0.13, 2.4)  # x, y (coin inférieur gauche), texte, barre fine, hauteur (mm)


def barres_code39(x, y, texte, w, h, controle=True):
    """Rectangles (x0, y0, x1, y1) des barres d'un code 39 avec clé de contrôle."""
    symboles = list(texte)
    if controle:
        symboles.append(ALPHABET39[sum(ALPHABET39.index(c) for c in texte) % 43])
    modules = []
    for ch in ["*"] + symboles + ["*"]:
        for j, large in enumerate(CODE39[ch]):
            modules += [j % 2 == 0] * (3 if large == "1" else 1)
        modules.append(False)
    modules.pop()
    rects = []
    debut = None
    for i, barre in enumerate(modules + [False]):
        if barre and debut is None:
            debut = i
        if not barre and debut is not None:
            rects.append((x + debut * w, y, x + i * w, y + h))
            debut = None
    return rects


def texte_en_traits(texte, x, y, hauteur, centre=False, miroir=False):
    """Traits (segments) d'un texte, en mm. Case de 4 × 6 unités ; avance de 6 unités. `miroir` : texte de la face
    du dessous, écrit en miroir dans le fichier pour se lire à l'endroit quand on regarde la carte par-dessous."""
    k = hauteur / 6.0
    largeur = len(texte) * 6 * k - 2 * k
    x0 = x - largeur / 2 if centre else x
    traits = []
    for i, ch in enumerate(texte.upper()):
        for ligne in POLICE.get(ch, []):
            pts = [(x0 + (i * 6 + px) * k, y + py * k) for px, py in ligne]
            if miroir:
                pts = [(2 * x0 + largeur - px, py) for px, py in pts]
            for a, b in zip(pts, pts[1:]):
                traits.append((a, b))
    return traits


# ---------------------------------------------------------------------------------------------------------------------
# Modèle de la carte : pastilles, pistes, vias, zones, sérigraphie, perçages, composants.
# ---------------------------------------------------------------------------------------------------------------------
class Carte:
    def __init__(self, revision):
        self.revision = revision
        self.pastilles = []  # dict : couche ("top"/"bottom"/"all"), forme, x, y, w, h, net, comp, broche, rot
        self.pistes = []  # dict : couche, points, largeur, net
        self.vias = []  # dict : x, y, d, trou, net
        self.zones = []  # dict : couche, contour, net
        self.serigraphie = {"top": [], "bottom": []}  # segments
        self.trous_npth = []  # (x, y, d)
        self.composants = []  # dict : ref, valeur, boitier, x, y, rot, cote, fabricant, mpn, contour (rectangle)
        self.nets = set()

    def composant(self, ref, valeur, boitier, x, y, rot, pastilles, mpn, fabricant="Typon Demo", cote="top", corps=(2.0, 1.2)):
        """pastilles : liste (broche, dx, dy, forme, w, h, net, trou) en coordonnées du composant (rot 0)."""
        c = {"ref": ref, "valeur": valeur, "boitier": boitier, "x": x, "y": y, "rot": rot, "cote": cote, "mpn": mpn, "fabricant": fabricant, "corps": corps, "broches": []}
        a = math.radians(rot)
        for broche, dx, dy, forme, w, h, net, trou in pastilles:
            px = x + dx * math.cos(a) - dy * math.sin(a)
            py = y + dx * math.sin(a) + dy * math.cos(a)
            couche = "all" if trou else cote
            self.pastilles.append({"couche": couche, "forme": forme, "x": px, "y": py, "w": w, "h": h, "net": net, "comp": ref, "broche": broche, "rot": rot, "trou": trou})
            c["broches"].append((broche, px, py, net))
            self.nets.add(net)
        self.composants.append(c)
        # Sérigraphie : contour du corps et repère
        cw, ch = corps
        coins = [(-cw / 2, -ch / 2), (cw / 2, -ch / 2), (cw / 2, ch / 2), (-cw / 2, ch / 2)]
        pts = [(x + u * math.cos(a) - v * math.sin(a), y + u * math.sin(a) + v * math.cos(a)) for u, v in coins]
        for p, q in zip(pts, pts[1:] + pts[:1]):
            self.serigraphie[cote].append((p, q))
        self.serigraphie[cote].extend(texte_en_traits(ref, x, y + max(cw, ch) / 2 + 0.6, 0.9, centre=True))
        return c

    def piste(self, couche, points, largeur, net):
        self.pistes.append({"couche": couche, "points": points, "largeur": largeur, "net": net})
        self.nets.add(net)

    def via(self, x, y, net, d=0.6, trou=0.3):
        self.vias.append({"x": x, "y": y, "d": d, "trou": trou, "net": net})
        self.nets.add(net)


def soic8(c, ref, x, y, rot, nets, valeur, mpn):
    p = []
    for i in range(4):
        p.append((str(i + 1), -2.7, 1.905 - i * 1.27, "rect", 1.55, 0.6, nets[i], 0))
        p.append((str(8 - i), 2.7, 1.905 - i * 1.27, "rect", 1.55, 0.6, nets[7 - i], 0))
    return c.composant(ref, valeur, "SOIC-8", x, y, rot, p, mpn, corps=(3.9, 4.9))


def c0603(c, ref, x, y, rot, n1, n2, valeur, mpn):
    p = [("1", -0.8, 0, "rect", 0.9, 0.95, n1, 0), ("2", 0.8, 0, "rect", 0.9, 0.95, n2, 0)]
    return c.composant(ref, valeur, "0603", x, y, rot, p, mpn, corps=(1.6, 0.8))


def connecteur(c, ref, x, y, rot, nets, valeur, mpn):
    p = [(str(i + 1), 0, -i * 2.54, "rect" if i == 0 else "oval", 1.7, 1.7, nets[i], 1.0) for i in range(len(nets))]
    return c.composant(ref, valeur, f"PinHeader_1x{len(nets)}_P2.54mm", x, y, rot, p, mpn, corps=(2.54, 2.54 * len(nets)))


def construire(revision):
    c = Carte(revision)
    # Microcontrôleur et capteur
    soic8(c, "U1", 22, 22, 0, ["3V3", "SDA", "SCL", "GND", "INT", "TX", "RX", "3V3"], "MCU-8", "TD-MCU8")
    soic8(c, "U2", 44, 22, 0, ["3V3", "SDA", "SCL", "GND", "GND", "ALERT", "3V3", "3V3"], "SENS-01", "TD-SENS01")
    connecteur(c, "J1", 6, 29, 0, ["3V3", "GND", "TX", "RX"], "UART", "HDR-1x4")
    c0603(c, "C1", 22, 30, 0, "3V3", "GND", "100n", "TD-C100N")
    c0603(c, "C2", 44, 30, 0, "3V3", "GND", "100n", "TD-C100N")
    c0603(c, "R1", 33, 30, 90, "3V3", "SDA", "4k7", "TD-R4K7")
    c0603(c, "R2", 36, 30, 90, "3V3", "SCL", "4k7", "TD-R4K7")
    c0603(c, "C3", 12, 12, 0, "3V3", "GND", "10u", "TD-C10U")
    if revision == "A":
        c0603(c, "R4", 54, 12, 90, "ALERT", "3V3", "10k", "TD-R10K")
    else:
        c0603(c, "C5", 54, 12, 90, "ALERT", "GND", "1n", "TD-C1N")
    c0603(c, "D1", 30, 12, 0, "INT", "LED_K", "VERT", "TD-LED-G")
    c0603(c, "R3", 36, 12, 0, "LED_K", "GND", "1k", "TD-R1K")

    def pad(ref, broche):
        for p in c.pastilles:
            if p["comp"] == ref and p["broche"] == broche:
                return (p["x"], p["y"])
        raise KeyError((ref, broche))

    # Pistes du dessus
    T = "top"
    c.piste(T, [pad("U1", "2"), (26, 20.635), (40, 20.635), pad("U2", "2")], 0.25, "SDA")
    if revision == "A":
        c.piste(T, [pad("U1", "3"), (27, 19.365), (39, 19.365), pad("U2", "3")], 0.25, "SCL")
    else:
        # Révision B : SCL contourne par le bas
        c.piste(T, [pad("U1", "3"), (26, 18.0), (40, 18.0), (40.6, 19.365), pad("U2", "3")], 0.25, "SCL")
    c.piste(T, [pad("R1", "2"), (33, 20.635)], 0.25, "SDA")
    c.piste(T, [pad("R2", "2"), (36, 26.0), (36, 19.365 if revision == "A" else 18.0)], 0.25, "SCL")
    c.piste(T, [pad("U1", "1"), (19.3, 25.5), (19.3, 30), pad("C1", "1")], 0.4, "3V3")
    c.piste(T, [pad("U2", "1"), (41.3, 25.5), (41.3, 30), pad("C2", "1")], 0.4, "3V3")
    c.piste(T, [pad("R1", "1"), (33, 33), (36, 33), pad("R2", "1")], 0.3, "3V3")
    c.piste(T, [pad("J1", "3"), (10, 23.92), (14, 19), (16, 19), (17, 19), (19.3, 17.0), (24.7, 17.0), (24.7, 18.19), pad("U1", "6")], 0.25, "TX")
    c.piste(T, [pad("J1", "4"), (9, 21.38), (13, 16), (18, 15.5), (26, 15.5), (26.0, 19.365 - 0.3), pad("U1", "7")], 0.25, "RX")
    c.piste(T, [pad("U1", "5"), (26, 18.0 if revision == "A" else 17.6), (28, 14), (29.2, 12), pad("D1", "1")], 0.25, "INT")
    c.piste(T, [pad("D1", "2"), pad("R3", "1")], 0.25, "LED_K")
    c.piste(T, [pad("U2", "6"), (48, 20.0), (52, 14), (54, 12.8), pad("R4" if revision == "A" else "C5", "1")], 0.25, "ALERT")
    c.piste(T, [pad("J1", "1"), (9, 29), (12, 33), (19.3, 33), (19.3, 30)], 0.5, "3V3")
    # Vias vers les plans
    for x, y, net in [(24, 30, "GND"), (46, 30, "GND"), (14, 12, "GND"), (39, 12, "GND"), (20, 33, "3V3"), (42, 33, "3V3"), (12, 9.5, "3V3")]:
        c.via(x, y, net)
    if revision == "B":
        c.via(50, 24, "GND")
    c.piste(T, [pad("C1", "2"), (24, 30)], 0.4, "GND")
    c.piste(T, [pad("C2", "2"), (46, 30)], 0.4, "GND")
    c.piste(T, [pad("C3", "2"), (14, 12)], 0.4, "GND")
    c.piste(T, [pad("C3", "1"), (11.2, 10.3), (12, 9.5)], 0.4, "3V3")
    c.piste(T, [pad("R3", "2"), (39, 12)], 0.3, "GND")
    c.piste(T, [pad("U1", "4"), (19.3, 18.19), (19.3, 16)], 0.4, "GND")
    c.via(19.3, 16, "GND")
    c.piste(T, [pad("U2", "4"), pad("U2", "5")], 0.4, "GND")
    c.piste(T, [(41.3, 18.19), (41.3, 16)], 0.4, "GND")
    c.via(41.3, 16, "GND")
    if revision == "A":
        c.piste(T, [pad("R4", "2"), (54, 16), (52, 22), (47.4, 22.635)], 0.3, "3V3")
    else:
        c.piste(T, [pad("C5", "2"), (50, 24)], 0.3, "GND")
    # Pistes du dessous
    B = "bottom"
    c.piste(B, [(20, 33), (42, 33)], 0.5, "3V3")
    c.piste(B, [pad("J1", "2"), (6, 20), (14, 12)], 0.5, "GND")
    # Plans internes : GND (couche 2) et 3V3 (couche 3), dégagements autour des perçages d'autres nets.
    contour_plan = [(1.0, 1.0), (LARGEUR - 1.0, 1.0), (LARGEUR - 1.0, HAUTEUR - 1.0), (1.0, HAUTEUR - 1.0)]
    c.zones.append({"couche": "gnd", "contour": contour_plan, "net": "GND"})
    c.zones.append({"couche": "pwr", "contour": contour_plan, "net": "3V3"})
    # Zone de masse sur le dessous (cuivre coulé), avec dégagement autour des éléments d'autres nets
    c.zones.append({"couche": "bottom", "contour": [(2, 2), (LARGEUR - 2, 2), (LARGEUR - 2, HAUTEUR - 2), (2, HAUTEUR - 2)], "net": "GND"})
    # Trous de fixation
    d_fix = 3.2 if revision == "A" else 3.5
    for x, y in [(4, 4), (LARGEUR - 4, 4), (4, HAUTEUR - 4), (LARGEUR - 4, HAUTEUR - 4)]:
        c.trous_npth.append((x, y, d_fix))
    # Sérigraphie de la carte
    c.serigraphie["top"].extend(texte_en_traits(f"TYPON DEMO REV {revision}", LARGEUR / 2, 36.2, 1.4, centre=True))
    c.serigraphie["top"].extend(texte_en_traits("SENSOR MODULE", LARGEUR / 2, 3.0, 1.0, centre=True))
    c.serigraphie["bottom"].extend(texte_en_traits("MADE WITH TYPON", LARGEUR / 2, 6.0, 1.0, centre=True, miroir=True))
    return c


# ---------------------------------------------------------------------------------------------------------------------
# Écriture Gerber X2/X3
# ---------------------------------------------------------------------------------------------------------------------
def coord(v):
    return str(int(round(v * 1_000_000)))


class Gerber:
    def __init__(self, fonction, polarite="Positive", revision="A"):
        self.lignes = [
            f"%TF.GenerationSoftware,Typon,generer_exemples.py,1.0*%",
            f"%TF.CreationDate,{DATE}*%",
            f"%TF.ProjectId,Typon-demo,7970e0f0-5d1a-4e0c-9b7e-74796f6e0001,{revision}*%",
            f"%TF.SameCoordinates,Original*%",
            f"%TF.FileFunction,{fonction}*%",
            f"%TF.FilePolarity,{polarite}*%",
            "%FSLAX46Y46*%",
            "G04 Typon : carte d'exemple fictive (licence MIT)*",
            "%MOMM*%",
            "%LPD*%",
            "G01*",
            "G75*",
        ]
        self.ouvertures = {}
        self.suivante = 10
        self.courante = None
        self.attrs_objet = {}

    def ouverture(self, forme, params, fonction=None):
        cle = (forme, params, fonction)
        if cle not in self.ouvertures:
            n = self.suivante
            self.suivante += 1
            if fonction:
                self.lignes.append(f"%TA.AperFunction,{fonction}*%")
            self.lignes.append(f"%ADD{n}{forme},{params}*%")
            if fonction:
                self.lignes.append("%TD*%")
            self.ouvertures[cle] = n
        n = self.ouvertures[cle]
        if self.courante != n:
            self.lignes.append(f"D{n}*")
            self.courante = n

    def objet(self, **attrs):
        nouveaux = {k: v for k, v in attrs.items() if v}
        if nouveaux != self.attrs_objet:
            if self.attrs_objet:
                self.lignes.append("%TD*%")
            for k, v in nouveaux.items():
                self.lignes.append(f"%TO.{k},{v}*%")
            self.attrs_objet = nouveaux

    def flash(self, x, y):
        self.lignes.append(f"X{coord(x)}Y{coord(y)}D03*")

    def trait(self, points):
        self.lignes.append(f"X{coord(points[0][0])}Y{coord(points[0][1])}D02*")
        for p in points[1:]:
            self.lignes.append(f"X{coord(p[0])}Y{coord(p[1])}D01*")

    def region(self, contour, sombre=True, fonction="Conductor"):
        self.lignes.append("%LPD*%" if sombre else "%LPC*%")
        self.lignes.append(f"%TA.AperFunction,{fonction}*%" if sombre else "%TA.AperFunction,Other,Clearance*%")
        self.lignes.append("G36*")
        self.trait(list(contour) + [contour[0]])
        self.lignes.append("G37*")
        self.lignes.append("%TD.AperFunction*%")
        if not sombre:
            self.lignes.append("%LPD*%")

    def texte(self):
        if self.attrs_objet:
            self.lignes.append("%TD*%")
        return "\n".join(self.lignes + ["M02*"]) + "\n"


def forme_pastille(p, marge=0.0):
    w, h = p["w"] + 2 * marge, p["h"] + 2 * marge
    if p["rot"] % 180 == 90:
        w, h = h, w
    if p["forme"] == "rect":
        return "R", f"{w:.3f}X{h:.3f}"
    if p["forme"] == "oval":
        return "O", f"{w:.3f}X{h:.3f}"
    return "C", f"{w:.3f}"


def contour_carte(rayon=3.0):
    pts = []
    for cx, cy, a0 in [(LARGEUR - rayon, rayon, -90), (LARGEUR - rayon, HAUTEUR - rayon, 0), (rayon, HAUTEUR - rayon, 90), (rayon, rayon, 180)]:
        for k in range(0, 91, 10):
            a = math.radians(a0 + k)
            pts.append((cx + rayon * math.cos(a), cy + rayon * math.sin(a)))
    return pts


def gerber_cuivre(c, couche, fonction):
    g = Gerber(fonction, revision=c.revision)
    if couche in ("gnd", "pwr", "bottom"):
        zone = next(z for z in c.zones if z["couche"] == couche)
        g.objet(N=zone["net"])
        g.region(zone["contour"])
        g.objet()
        # Dégagements (polarité claire) autour des éléments d'un autre net
        elements = []
        for p in c.pastilles:
            if p["couche"] in ("all", couche):
                elements.append((p["x"], p["y"], max(p["w"], p["h"]) / 2 + 0.4, p["net"]))
        for v in c.vias:
            elements.append((v["x"], v["y"], v["d"] / 2 + 0.4 if couche == "bottom" else v["trou"] / 2 + 0.45, v["net"]))
        for x, y, d in c.trous_npth:
            elements.append((x, y, d / 2 + 0.5, None))
        for t in c.pistes:
            if t["couche"] == couche and t["net"] != zone["net"]:
                for a, b in zip(t["points"], t["points"][1:]):
                    elements.append(("piste", a, b, t["largeur"] / 2 + 0.35))
        for e in elements:
            if e[0] == "piste":
                _, a, b, r = e
                g.lignes.append("%LPC*%")
                g.ouverture("C", f"{2 * r:.3f}")
                g.trait([a, b])
                g.lignes.append("%LPD*%")
            else:
                x, y, r, net = e
                if net == zone["net"] and couche != "bottom":
                    continue
                if net == zone["net"]:
                    continue
                pts = [(x + r * math.cos(math.radians(k * 15)), y + r * math.sin(math.radians(k * 15))) for k in range(24)]
                g.region(pts, sombre=False)
    for t in c.pistes:
        if t["couche"] == couche:
            g.objet(N=t["net"])
            g.ouverture("C", f"{t['largeur']:.3f}", "Conductor")
            g.trait(t["points"])
    for p in c.pastilles:
        if p["couche"] in ("all", couche) and couche in ("top", "bottom") or p["couche"] == "all" and couche in ("gnd", "pwr") and p["net"] == ("GND" if couche == "gnd" else "3V3"):
            fonction_p = "ComponentPad" if p["trou"] else "SMDPad,CuDef"
            forme, params = forme_pastille(p)
            if couche in ("gnd", "pwr"):
                forme, params = "C", f"{max(p['w'], p['h']):.3f}"
            g.objet(N=p["net"], P=f"{p['comp']},{p['broche']}", C=p["comp"])
            g.ouverture(forme, params, fonction_p)
            g.flash(p["x"], p["y"])
    for v in c.vias:
        g.objet(N=v["net"])
        g.ouverture("C", f"{v['d']:.3f}", "ViaPad")
        g.flash(v["x"], v["y"])
    return g.texte()


def gerber_masque(c, cote):
    g = Gerber(f"Soldermask,{'Top' if cote == 'top' else 'Bot'}", polarite="Negative", revision=c.revision)
    for p in c.pastilles:
        if p["couche"] in ("all", cote):
            forme, params = forme_pastille(p, 0.05)
            g.ouverture(forme, params, "SMDPad,CuDef" if not p["trou"] else "ComponentPad")
            g.flash(p["x"], p["y"])
    return g.texte()


def gerber_pate(c, cote):
    g = Gerber(f"Paste,{'Top' if cote == 'top' else 'Bot'}", revision=c.revision)
    for p in c.pastilles:
        if p["couche"] == cote:
            forme, params = forme_pastille(p, -0.05)
            g.ouverture(forme, params, "SMDPad,CuDef")
            g.flash(p["x"], p["y"])
    return g.texte()


def gerber_serigraphie(c, cote):
    g = Gerber(f"Legend,{'Top' if cote == 'top' else 'Bot'}", revision=c.revision)
    g.ouverture("C", "0.150", "Other,Legend")
    for a, b in c.serigraphie[cote]:
        g.trait([a, b])
    if cote == "top":
        # Code-barres du numéro de série : des rectangles, comme l'écrivent les logiciels de CAO en Gerber.
        g.objet()
        for x0, y0, x1, y1 in barres_code39(*CODE_BARRES):
            g.region([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], fonction="Other,Legend")
    return g.texte()


def gerber_contour(c):
    g = Gerber("Profile,NP", revision=c.revision)
    g.ouverture("C", "0.100", "Profile")
    pts = contour_carte()
    g.trait(pts + [pts[0]])
    return g.texte()


def gerber_composants(c, cote):
    g = Gerber(f"Component,L{'1' if cote == 'top' else '4'},{'Top' if cote == 'top' else 'Bot'}", revision=c.revision)
    for comp in c.composants:
        if comp["cote"] != cote:
            continue
        g.lignes.append("%TA.AperFunction,ComponentMain*%")
        g.lignes.append("%ADD{0}C,0.300*%".format(g.suivante))
        g.lignes.append("%TD*%")
        n = g.suivante
        g.suivante += 1
        g.lignes.append(f"D{n}*")
        g.courante = n
        g.objet(C=comp["ref"], CRot=f"{comp['rot']:.1f}", CMfr=comp["fabricant"], CMPN=comp["mpn"], CVal=comp["valeur"], CFtp=comp["boitier"], CMnt="TH" if comp["boitier"].startswith("PinHeader") else "SMD")
        g.flash(comp["x"], comp["y"])
    return g.texte()


def excellon(c, plated):
    """Perçages au format XNC (Excellon normalisé par Ucamco), avec attributs."""
    trous = []
    if plated:
        for p in c.pastilles:
            if p["trou"]:
                trous.append((p["trou"], p["x"], p["y"], "ComponentDrill"))
        for v in c.vias:
            trous.append((v["trou"], v["x"], v["y"], "ViaDrill"))
    else:
        for x, y, d in c.trous_npth:
            trous.append((d, x, y, "MechanicalDrill"))
    outils = sorted({(d, f) for d, _, _, f in trous})
    lignes = [
        "M48",
        "; DRILL file {Typon generer_exemples.py} date 2026-11-08",
        "; #@! TF.GenerationSoftware,Typon,generer_exemples.py,1.0",
        f"; #@! TF.FileFunction,{'Plated,1,4,PTH' if plated else 'NonPlated,1,4,NPTH'}",
        "FMAT,2",
        "METRIC",
    ]
    for i, (d, f) in enumerate(outils, 1):
        lignes.append(f"; #@! TA.AperFunction,{'Plated' if plated else 'NonPlated'},{'PTH' if plated else 'NPTH'},{f}")
        lignes.append(f"T{i}C{d:.3f}")
    lignes += ["%", "G90", "G05"]
    for i, (d, f) in enumerate(outils, 1):
        lignes.append(f"T{i}")
        for dd, x, y, ff in trous:
            if (dd, ff) == (d, f):
                lignes.append(f"X{x:.3f}Y{y:.3f}")
    lignes.append("M30")
    return "\n".join(lignes) + "\n"


def job(c, fichiers):
    return json.dumps({
        "Header": {"GenerationSoftware": {"Vendor": "Typon", "Application": "generer_exemples.py", "Version": "1.0"}, "CreationDate": DATE},
        "GeneralSpecs": {
            "ProjectId": {"Name": "Typon-demo", "GUID": "7970e0f0-5d1a-4e0c-9b7e-74796f6e0001", "Revision": c.revision},
            "Size": {"X": LARGEUR, "Y": HAUTEUR},
            "LayerNumber": 4,
            "BoardThickness": 1.6,
            "Finish": "ENIG",
        },
        "DesignRules": [{"Layers": "Outer", "PadToPad": 0.2, "PadToTrack": 0.2, "TrackToTrack": 0.2, "MinLineWidth": 0.25}],
        "FilesAttributes": [{"Path": nom, "FileFunction": f, "FilePolarity": "Positive"} for nom, f in fichiers],
        "MaterialStackup": [
            {"Type": "Legend", "Name": "Top Silk Screen"},
            {"Type": "SolderMask", "Thickness": 0.01, "Name": "Top Solder Mask"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "F.Cu"},
            {"Type": "Dielectric", "Thickness": 0.2, "Material": "FR4", "Name": "Prepreg 1"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "In1.Cu (GND)"},
            {"Type": "Dielectric", "Thickness": 1.065, "Material": "FR4", "Name": "Core"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "In2.Cu (3V3)"},
            {"Type": "Dielectric", "Thickness": 0.2, "Material": "FR4", "Name": "Prepreg 2"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "B.Cu"},
            {"Type": "SolderMask", "Thickness": 0.01, "Name": "Bottom Solder Mask"},
            {"Type": "Legend", "Name": "Bottom Silk Screen"},
        ],
    }, indent=2, ensure_ascii=False)


def archive_gerber(c):
    p = f"typon-demo-rev{c.revision}"
    fichiers = {
        f"{p}-F_Cu.gbr": (gerber_cuivre(c, "top", "Copper,L1,Top"), "Copper,L1,Top"),
        f"{p}-In1_Cu.gbr": (gerber_cuivre(c, "gnd", "Copper,L2,Inr"), "Copper,L2,Inr"),
        f"{p}-In2_Cu.gbr": (gerber_cuivre(c, "pwr", "Copper,L3,Inr"), "Copper,L3,Inr"),
        f"{p}-B_Cu.gbr": (gerber_cuivre(c, "bottom", "Copper,L4,Bot"), "Copper,L4,Bot"),
        f"{p}-F_Mask.gbr": (gerber_masque(c, "top"), "Soldermask,Top"),
        f"{p}-B_Mask.gbr": (gerber_masque(c, "bottom"), "Soldermask,Bot"),
        f"{p}-F_Paste.gbr": (gerber_pate(c, "top"), "Paste,Top"),
        f"{p}-F_Silkscreen.gbr": (gerber_serigraphie(c, "top"), "Legend,Top"),
        f"{p}-B_Silkscreen.gbr": (gerber_serigraphie(c, "bottom"), "Legend,Bot"),
        f"{p}-Edge_Cuts.gbr": (gerber_contour(c), "Profile,NP"),
        f"{p}-pnp_top.gbr": (gerber_composants(c, "top"), "Component,L1,Top"),
        f"{p}-PTH.drl": (excellon(c, True), "Plated,1,4,PTH"),
        f"{p}-NPTH.drl": (excellon(c, False), "NonPlated,1,4,NPTH"),
    }
    tampon = io.BytesIO()
    with zipfile.ZipFile(tampon, "w", zipfile.ZIP_DEFLATED) as z:
        for nom, (texte, _) in fichiers.items():
            info = zipfile.ZipInfo(nom, date_time=(2026, 11, 8, 10, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, texte)
        info = zipfile.ZipInfo(f"{p}-job.gbrjob", date_time=(2026, 11, 8, 10, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, job(c, [(n, f) for n, (_, f) in fichiers.items() if not n.endswith(".drl")]))
    return tampon.getvalue()


# ---------------------------------------------------------------------------------------------------------------------
# Écriture ODB++ 8.1
# ---------------------------------------------------------------------------------------------------------------------
class Features:
    """Fichier « features » d'une couche ODB++ (unités mm, symboles en microns)."""

    def __init__(self):
        self.symboles = []
        self.attributs = [".pad_usage", ".smd", ".string"]
        self.textes = []
        self.lignes = []
        self.n = 0  # nombre d'éléments (P, L, S…) : leur numéro sert à relier les éléments aux nets (eda/data)

    def symbole(self, nom):
        if nom not in self.symboles:
            self.symboles.append(nom)
        return self.symboles.index(nom)

    def pastille(self, x, y, symbole, attrs="", negatif=False):
        self.lignes.append(f"P {x:.4f} {y:.4f} {self.symbole(symbole)} {'N' if negatif else 'P'} 0 0{(';' + attrs) if attrs else ''}")
        self.n += 1
        return self.n - 1

    def trait(self, a, b, largeur, negatif=False):
        self.lignes.append(f"L {a[0]:.4f} {a[1]:.4f} {b[0]:.4f} {b[1]:.4f} {self.symbole(f'r{largeur * 1000:.1f}')} {'N' if negatif else 'P'} 0")
        self.n += 1
        return self.n - 1

    def surface(self, contour, negatif=False):
        self.lignes.append(f"S {'N' if negatif else 'P'} 0")
        self.lignes.append(f"OB {contour[0][0]:.4f} {contour[0][1]:.4f} I")
        for x, y in contour[1:] + contour[:1]:
            self.lignes.append(f"OS {x:.4f} {y:.4f}")
        self.lignes.append("OE")
        self.lignes.append("SE")
        self.n += 1
        return self.n - 1

    def texte(self):
        tete = ["#", "#Units", "#", "UNITS=MM", "#", f"#Num Features", "#", f"F {self.n}", "", "#", "#Feature symbol names", "#"]
        tete += [f"${i} {s}" for i, s in enumerate(self.symboles)]
        tete += ["", "#", "#Feature attribute names", "#"] + [f"@{i} {a}" for i, a in enumerate(self.attributs)]
        tete += ["", "#", "#Layer features", "#"]
        return "\n".join(tete + self.lignes) + "\n"


def symbole_pastille(p, marge=0.0, rond=False):
    w, h = p["w"] + 2 * marge, p["h"] + 2 * marge
    if rond:
        return f"r{max(w, h) * 1000:.1f}"
    if p["forme"] == "rect":
        return f"rect{w * 1000:.1f}x{h * 1000:.1f}"
    if p["forme"] == "oval":
        return f"oval{w * 1000:.1f}x{h * 1000:.1f}"
    return f"r{w * 1000:.1f}"


def odb(c):
    racine = "typon-demo-reva"
    fichiers = {}
    couches = [
        ("comp_+_top", "COMPONENT", "POSITIVE"),
        ("spt", "SOLDER_PASTE", "POSITIVE"),
        ("smt", "SOLDER_MASK", "POSITIVE"),
        ("sst", "SILK_SCREEN", "POSITIVE"),
        ("top", "SIGNAL", "POSITIVE"),
        ("gnd", "POWER_GROUND", "POSITIVE"),
        ("pwr", "POWER_GROUND", "POSITIVE"),
        ("bottom", "SIGNAL", "POSITIVE"),
        ("ssb", "SILK_SCREEN", "POSITIVE"),
        ("smb", "SOLDER_MASK", "POSITIVE"),
        ("drill", "DRILL", "POSITIVE"),
        ("npth", "DRILL", "POSITIVE"),
    ]
    matrice = ["STEP {", "   COL=1", "   NAME=PCB", "}", ""]
    for i, (nom, type_, pol) in enumerate(couches, 1):
        debut, fin = ("top", "bottom") if type_ == "DRILL" else ("", "")
        matrice += ["LAYER {", f"   ROW={i}", "   CONTEXT=BOARD", f"   TYPE={type_}", f"   NAME={nom.upper()}", f"   POLARITY={pol}", f"   START_NAME={debut.upper()}", f"   END_NAME={fin.upper()}", "}", ""]
    fichiers["matrix/matrix"] = "\n".join(matrice)
    fichiers["misc/info"] = "\n".join(["JOB_NAME=typon-demo-reva", "UNITS=MM", "ODB_VERSION_MAJOR=8", "ODB_VERSION_MINOR=1", "ODB_SOURCE=Typon generer_exemples.py", "CREATION_DATE=20261108.100000", "SAVE_DATE=20261108.100000", "SAVE_APP=Typon generer_exemples.py 1.0", "SAVE_USER=typon"]) + "\n"
    fichiers["steps/pcb/stephdr"] = "UNITS=MM\nX_DATUM=0\nY_DATUM=0\nX_ORIGIN=0\nY_ORIGIN=0\n"
    # Profil
    prof = Features()
    prof.surface(contour_carte())
    fichiers["steps/pcb/profile"] = prof.texte()
    # Police
    police = ["XSIZE 0.040000", "YSIZE 0.060000", "OFFSET 0.020000", ""]
    for ch, traits in POLICE.items():
        police.append(f"CHAR {ch}")
        for ligne in traits:
            for (x1, y1), (x2, y2) in zip(ligne, ligne[1:]):
                police.append(f"LINE {x1 * 0.01:.6f} {y1 * 0.01:.6f} {x2 * 0.01:.6f} {y2 * 0.01:.6f} P R 0.006000")
        police.append("ECHAR")
    fichiers["fonts/standard"] = "\n".join(police) + "\n"
    # Couches cuivre, avec numéros d'éléments par net (pour eda/data)
    fid = {}  # net -> [(index couche eda, index élément)]
    lyr = [n for n, _, _ in couches]

    def noter(net, couche, i):
        fid.setdefault(net, []).append((lyr.index(couche), i))

    for couche in ("top", "gnd", "pwr", "bottom"):
        f = Features()
        if couche in ("gnd", "pwr", "bottom"):
            zone = next(z for z in c.zones if z["couche"] == couche)
            i = f.surface(zone["contour"])
            noter(zone["net"], couche, i)
            for p in c.pastilles:
                if p["couche"] in ("all", couche) and p["net"] != zone["net"]:
                    f.pastille(p["x"], p["y"], f"r{(max(p['w'], p['h']) + 0.8) * 1000:.1f}", negatif=True)
            for v in c.vias:
                if v["net"] != zone["net"]:
                    d = v["d"] + 0.8 if couche == "bottom" else v["trou"] + 0.9
                    f.pastille(v["x"], v["y"], f"r{d * 1000:.1f}", negatif=True)
            for x, y, d in c.trous_npth:
                f.pastille(x, y, f"r{(d + 1.0) * 1000:.1f}", negatif=True)
            for t in c.pistes:
                if t["couche"] == couche and t["net"] != zone["net"]:
                    for a, b in zip(t["points"], t["points"][1:]):
                        f.trait(a, b, t["largeur"] + 0.7, negatif=True)
        for t in c.pistes:
            if t["couche"] == couche:
                for a, b in zip(t["points"], t["points"][1:]):
                    noter(t["net"], couche, f.trait(a, b, t["largeur"]))
        for p in c.pastilles:
            visible = p["couche"] in ("all", couche) and couche in ("top", "bottom") or p["couche"] == "all" and couche in ("gnd", "pwr") and p["net"] == ("GND" if couche == "gnd" else "3V3")
            if visible:
                rond = couche in ("gnd", "pwr")
                i = f.pastille(p["x"], p["y"], symbole_pastille(p, rond=rond) if p["rot"] % 180 == 0 else symbole_pastille({**p, "w": p["h"], "h": p["w"]}, rond=rond), "0=0" + (",1" if not p["trou"] else ""))
                noter(p["net"], couche, i)
        for v in c.vias:
            noter(v["net"], couche, f.pastille(v["x"], v["y"], f"r{v['d'] * 1000:.1f}", "0=1"))
        fichiers[f"steps/pcb/layers/{couche}/features"] = f.texte()
    # Masques, pâte, sérigraphie
    for nom, cote, marge in (("smt", "top", 0.05), ("smb", "bottom", 0.05), ("spt", "top", -0.05)):
        f = Features()
        for p in c.pastilles:
            if p["couche"] == cote or (p["couche"] == "all" and nom != "spt"):
                pp = p if p["rot"] % 180 == 0 else {**p, "w": p["h"], "h": p["w"]}
                f.pastille(p["x"], p["y"], symbole_pastille(pp, marge))
        fichiers[f"steps/pcb/layers/{nom}/features"] = f.texte()
    for nom, cote in (("sst", "top"), ("ssb", "bottom")):
        f = Features()
        for a, b in c.serigraphie[cote]:
            f.trait(a, b, 0.15)
        if cote == "top":
            # Numéro de série en code 39 (barre fine 0,13 mm, hauteur 2,4 mm, clé de contrôle, sans texte).
            x, y, texte, w, h = CODE_BARRES
            f.lignes.append(f"B {x:.4f} {y:.4f} UPC39 standard P 0 E {w} {h} N Y N N B '{texte}'")
            f.n += 1
        fichiers[f"steps/pcb/layers/{nom}/features"] = f.texte()
    # Perçages et outils
    for nom, plated in (("drill", True), ("npth", False)):
        f = Features()
        outils = {}
        if plated:
            for p in c.pastilles:
                if p["trou"]:
                    f.pastille(p["x"], p["y"], f"r{p['trou'] * 1000:.1f}")
                    outils[p["trou"]] = "PLATED"
            for v in c.vias:
                f.pastille(v["x"], v["y"], f"r{v['trou'] * 1000:.1f}")
                outils[v["trou"]] = "VIA"
        else:
            for x, y, d in c.trous_npth:
                f.pastille(x, y, f"r{d * 1000:.1f}")
                outils[d] = "NON_PLATED"
        fichiers[f"steps/pcb/layers/{nom}/features"] = f.texte()
        outils_txt = ["UNITS=MM", "THICKNESS=1600", ""]
        for i, (d, t) in enumerate(sorted(outils.items()), 1):
            outils_txt += ["TOOLS {", f"   NUM={i}", f"   TYPE={t}", "   MIN_TOL=0", "   MAX_TOL=0", f"   FINISH_SIZE={d * 1000:.1f}", f"   DRILL_SIZE={d * 1000:.1f}", "}", ""]
        fichiers[f"steps/pcb/layers/{nom}/tools"] = "\n".join(outils_txt)
    # eda/data : couches, nets, boîtiers
    boitiers = sorted({comp["boitier"] for comp in c.composants})
    eda = ["HDR Typon generer_exemples.py", "UNITS=MM", "LYR " + " ".join(lyr), "#"]
    for net in sorted(fid):
        eda.append(f"NET {net}")
        eda.append("SNT TRC")
        for couche, i in fid[net]:
            eda.append(f"FID C {couche} {i}")
    for b in boitiers:
        comp = next(x for x in c.composants if x["boitier"] == b)
        cw, ch = comp["corps"]
        eda.append(f"PKG {b} 1.27 {-cw / 2:.3f} {-ch / 2:.3f} {cw / 2:.3f} {ch / 2:.3f}")
        eda.append(f"RC {-cw / 2:.3f} {-ch / 2:.3f} {cw:.3f} {ch:.3f}")
        a = math.radians(-comp["rot"])
        for broche, px, py, _ in comp["broches"]:
            dx, dy = px - comp["x"], py - comp["y"]
            lx = dx * math.cos(a) - dy * math.sin(a)
            ly = dx * math.sin(a) + dy * math.cos(a)
            p = next(q for q in c.pastilles if q["comp"] == comp["ref"] and q["broche"] == broche)
            eda.append(f"PIN {broche} {'T' if p['trou'] else 'S'} {lx:.3f} {ly:.3f} 0 U U")
            eda.append(f"RC {lx - p['w'] / 2:.3f} {ly - p['h'] / 2:.3f} {p['w']:.3f} {p['h']:.3f}")
    fichiers["steps/pcb/eda/data"] = "\n".join(eda) + "\n"
    # Composants
    comp_txt = ["UNITS=MM", "#", "@0 .comp_mount_type", "&0 smt", "&1 thmt", "#"]
    for i, comp in enumerate(c.composants):
        pkg = boitiers.index(comp["boitier"])
        monte = 1 if comp["boitier"].startswith("PinHeader") else 0
        # ODB++ : rotation dans le sens horaire
        comp_txt.append(f"CMP {pkg} {comp['x']:.4f} {comp['y']:.4f} {(-comp['rot']) % 360:.1f} N {comp['ref']} {comp['mpn']};0={monte}")
        comp_txt.append(f"PRP VALUE '{comp['valeur']}'")
        comp_txt.append(f"PRP MANUFACTURER '{comp['fabricant']}'")
        for k, (broche, px, py, net) in enumerate(comp["broches"]):
            comp_txt.append(f"TOP {k} {px:.4f} {py:.4f} 0 N 0 0 {broche}")
        comp_txt.append("#")
    fichiers["steps/pcb/layers/comp_+_top/components"] = "\n".join(comp_txt) + "\n"
    fichiers["steps/pcb/layers/comp_+_top/features"] = Features().texte()
    tampon = io.BytesIO()
    with tarfile.open(fileobj=tampon, mode="w:gz", format=tarfile.USTAR_FORMAT) as t:
        for chemin, texte in sorted(fichiers.items()):
            donnees = texte.encode("utf-8")
            info = tarfile.TarInfo(f"{racine}/{chemin}")
            info.size = len(donnees)
            info.mtime = 1_793_000_000
            t.addfile(info, io.BytesIO(donnees))
    return tampon.getvalue()


def rs274d(c, style):
    """Cuivre et sérigraphie du dessus en RS-274D (sans format ni ouvertures), avec leur liste d'ouvertures séparée.
    style « eagle » : roue .whl en pouces, coordonnées 2.4 zéros de tête omis ; « protel » : liste .apr en mils,
    coordonnées 2.4 zéros de fin omis."""
    ouvertures = {}

    def code(forme, a, b=None):
        cle = (forme, round(a, 4), round(b if b is not None else a, 4))
        if cle not in ouvertures:
            ouvertures[cle] = 10 + len(ouvertures)
        return ouvertures[cle]

    def xy(x, y):
        def c1(v):
            n = int(round(v / 25.4 * 10000))
            if style == "eagle":
                return str(n)
            # zéros de fin omis : 6 chiffres (2.4) avec zéros de tête, puis les zéros de fin retirés
            s = f"{abs(n):06d}".rstrip("0") or "0"
            return ("-" if n < 0 else "") + s
        return f"X{c1(x)}Y{c1(y)}"

    fichiers = {}
    for couche, nom in (("cuivre", "F_Cu"), ("serigraphie", "F_Silkscreen")):
        lignes = ["G04 Typon : carte d'exemple fictive en RS-274D (licence MIT)*"]
        courant = None

        def choisir(n):
            nonlocal courant
            if n != courant:
                lignes.append(f"G54D{n}*")
                courant = n

        if couche == "cuivre":
            for t in c.pistes:
                if t["couche"] == "top":
                    choisir(code("round", t["largeur"]))
                    lignes.append(xy(*t["points"][0]) + "D02*")
                    for q in t["points"][1:]:
                        lignes.append(xy(*q) + "D01*")
            for p_ in c.pastilles:
                if p_["couche"] in ("all", "top"):
                    w, h = p_["w"], p_["h"]
                    if p_["rot"] % 180 == 90:
                        w, h = h, w
                    forme = {"rect": "rectangle", "oval": "oval"}.get(p_["forme"], "round")
                    choisir(code(forme, w, h))
                    lignes.append(xy(p_["x"], p_["y"]) + "D03*")
            for v in c.vias:
                choisir(code("round", v["d"]))
                lignes.append(xy(v["x"], v["y"]) + "D03*")
        else:
            choisir(code("round", 0.15))
            for a, b in c.serigraphie["top"]:
                lignes.append(xy(*a) + "D02*")
                lignes.append(xy(*b) + "D01*")
        lignes.append("M02*")
        fichiers[f"typon-demo-{nom}.{'cmp' if couche == 'cuivre' and style == 'eagle' else 'gtl' if couche == 'cuivre' else 'plc' if style == 'eagle' else 'gto'}"] = "\n".join(lignes) + "\n"
    if style == "eagle":
        roue = ["; Aperture wheel (Eagle style) - Typon demo"]
        for (forme, a, b), n in sorted(ouvertures.items(), key=lambda x: x[1]):
            if forme == "round":
                roue.append(f"D{n}\tround\t{a / 25.4:.4f}")
            else:
                roue.append(f"D{n}\t{forme}\t{a / 25.4:.4f} x {b / 25.4:.4f}")
        fichiers["typon-demo.whl"] = "\n".join(roue) + "\n"
    else:
        apr = ["Protel aperture list - dimensions in mils", ""]
        noms = {"round": "ROUNDED", "rectangle": "RECTANGULAR", "oval": "OBLONG"}
        for (forme, a, b), n in sorted(ouvertures.items(), key=lambda x: x[1]):
            apr.append(f"D{n}  {noms[forme]:<12}{a / 0.0254:9.3f}{b / 0.0254:9.3f}    0.000   0.000      0.0   LINE   0.00")
        fichiers["typon-demo.apr"] = "\n".join(apr) + "\n"
    tampon = io.BytesIO()
    with zipfile.ZipFile(tampon, "w", zipfile.ZIP_DEFLATED) as z:
        for nom, texte in fichiers.items():
            info = zipfile.ZipInfo(nom, date_time=(2026, 11, 9, 10, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, texte)
    return tampon.getvalue()


def main():
    os.makedirs(SORTIE, exist_ok=True)
    a = construire("A")
    b = construire("B")
    for nom, donnees in (("typon-demo-revA.zip", archive_gerber(a)), ("typon-demo-revB.zip", archive_gerber(b)), ("typon-demo-revA-odb.tgz", odb(a))):
        with open(os.path.join(SORTIE, nom), "wb") as f:
            f.write(donnees)
        print(f"{nom} : {len(donnees) / 1024:.0f} Ko")
    essais = os.path.join(RACINE, "core", "tests", "donnees")
    os.makedirs(essais, exist_ok=True)
    for style in ("eagle", "protel"):
        donnees = rs274d(a, style)
        with open(os.path.join(essais, f"rs274d-{style}.zip"), "wb") as f:
            f.write(donnees)
        print(f"core/tests/donnees/rs274d-{style}.zip : {len(donnees) / 1024:.0f} Ko")


if __name__ == "__main__":
    main()
