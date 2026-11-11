"""Cartes d'exemple de Typon : un nœud capteur IoT fictif à 4 couches (microcontrôleur QFN-32, USB-C, charge de
batterie Li-ion, capteur d'environnement, connecteur Qwiic, deux barrettes au pas de 2,54 mm), en Gerber X2/X3
(révisions A et B) et en ODB++ (révision A). Aucune donnée réelle : tout est dessiné ici. Licence MIT, comme Typon.

    pip install shapely          # calcul des zones de cuivre (plans de masse, dégagements, liaisons thermiques)
    python scripts/generer_exemples.py
    python scripts/verifier_exemple.py   # vérification indépendante des fichiers produits

Produit dans public/exemples/ :
    typon-demo-revA.zip      Gerber X2/X3, perçages XNC, fichier de job
    typon-demo-revB.zip      la révision B (quelques modifications à retrouver avec la comparaison)
    typon-demo-revA-odb.tgz  la révision A en ODB++ 8.1 (avec un code-barres de numéro de série)

et dans core/tests/donnees/ (essais du moteur) :
    rs274d-eagle.zip         cuivre (pistes et pastilles) et sérigraphie du dessus en Gerber RS-274D, roue Eagle
    rs274d-protel.zip        les mêmes avec une liste d'ouvertures Protel (.apr, mils, zéros de fin omis)

La carte respecte partout des règles de fabrication courantes (isolement ≥ 0,15 mm, anneau ≥ 0,15 mm, cuivre à
≥ 0,4 mm du contour…), sauf six défauts volontaires décrits dans docs/defauts-exemple.md et marqués « DÉFAUT » ici.
"""
import gzip
import io
import json
import math
import os
import tarfile
import zipfile

from shapely.geometry import LineString, Point, Polygon, box
from shapely.ops import unary_union

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SORTIE = os.path.join(RACINE, "public", "exemples")
DATE = "2026-11-08T10:00:00+01:00"
LARGEUR, HAUTEUR = 60.0, 38.0
RAYON_COINS = 3.0
QS = 8  # segments par quart de cercle des zones de cuivre

# Règles de conception de la carte (mm)
ISOLEMENT_ZONE = 0.3     # zone de cuivre ↔ autre net
GARDE_BORD = 0.5         # zone de cuivre ↔ contour et découpes
GARDE_NPTH = 0.6         # zone de cuivre ↔ trou non métallisé
ANTIPASTILLE = 0.45      # plans internes : dégagement autour des trous d'un autre net (au-delà du rayon du trou)
LARGEUR_ZONE_MIN = 0.25  # les parties plus étroites des zones sont supprimées
RAYON_THERMIQUE = 0.3    # écart de la liaison thermique autour d'une pastille traversante
BRANCHE_THERMIQUE = 0.4  # largeur des branches de la liaison thermique

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
    "_": [[(0, 0), (4, 0)]],
}


# Code 39 (comme le lit Typon) : 1 = élément large (3 modules), 0 = étroit ; caractère de début et de fin « * ».
CODE39 = dict(zip("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%*", [
    "000110100", "100100001", "001100001", "101100000", "000110001", "100110000", "001110000", "000100101", "100100100", "001100100",
    "100001001", "001001001", "101001000", "000011001", "100011000", "001011000", "000001101", "100001100", "001001100", "000011100",
    "100000011", "001000011", "101000010", "000010011", "100010010", "001010010", "000000111", "100000110", "001000110", "000010110",
    "110000001", "011000001", "111000000", "010010001", "110010000", "011010000", "010000101", "110000100", "011000100", "010101000",
    "010100010", "010001010", "000101010", "010010100"]))
ALPHABET39 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%"
CODE_BARRES = (33.6, 4.75, "TD-0001", 0.13, 2.2)  # x, y (coin inférieur gauche), texte, barre fine, hauteur (mm)


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


def largeur_texte(texte, hauteur):
    k = hauteur / 6.0
    return len(texte) * 6 * k - 2 * k


def texte_en_traits(texte, x, y, hauteur, centre=False, miroir=False, rot=0):
    """Traits (segments) d'un texte, en mm. Case de 4 × 6 unités ; avance de 6 unités. (x, y) : coin inférieur gauche,
    ou centre si `centre`. `rot` : 0 ou 90 (texte montant). `miroir` : texte de la face du dessous, écrit en miroir
    dans le fichier pour se lire à l'endroit quand on regarde la carte par-dessous."""
    k = hauteur / 6.0
    largeur = largeur_texte(texte, hauteur)
    u0, v0 = (-largeur / 2, -hauteur / 2) if centre else (0.0, 0.0)
    traits = []
    for i, ch in enumerate(texte.upper()):
        for ligne in POLICE.get(ch, []):
            pts = []
            for px, py in ligne:
                u = u0 + (i * 6 + px) * k
                v = v0 + py * k
                if miroir:
                    u = (2 * u0 + largeur) - u
                if rot == 90:
                    u, v = -v, u
                pts.append((x + u, y + v))
            for a, b in zip(pts, pts[1:]):
                traits.append((a, b))
    return traits


# ---------------------------------------------------------------------------------------------------------------------
# Modèle de la carte
# ---------------------------------------------------------------------------------------------------------------------
def tourner(dx, dy, rot):
    """Rotation d'un vecteur d'un multiple de 90° (sens trigonométrique)."""
    return {0: (dx, dy), 90: (-dy, dx), 180: (-dx, -dy), 270: (dy, -dx)}[rot % 360]


def forme_geo(forme, x, y, w, h, marge=0.0):
    """Géométrie shapely d'une pastille (rect, oval, rond) agrandie de `marge`."""
    w, h = w + 2 * marge, h + 2 * marge
    if forme == "rect":
        return box(x - w / 2, y - h / 2, x + w / 2, y + h / 2)
    if forme == "rond":
        return Point(x, y).buffer(w / 2, quad_segs=QS)
    r = min(w, h) / 2
    if w >= h:
        return LineString([(x - w / 2 + r, y), (x + w / 2 - r, y)]).buffer(r, quad_segs=QS)
    return LineString([(x, y - h / 2 + r), (x, y + h / 2 - r)]).buffer(r, quad_segs=QS)


class Carte:
    def __init__(self, revision):
        self.revision = revision
        self.pastilles = []  # dict : couche ("top" ou "all"), forme, x, y, w, h, net, comp, broche, trou, fonction, masque, pate
        self.pistes = []  # dict : couche, points, largeur, net
        self.vias = []  # dict : x, y, d, trou, net
        self.trous_npth = []  # (x, y, d)
        self.composants = []
        self.serigraphie = {"top": [], "bottom": []}  # (a, b, largeur)
        self.zones = []  # dict : couche, net, contour (Polygon), thermique (bool)
        self.coulages = {}  # couche -> [(net, Polygon)]
        self.decoupes = []  # contours (listes de points) des découpes internes
        self.nets = set()

    # Éléments
    def composant(self, ref, valeur, boitier, x, y, rot, broches, nets, mpn, fabricant, corps, hauteur, montage="SMD", marge=0.05, pate=True, fonction=None, seri=True, nomenclature=True):
        """broches : (numéro, dx, dy, forme, w, h, trou[, options]) en coordonnées du composant (rotation 0)."""
        c = {"ref": ref, "valeur": valeur, "boitier": boitier, "x": x, "y": y, "rot": rot, "mpn": mpn, "fabricant": fabricant, "corps": corps, "hauteur": hauteur, "montage": montage, "broches": [], "seri": seri, "nomenclature": nomenclature}
        for b in broches:
            num, dx, dy, forme, w, h, trou = b[:7]
            opts = b[7] if len(b) > 7 else {}
            ox, oy = tourner(dx, dy, rot)
            ww, hh = (h, w) if rot % 180 == 90 else (w, h)
            net = nets.get(num, "")
            f = opts.get("fonction", fonction or ("ComponentPad" if trou else "SMDPad,CuDef"))
            p = {"couche": "all" if trou else "top", "forme": forme, "x": x + ox, "y": y + oy, "w": ww, "h": hh, "net": net, "comp": ref, "broche": num, "trou": trou,
                 "fonction": f, "masque": opts.get("masque", marge), "pate": opts.get("pate", "auto" if pate and not trou else None), "lx": dx, "ly": dy, "lw": w, "lh": h}
            if p["pate"] not in (None, "auto"):
                p["pate"] = [tuple(x + u for u in tourner(a, b2, rot)) for a, b2 in p["pate"]]
            self.pastilles.append(p)
            c["broches"].append(p)
            if net:
                self.nets.add(net)
        self.composants.append(c)
        return c

    def p(self, ref, broche):
        for q in self.pastilles:
            if q["comp"] == ref and q["broche"] == str(broche):
                return (q["x"], q["y"])
        raise KeyError((ref, broche))

    def piste(self, couche, points, largeur, net):
        self.pistes.append({"couche": couche, "points": [tuple(map(float, q)) for q in points], "largeur": largeur, "net": net})
        self.nets.add(net)

    def via(self, x, y, net, d=0.6, trou=0.3):
        self.vias.append({"x": x, "y": y, "d": d, "trou": trou, "net": net})
        self.nets.add(net)

    # Géométries
    def contour(self):
        pts = []
        for cx, cy, a0 in [(LARGEUR - RAYON_COINS, RAYON_COINS, -90), (LARGEUR - RAYON_COINS, HAUTEUR - RAYON_COINS, 0), (RAYON_COINS, HAUTEUR - RAYON_COINS, 90), (RAYON_COINS, RAYON_COINS, 180)]:
            for k in range(0, 91, 10):
                a = math.radians(a0 + k)
                pts.append((round(cx + RAYON_COINS * math.cos(a), 4), round(cy + RAYON_COINS * math.sin(a), 4)))
        return pts

    def plaque(self):
        g = Polygon(self.contour())
        for d in self.decoupes:
            g = g.difference(Polygon(d))
        return g

    def cuivre_couche(self, couche):
        """Géométries (shapely) des pastilles, pistes et vias d'une couche externe."""
        elems = []
        for p in self.pastilles:
            if p["couche"] in ("all", couche):
                elems.append((p["net"], forme_geo(p["forme"], p["x"], p["y"], p["w"], p["h"]), p))
        for t in self.pistes:
            if t["couche"] == couche:
                elems.append((t["net"], LineString(t["points"]).buffer(t["largeur"] / 2, quad_segs=QS) if len(t["points"]) > 1 else Point(t["points"][0]).buffer(t["largeur"] / 2), t))
        for v in self.vias:
            elems.append((v["net"], Point(v["x"], v["y"]).buffer(v["d"] / 2, quad_segs=QS), v))
        return elems


def ouvrir(g, r):
    """Ouverture morphologique : supprime les parties plus étroites que 2 r."""
    return g.buffer(-r, quad_segs=QS).buffer(r, quad_segs=QS).intersection(g)


def polygones(g):
    if g.is_empty:
        return []
    if isinstance(g, Polygon):
        return [g]
    return [q for q in getattr(g, "geoms", []) if isinstance(q, Polygon) and not q.is_empty]


def nettoyer(poly):
    """Arrondit les coordonnées au dixième de micron (comme les fichiers) en gardant un polygone valide."""
    poly = poly.simplify(0.008)
    ext = [(round(x, 4), round(y, 4)) for x, y in poly.exterior.coords]
    trous = [[(round(x, 4), round(y, 4)) for x, y in i.coords] for i in poly.interiors]
    q = Polygon(ext, trous)
    return q if q.is_valid else q.buffer(0)


def couler(c):
    """Zones de cuivre : plans internes et zones de masse externes, avec dégagements, liaisons thermiques et sans
    parties isolées ni trop étroites."""
    zone_utile = Polygon(c.contour()).buffer(-GARDE_BORD, quad_segs=QS)
    for d in c.decoupes:
        zone_utile = zone_utile.difference(Polygon(d).buffer(GARDE_BORD, quad_segs=QS))
    for x, y, d in c.trous_npth:
        zone_utile = zone_utile.difference(Point(x, y).buffer(d / 2 + GARDE_NPTH, quad_segs=QS))
    mires = unary_union([Point(p["x"], p["y"]).buffer(p["w"] / 2 + 1.0, quad_segs=QS) for p in c.pastilles if p["fonction"].startswith("FiducialPad")])
    for couche in sorted({z["couche"] for z in c.zones}):
        resultat = []
        zones = [z for z in c.zones if z["couche"] == couche]
        externe = couche in ("top", "bottom")
        for z in zones:
            net = z["net"]
            base = zone_utile.intersection(z["contour"])
            if couche == "top":
                base = base.difference(mires)
            for autre in zones:
                if autre is not z:
                    base = base.difference(autre["contour"].buffer(0.25, quad_segs=QS))
            obstacles, branches, ancres = [], [], []
            if externe:
                for n, g, e in c.cuivre_couche(couche):
                    if n != net:
                        obstacles.append(g.buffer(ISOLEMENT_ZONE, quad_segs=QS))
                    elif isinstance(e, dict) and e.get("trou") and "comp" in e:
                        obstacles.append(g.buffer(RAYON_THERMIQUE, quad_segs=QS))
                        branches.append((e, g))
                    else:
                        ancres.append(g)
            else:
                for p in c.pastilles:
                    if p["trou"]:
                        if p["net"] == net:
                            g = forme_geo("rond", p["x"], p["y"], min(p["w"], p["h"]), 0)
                            obstacles.append(g.buffer(RAYON_THERMIQUE, quad_segs=QS))
                            branches.append((p, g))
                        else:
                            obstacles.append(Point(p["x"], p["y"]).buffer(p["trou"] / 2 + ANTIPASTILLE, quad_segs=QS))
                for v in c.vias:
                    if v["net"] == net:
                        ancres.append(Point(v["x"], v["y"]).buffer(v["trou"] / 2, quad_segs=QS))
                    else:
                        obstacles.append(Point(v["x"], v["y"]).buffer(v["trou"] / 2 + ANTIPASTILLE, quad_segs=QS))
            interdit = unary_union(obstacles) if obstacles else Polygon()
            coulee = base.difference(interdit)
            # Liaisons thermiques : quatre branches de la pastille vers la zone
            croix = []
            for e, g in branches:
                r = max(e["w"], e["h"]) / 2 + RAYON_THERMIQUE + 0.4
                bw = BRANCHE_THERMIQUE / 2
                croix.append(box(e["x"] - r, e["y"] - bw, e["x"] + r, e["y"] + bw))
                croix.append(box(e["x"] - bw, e["y"] - r, e["x"] + bw, e["y"] + r))
                ancres.append(g)
            if croix:
                interdit_autres = unary_union([g.buffer(ISOLEMENT_ZONE, quad_segs=QS) for n, g, e in (c.cuivre_couche(couche) if externe else []) if n != net] +
                                              ([] if externe else [Point(q["x"], q["y"]).buffer(q["trou"] / 2 + ANTIPASTILLE, quad_segs=QS) for q in c.pastilles if q["trou"] and q["net"] != net] +
                                               [Point(v["x"], v["y"]).buffer(v["trou"] / 2 + ANTIPASTILLE, quad_segs=QS) for v in c.vias if v["net"] != net]))
                coulee = coulee.union(unary_union(croix).intersection(base).difference(interdit_autres))
            coulee = ouvrir(coulee, LARGEUR_ZONE_MIN / 2)
            # Ne garder que les parties reliées au net de la zone
            for q in polygones(coulee):
                if q.area > 0.3 and any(q.intersects(a) for a in ancres):
                    resultat.append((net, nettoyer(q)))
        resultat.sort(key=lambda r: -r[1].area)
        c.coulages[couche] = resultat
    return c


# ---------------------------------------------------------------------------------------------------------------------
# Sérigraphie : contours des boîtiers (coupés autour des ouvertures du vernis) et repères placés sans chevauchement.
# ---------------------------------------------------------------------------------------------------------------------
SERI = 0.15  # largeur des traits de sérigraphie
GARDE_SERI = 0.15  # bord d'un trait de sérigraphie ↔ ouverture du vernis


def ouvertures_vernis(c, cote="top"):
    geos = []
    for p in c.pastilles:
        if p["masque"] is not None and p["couche"] in ("all", cote):
            geos.append(forme_geo(p["forme"], p["x"], p["y"], p["w"], p["h"], p["masque"]))
    return geos


def couper_segment(a, b, interdit):
    """Parties d'un segment hors de la zone interdite."""
    reste = LineString([a, b]).difference(interdit)
    morceaux = [reste] if isinstance(reste, LineString) else list(getattr(reste, "geoms", []))
    return [(m.coords[0], m.coords[-1]) for m in morceaux if not m.is_empty and m.length > 0.2]


def texte_seri(c, cote, texte, x, y, hauteur, rot=0, largeur=SERI):
    """Texte centré en (x, y) sur une sérigraphie (en miroir sur le dessous) ; renvoie son cadre."""
    for a, b in texte_en_traits(texte, x, y, hauteur, centre=True, miroir=cote == "bottom", rot=rot):
        c.serigraphie[cote].append((a, b, largeur))
    lt = largeur_texte(texte, hauteur)
    w, h = (lt, hauteur) if rot == 0 else (hauteur, lt)
    return box(x - w / 2 - largeur, y - h / 2 - largeur, x + w / 2 + largeur, y + h / 2 + largeur)


def zone_interdite_seri(c, cote):
    """Ouvertures du vernis (avec la garde), bord de la carte, découpes et trous non métallisés."""
    ouvertures = ouvertures_vernis(c, cote)
    return unary_union([o.buffer(GARDE_SERI + SERI / 2, quad_segs=4) for o in ouvertures] + [Polygon(c.contour()).buffer(5).difference(c.plaque().buffer(-0.45))]
                       + [Point(x, y).buffer(d / 2 + 0.3) for x, y, d in c.trous_npth])


def serigraphier(c, textes):
    """Sérigraphie du dessus : contours des boîtiers (coupés autour des ouvertures du vernis), repère de la broche 1,
    textes fixes (logo, étiquettes) puis repères des composants placés sans chevauchement."""
    interdit = zone_interdite_seri(c, "top")
    plaque_int = c.plaque().buffer(-0.5)
    occupe = [interdit]
    # Textes fixes : (texte, x, y, hauteur, rot, volontairement sur une pastille)
    for texte, x, y, hauteur, rot, defaut in textes:
        cadre = texte_seri(c, "top", texte, x, y, hauteur, rot)
        if not defaut and cadre.intersects(interdit):
            raise RuntimeError(f"texte « {texte} » sur une ouverture du vernis ou hors de la carte")
        occupe.append(cadre.buffer(0.1))
    # Contours des boîtiers
    for comp in c.composants:
        if not comp["seri"]:
            continue
        w, h = comp["corps"]
        if comp["rot"] % 180 == 90:
            w, h = h, w
        x0, y0, x1, y1 = comp["x"] - w / 2, comp["y"] - h / 2, comp["x"] + w / 2, comp["y"] + h / 2
        coins = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        for a, b in zip(coins, coins[1:] + coins[:1]):
            for m in couper_segment(a, b, interdit):
                c.serigraphie["top"].append((m[0], m[1], SERI))
                occupe.append(LineString(m).buffer(0.2))
        # Repère de la broche 1 (circuits intégrés et connecteurs) : un point près du coin de la broche 1
        if comp["ref"][0] in "UJQY" or comp["ref"].startswith("SW"):
            p1 = comp["broches"][0]
            dx, dy = p1["x"] - comp["x"], p1["y"] - comp["y"]
            for k in (1.0, 1.4, 1.8):
                px = comp["x"] + math.copysign(w / 2 + 0.35 * k, dx) if abs(dx) > 1e-6 else comp["x"]
                py = comp["y"] + math.copysign(h / 2 + 0.35 * k, dy) if abs(dy) > 1e-6 else comp["y"]
                point = Point(px, py).buffer(0.25)
                if not point.intersects(interdit) and plaque_int.contains(point):
                    c.serigraphie["top"].append(((px - 0.05, py), (px + 0.05, py), 0.25))
                    occupe.append(point.buffer(0.1))
                    break
    # Repères : autour du corps, à la première place libre (horizontaux de préférence)
    for comp in sorted(c.composants, key=lambda k: (not k.get("reperes"), -k["corps"][0] * k["corps"][1])):
        if not comp["seri"]:
            continue
        ref = comp["ref"]
        hauteur = 0.8 if len(ref) <= 3 else 0.7
        lt = largeur_texte(ref, hauteur)
        w, h = comp["corps"]
        if comp["rot"] % 180 == 90:
            w, h = h, w
        candidats = list(comp.get("reperes", []))
        for marge in (0.35, 0.6, 0.9, 1.3, 1.8):
            for rot in (0, 90):
                tw, th = (lt, hauteur) if rot == 0 else (hauteur, lt)
                candidats += [(comp["x"], comp["y"] + h / 2 + marge + th / 2, rot), (comp["x"], comp["y"] - h / 2 - marge - th / 2, rot),
                              (comp["x"] - w / 2 - marge - tw / 2, comp["y"], rot), (comp["x"] + w / 2 + marge + tw / 2, comp["y"], rot),
                              (comp["x"] - w / 2 - marge - tw / 2, comp["y"] + h / 2 + marge + th / 2, rot), (comp["x"] + w / 2 + marge + tw / 2, comp["y"] + h / 2 + marge + th / 2, rot),
                              (comp["x"] - w / 2 - marge - tw / 2, comp["y"] - h / 2 - marge - th / 2, rot), (comp["x"] + w / 2 + marge + tw / 2, comp["y"] - h / 2 - marge - th / 2, rot)]
        place = None
        for cx, cy, rot in candidats:
            tw, th = (lt, hauteur) if rot == 0 else (hauteur, lt)
            cadre = box(cx - tw / 2 - 0.1, cy - th / 2 - 0.1, cx + tw / 2 + 0.1, cy + th / 2 + 0.1)
            if plaque_int.contains(cadre) and not any(cadre.intersects(o) for o in occupe):
                place = (cx, cy, rot, cadre)
                break
        if place is None:
            raise RuntimeError(f"pas de place pour le repère {ref}")
        cx, cy, rot, cadre = place
        occupe.append(cadre.buffer(0.1))
        texte_seri(c, "top", ref, cx, cy, hauteur, rot)


def serigraphier_dessous(c, textes):
    """Sérigraphie du dessous : textes en miroir (nom des broches des barrettes, titre)."""
    interdit = zone_interdite_seri(c, "bottom")
    for texte, x, y, hauteur, rot in textes:
        cadre = texte_seri(c, "bottom", texte, x, y, hauteur, rot)
        if cadre.intersects(interdit):
            raise RuntimeError(f"texte « {texte} » du dessous sur une ouverture du vernis ou hors de la carte")


# ---------------------------------------------------------------------------------------------------------------------
# Écriture Gerber X2/X3
# ---------------------------------------------------------------------------------------------------------------------
def coord(v):
    return str(int(round(v * 1_000_000)))


class Gerber:
    def __init__(self, fonction, polarite="Positive", revision="A"):
        self.lignes = [
            "%TF.GenerationSoftware,Typon,generer_exemples.py,2.0*%",
            f"%TF.CreationDate,{DATE}*%",
            f"%TF.ProjectId,Typon-demo,7970e0f0-5d1a-4e0c-9b7e-74796f6e0001,{revision}*%",
            "%TF.SameCoordinates,Original*%",
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
                self.lignes.append("%TD.AperFunction*%")
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
        contour = list(contour)
        if contour[0] == contour[-1]:
            contour = contour[:-1]
        self.lignes.append("%LPD*%" if sombre else "%LPC*%")
        self.lignes.append(f"%TA.AperFunction,{fonction}*%")
        self.lignes.append("G36*")
        self.trait(contour + [contour[0]])
        self.lignes.append("G37*")
        self.lignes.append("%TD.AperFunction*%")
        if not sombre:
            self.lignes.append("%LPD*%")

    def texte(self):
        if self.attrs_objet:
            self.lignes.append("%TD*%")
        return "\n".join(self.lignes + ["M02*"]) + "\n"


def ouverture_pastille(p, marge=0.0, rond=False):
    w, h = p["w"] + 2 * marge, p["h"] + 2 * marge
    if rond or p["forme"] == "rond":
        return "C", f"{max(w, h):.4f}"
    if p["forme"] == "rect":
        return "R", f"{w:.4f}X{h:.4f}"
    return "O", f"{w:.4f}X{h:.4f}"


def pastilles_internes(c, couche):
    """Pastilles des trous traversants reliés au plan d'une couche interne (avec liaison thermique)."""
    nets = {z["net"] for z in c.zones if z["couche"] == couche}
    return [p for p in c.pastilles if p["trou"] and p["net"] in nets]


def gerber_cuivre(c, couche, fonction):
    g = Gerber(fonction, revision=c.revision)
    for net, poly in c.coulages.get(couche, []):
        g.objet(N=net)
        g.region(poly.exterior.coords)
        for trou in poly.interiors:
            g.region(trou.coords, sombre=False, fonction="Conductor")
    if couche in ("top", "bottom"):
        for t in c.pistes:
            if t["couche"] == couche:
                g.objet(N=t["net"])
                g.ouverture("C", f"{t['largeur']:.4f}", "Conductor")
                g.trait(t["points"])
        for p in c.pastilles:
            if p["couche"] in ("all", couche):
                forme, params = ouverture_pastille(p)
                g.objet(N=p["net"] or "N/C", P=f"{p['comp']},{p['broche']}", C=p["comp"])
                g.ouverture(forme, params, p["fonction"])
                g.flash(p["x"], p["y"])
        for v in c.vias:
            g.objet(N=v["net"])
            g.ouverture("C", f"{v['d']:.4f}", "ViaPad")
            g.flash(v["x"], v["y"])
    else:
        for p in pastilles_internes(c, couche):
            forme, params = ouverture_pastille(p, rond=True)
            params = f"{min(p['w'], p['h']):.4f}"
            g.objet(N=p["net"], P=f"{p['comp']},{p['broche']}", C=p["comp"])
            g.ouverture("C", params, "ComponentPad")
            g.flash(p["x"], p["y"])
    return g.texte()


def gerber_masque(c, cote):
    g = Gerber(f"Soldermask,{'Top' if cote == 'top' else 'Bot'}", polarite="Negative", revision=c.revision)
    for p in c.pastilles:
        if p["couche"] in ("all", cote) and p["masque"] is not None:
            forme, params = ouverture_pastille(p, p["masque"])
            g.ouverture(forme, params, p["fonction"])
            g.flash(p["x"], p["y"])
    return g.texte()


def pates(c, cote="top"):
    """Ouvertures du pochoir : (forme, x, y, w, h)."""
    r = []
    for p in c.pastilles:
        if p["couche"] != cote or p["pate"] is None:
            continue
        if p["pate"] == "auto":
            r.append((p["forme"], p["x"], p["y"], p["w"] - 0.1, p["h"] - 0.1))
        else:
            for x, y in p["pate"]:
                r.append(("rect", x, y, 1.1, 1.1))
    return r


def gerber_pate(c, cote):
    g = Gerber(f"Paste,{'Top' if cote == 'top' else 'Bot'}", revision=c.revision)
    for forme, x, y, w, h in pates(c, cote):
        f, params = ouverture_pastille({"forme": forme, "w": w, "h": h})
        g.ouverture(f, params, "SMDPad,CuDef")
        g.flash(x, y)
    return g.texte()


def gerber_serigraphie(c, cote):
    g = Gerber(f"Legend,{'Top' if cote == 'top' else 'Bot'}", revision=c.revision)
    for a, b, w in c.serigraphie[cote]:
        g.ouverture("C", f"{w:.4f}", "Other,Legend")
        g.trait([a, b])
    if cote == "top":
        # Code-barres du numéro de série : des rectangles, comme l'écrivent les logiciels de CAO en Gerber.
        g.objet()
        for x0, y0, x1, y1 in barres_code39(*CODE_BARRES):
            g.region([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], fonction="Other,Legend")
    return g.texte()


def gerber_contour(c):
    g = Gerber("Profile,NP", revision=c.revision)
    g.ouverture("C", "0.1000", "Profile")
    for pts in [c.contour()] + c.decoupes:
        g.trait(pts + [pts[0]])
    return g.texte()


def gerber_composants(c, cote):
    g = Gerber(f"Component,L{'1' if cote == 'top' else '4'},{'Top' if cote == 'top' else 'Bot'}", revision=c.revision)
    for comp in c.composants:
        if not comp["nomenclature"]:
            continue
        g.objet(C=comp["ref"], CRot=f"{comp['rot']:.1f}", CMfr=comp["fabricant"], CMPN=comp["mpn"], CVal=comp["valeur"], CFtp=comp["boitier"], CMnt=comp["montage"], CHgt=f"{comp['hauteur']:.2f}")
        g.ouverture("C", "0.3000", "ComponentMain")
        g.flash(comp["x"], comp["y"])
        w, h = comp["corps"]
        coins = [tourner(u, v, comp["rot"]) for u, v in ((-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2))]
        pts = [(comp["x"] + u, comp["y"] + v) for u, v in coins]
        g.ouverture("C", "0.1000", "ComponentOutline,Body")
        g.trait(pts + [pts[0]])
    return g.texte()


def trous_metallises(c):
    trous = [(p["trou"], p["x"], p["y"], "ComponentDrill") for p in c.pastilles if p["trou"]]
    trous += [(v["trou"], v["x"], v["y"], "ViaDrill") for v in c.vias]
    return trous


def excellon(c, plated):
    """Perçages au format XNC (Excellon normalisé par Ucamco), avec attributs."""
    trous = trous_metallises(c) if plated else [(d, x, y, "MechanicalDrill") for x, y, d in c.trous_npth]
    outils = sorted({(d, f) for d, _, _, f in trous})
    lignes = [
        "M48",
        "; DRILL file {Typon generer_exemples.py} date 2026-11-08",
        "; #@! TF.GenerationSoftware,Typon,generer_exemples.py,2.0",
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
        "Header": {"GenerationSoftware": {"Vendor": "Typon", "Application": "generer_exemples.py", "Version": "2.0"}, "CreationDate": DATE},
        "GeneralSpecs": {
            "ProjectId": {"Name": "Typon-demo", "GUID": "7970e0f0-5d1a-4e0c-9b7e-74796f6e0001", "Revision": c.revision},
            "Size": {"X": LARGEUR, "Y": HAUTEUR},
            "LayerNumber": 4,
            "BoardThickness": 1.6,
            "Finish": "ENIG",
        },
        "DesignRules": [
            {"Layers": "Outer", "PadToPad": 0.15, "PadToTrack": 0.15, "TrackToTrack": 0.15, "MinLineWidth": 0.15, "TrackToRegion": 0.3, "RegionToRegion": 0.3},
            {"Layers": "Inner", "PadToPad": 0.2, "PadToTrack": 0.2, "TrackToTrack": 0.2, "TrackToRegion": 0.3, "RegionToRegion": 0.3},
        ],
        "FilesAttributes": [{"Path": nom, "FileFunction": f, "FilePolarity": "Negative" if f.startswith("Soldermask") else "Positive"} for nom, f in fichiers],
        "MaterialStackup": [
            {"Type": "Legend", "Color": "White", "Name": "Top Silk Screen"},
            {"Type": "SolderMask", "Color": "Green", "Thickness": 0.01, "Name": "Top Solder Mask"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "F.Cu"},
            {"Type": "Dielectric", "Thickness": 0.2, "Material": "FR4", "Name": "Prepreg 1", "Notes": "7628"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "In1.Cu (GND)"},
            {"Type": "Dielectric", "Thickness": 1.065, "Material": "FR4", "Name": "Core"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "In2.Cu (3V3, VSYS)"},
            {"Type": "Dielectric", "Thickness": 0.2, "Material": "FR4", "Name": "Prepreg 2", "Notes": "7628"},
            {"Type": "Copper", "Thickness": 0.035, "Name": "B.Cu"},
            {"Type": "SolderMask", "Color": "Green", "Thickness": 0.01, "Name": "Bottom Solder Mask"},
            {"Type": "Legend", "Color": "White", "Name": "Bottom Silk Screen"},
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

    def surface(self, contour, trous=(), negatif=False):
        def anneau(pts, genre):
            pts = list(pts)
            if pts[0] == pts[-1]:
                pts = pts[:-1]
            self.lignes.append(f"OB {pts[0][0]:.4f} {pts[0][1]:.4f} {genre}")
            for x, y in pts[1:] + pts[:1]:
                self.lignes.append(f"OS {x:.4f} {y:.4f}")
            self.lignes.append("OE")
        self.lignes.append(f"S {'N' if negatif else 'P'} 0")
        anneau(contour, "I")
        for t in trous:
            anneau(t, "H")
        self.lignes.append("SE")
        self.n += 1
        return self.n - 1

    def texte(self):
        tete = ["#", "#Units", "#", "UNITS=MM", "#", "#Num Features", "#", f"F {self.n}", "", "#", "#Feature symbol names", "#"]
        tete += [f"${i} {s}" for i, s in enumerate(self.symboles)]
        tete += ["", "#", "#Feature attribute names", "#"] + [f"@{i} {a}" for i, a in enumerate(self.attributs)]
        tete += ["", "#", "#Layer features", "#"]
        return "\n".join(tete + self.lignes) + "\n"


def symbole_pastille(forme, w, h, marge=0.0):
    w, h = w + 2 * marge, h + 2 * marge
    if forme == "rect":
        return f"rect{w * 1000:.1f}x{h * 1000:.1f}"
    if forme == "oval":
        return f"oval{w * 1000:.1f}x{h * 1000:.1f}"
    return f"r{max(w, h) * 1000:.1f}"


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
    fichiers["misc/info"] = "\n".join(["JOB_NAME=typon-demo-reva", "UNITS=MM", "ODB_VERSION_MAJOR=8", "ODB_VERSION_MINOR=1", "ODB_SOURCE=Typon generer_exemples.py", "CREATION_DATE=20261108.100000", "SAVE_DATE=20261108.100000", "SAVE_APP=Typon generer_exemples.py 2.0", "SAVE_USER=typon"]) + "\n"
    fichiers["misc/attrlist"] = "UNITS=MM\n.board_thickness=1.6\n"
    fichiers["steps/pcb/stephdr"] = "UNITS=MM\nX_DATUM=0\nY_DATUM=0\nX_ORIGIN=0\nY_ORIGIN=0\n"
    # Profil : contour de la carte et découpes
    prof = Features()
    prof.surface(c.contour(), c.decoupes)
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
        if net:
            fid.setdefault(net, []).append((lyr.index(couche), i))

    for couche in ("top", "gnd", "pwr", "bottom"):
        f = Features()
        for net, poly in c.coulages.get(couche, []):
            noter(net, couche, f.surface(list(poly.exterior.coords), [list(t.coords) for t in poly.interiors]))
        if couche in ("top", "bottom"):
            for t in c.pistes:
                if t["couche"] == couche:
                    for a, b in zip(t["points"], t["points"][1:]):
                        noter(t["net"], couche, f.trait(a, b, t["largeur"]))
            for p in c.pastilles:
                if p["couche"] in ("all", couche):
                    noter(p["net"], couche, f.pastille(p["x"], p["y"], symbole_pastille(p["forme"], p["w"], p["h"]), "0=0" + (",1" if not p["trou"] else "")))
            for v in c.vias:
                noter(v["net"], couche, f.pastille(v["x"], v["y"], f"r{v['d'] * 1000:.1f}", "0=1"))
        else:
            for p in pastilles_internes(c, couche):
                noter(p["net"], couche, f.pastille(p["x"], p["y"], f"r{min(p['w'], p['h']) * 1000:.1f}", "0=0"))
        fichiers[f"steps/pcb/layers/{couche}/features"] = f.texte()
    # Masques, pâte, sérigraphie
    for nom, cote in (("smt", "top"), ("smb", "bottom")):
        f = Features()
        for p in c.pastilles:
            if p["couche"] in ("all", cote) and p["masque"] is not None:
                f.pastille(p["x"], p["y"], symbole_pastille(p["forme"], p["w"], p["h"], p["masque"]))
        fichiers[f"steps/pcb/layers/{nom}/features"] = f.texte()
    f = Features()
    for forme, x, y, w, h in pates(c, "top"):
        f.pastille(x, y, symbole_pastille(forme, w, h))
    fichiers["steps/pcb/layers/spt/features"] = f.texte()
    for nom, cote in (("sst", "top"), ("ssb", "bottom")):
        f = Features()
        for a, b, w in c.serigraphie[cote]:
            f.trait(a, b, w)
        if cote == "top":
            # Numéro de série en code 39 (barre fine 0,13 mm, clé de contrôle, sans texte).
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
                    outils[p["trou"]] = outils.get(p["trou"], "PLATED")
            for v in c.vias:
                f.pastille(v["x"], v["y"], f"r{v['trou'] * 1000:.1f}")
                outils[v["trou"]] = "VIA" if outils.get(v["trou"], "VIA") == "VIA" else "PLATED"
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
    composants = [comp for comp in c.composants if comp["nomenclature"]]
    boitiers = sorted({comp["boitier"] for comp in composants})
    eda = ["HDR Typon generer_exemples.py", "UNITS=MM", "LYR " + " ".join(lyr), "#"]
    for net in sorted(fid):
        eda.append(f"NET {net}")
        eda.append("SNT TRC")
        for couche, i in fid[net]:
            eda.append(f"FID C {couche} {i}")
    for b in boitiers:
        comp = next(x for x in composants if x["boitier"] == b)
        cw, ch = comp["corps"]
        eda.append(f"PKG {b} 1.27 {-cw / 2:.3f} {-ch / 2:.3f} {cw / 2:.3f} {ch / 2:.3f}")
        eda.append(f"RC {-cw / 2:.3f} {-ch / 2:.3f} {cw:.3f} {ch:.3f}")
        for p in comp["broches"]:
            eda.append(f"PIN {p['broche']} {'T' if p['trou'] else 'S'} {p['lx']:.3f} {p['ly']:.3f} 0 U U")
            if p["forme"] == "rond":
                eda.append(f"CR {p['lx']:.3f} {p['ly']:.3f} {p['lw'] / 2:.3f}")
            else:
                eda.append(f"RC {p['lx'] - p['lw'] / 2:.3f} {p['ly'] - p['lh'] / 2:.3f} {p['lw']:.3f} {p['lh']:.3f}")
    fichiers["steps/pcb/eda/data"] = "\n".join(eda) + "\n"
    # Composants (hauteur en mm, rotation ODB++ dans le sens horaire)
    comp_txt = ["UNITS=MM", "#", "@0 .comp_mount_type", "@1 .comp_height", "&0 smt", "&1 thmt", "#"]
    for comp in composants:
        pkg = boitiers.index(comp["boitier"])
        monte = 1 if comp["montage"] == "TH" else 0
        comp_txt.append(f"CMP {pkg} {comp['x']:.4f} {comp['y']:.4f} {(-comp['rot']) % 360:.1f} N {comp['ref']} {comp['mpn']};0={monte},1={comp['hauteur']:.2f}")
        comp_txt.append(f"PRP VALUE '{comp['valeur']}'")
        comp_txt.append(f"PRP MANUFACTURER '{comp['fabricant']}'")
        for k, p in enumerate(comp["broches"]):
            comp_txt.append(f"TOP {k} {p['x']:.4f} {p['y']:.4f} 0 N 0 0 {p['broche']}")
        comp_txt.append("#")
    fichiers["steps/pcb/layers/comp_+_top/components"] = "\n".join(comp_txt) + "\n"
    fichiers["steps/pcb/layers/comp_+_top/features"] = Features().texte()
    tampon = io.BytesIO()
    # Date fixe dans l'en-tête gzip : l'archive est identique d'une génération à l'autre.
    with gzip.GzipFile(fileobj=tampon, mode="wb", mtime=1_793_000_000) as gz, tarfile.open(fileobj=gz, mode="w", format=tarfile.USTAR_FORMAT) as t:
        for chemin, texte in sorted(fichiers.items()):
            donnees = texte.encode("utf-8")
            info = tarfile.TarInfo(f"{racine}/{chemin}")
            info.size = len(donnees)
            info.mtime = 1_793_000_000
            t.addfile(info, io.BytesIO(donnees))
    return tampon.getvalue()


def rs274d(c, style):
    """Cuivre (pistes, pastilles, vias, sans les zones) et sérigraphie du dessus en RS-274D (sans format ni
    ouvertures), avec leur liste d'ouvertures séparée. style « eagle » : roue .whl en pouces, coordonnées 2.4 zéros
    de tête omis ; « protel » : liste .apr en mils, coordonnées 2.4 zéros de fin omis."""
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
                    forme = {"rect": "rectangle", "oval": "oval"}.get(p_["forme"], "round")
                    choisir(code(forme, p_["w"], p_["h"]) if forme != "round" else code("round", p_["w"]))
                    lignes.append(xy(p_["x"], p_["y"]) + "D03*")
            for v in c.vias:
                choisir(code("round", v["d"]))
                lignes.append(xy(v["x"], v["y"]) + "D03*")
        else:
            for a, b, w in c.serigraphie["top"]:
                choisir(code("round", w))
                lignes.append(xy(*a) + "D02*")
                lignes.append(xy(*b) + "D01*")
        lignes.append("M02*")
        fichiers[f"typon-demo-{nom}.{'cmp' if couche == 'cuivre' and style == 'eagle' else 'gtl' if couche == 'cuivre' else 'plc' if style == 'eagle' else 'gto'}"] = "\n".join(lignes) + "\n"
    if style == "eagle":
        roue = ["; Aperture wheel (Eagle style) - Typon demo"]
        for (forme, a, b), n in sorted(ouvertures.items(), key=lambda x: x[1]):
            if forme == "round":
                roue.append(f"D{n}\tround\t{a / 25.4:.5f}")
            else:
                roue.append(f"D{n}\t{forme}\t{a / 25.4:.5f} x {b / 25.4:.5f}")
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


# ---------------------------------------------------------------------------------------------------------------------
# Conception de la carte
# ---------------------------------------------------------------------------------------------------------------------
# Empreintes (coordonnées locales, rotation 0), d'après les empreintes KiCad courantes.
def passif(c, ref, x, y, rot, n1, n2, valeur, mpn, taille="0402", fabricant="Yageo", hauteur=None):
    dims = {"0402": (0.5, 0.55, 0.6, (1.0, 0.5), 0.35), "0603": (0.8, 0.8, 0.95, (1.6, 0.8), 0.45), "0805": (0.95, 1.0, 1.45, (2.0, 1.25), 1.25), "1206": (1.45, 1.0, 1.75, (3.2, 1.6), 0.85)}
    e, w, h, corps, ht = dims[taille]
    prefixe = {"R": "R", "C": "C", "D": "LED", "F": "Fuse"}[ref[0]]
    return c.composant(ref, valeur, f"{prefixe}_{taille}", x, y, rot, [("1", -e, 0, "rect", w, h, 0), ("2", e, 0, "rect", w, h, 0)], {"1": n1, "2": n2}, mpn, fabricant, corps, hauteur or ht)


def sod123(c, ref, x, y, rot, k, a, valeur, mpn, fabricant):
    return c.composant(ref, valeur, "D_SOD-123", x, y, rot, [("1", -1.65, 0, "rect", 0.9, 1.2, 0), ("2", 1.65, 0, "rect", 0.9, 1.2, 0)], {"1": k, "2": a}, mpn, fabricant, (2.7, 1.6), 1.1)


def sot23(c, ref, x, y, rot, nets, valeur, mpn, fabricant, broches=3):
    """SOT-23 (3 broches), SOT-23-5 ou SOT-23-6 : rangée des broches 1… en bas, les suivantes en haut, de droite à
    gauche."""
    if broches == 3:
        pos = [(-0.95, -1.1), (0.95, -1.1), (0, 1.1)]
    elif broches == 5:
        pos = [(-0.95, -1.1), (0, -1.1), (0.95, -1.1), (0.95, 1.1), (-0.95, 1.1)]
    else:
        pos = [(-0.95, -1.1), (0, -1.1), (0.95, -1.1), (0.95, 1.1), (0, 1.1), (-0.95, 1.1)]
    p = [(str(i + 1), dx, dy, "rect", 0.6, 1.05, 0) for i, (dx, dy) in enumerate(pos)]
    nom = "SOT-23" if broches == 3 else f"SOT-23-{broches}"
    return c.composant(ref, valeur, nom, x, y, rot, p, {str(i + 1): n for i, n in enumerate(nets)}, mpn, fabricant, (2.9, 1.6), 1.1)


def qfn32(c, ref, x, y, nets, valeur, mpn, fabricant):
    """QFN-32 5 × 5 mm, pas de 0,5 mm, pastille centrale 3,3 mm (4 vias thermiques, pâte en 4 fenêtres)."""
    p = []
    for i in range(8):
        p.append((str(1 + i), -2.45, 1.75 - i * 0.5, "rect", 0.8, 0.25, 0))
        p.append((str(9 + i), -1.75 + i * 0.5, -2.45, "rect", 0.25, 0.8, 0))
        p.append((str(17 + i), 2.45, -1.75 + i * 0.5, "rect", 0.8, 0.25, 0))
        p.append((str(25 + i), 1.75 - i * 0.5, 2.45, "rect", 0.25, 0.8, 0))
    p.append(("33", 0, 0, "rect", 3.3, 3.3, 0, {"pate": [(-0.8, -0.8), (0.8, -0.8), (-0.8, 0.8), (0.8, 0.8)]}))
    return c.composant(ref, valeur, "QFN-32-1EP_5x5mm_P0.5mm", x, y, 0, p, nets, mpn, fabricant, (5.0, 5.0), 0.9)


def lga8(c, ref, x, y, nets, valeur, mpn, fabricant):
    """LGA-8 2,5 × 2,5 mm (capteur d'environnement), pas de 0,65 mm."""
    p = []
    for i in range(4):
        p.append((str(1 + i), -1.025, 0.975 - i * 0.65, "rect", 0.5, 0.35, 0))
        p.append((str(5 + i), 1.025, -0.975 + i * 0.65, "rect", 0.5, 0.35, 0))
    return c.composant(ref, valeur, "Bosch_LGA-8_2.5x2.5mm_P0.65mm", x, y, 0, p, nets, mpn, fabricant, (2.5, 2.5), 0.93)


def quartz3225(c, ref, x, y, rot, nets, valeur, mpn, fabricant):
    p = [("1", -1.1, -0.85, "rect", 1.4, 1.15, 0), ("2", 1.1, -0.85, "rect", 1.4, 1.15, 0), ("3", 1.1, 0.85, "rect", 1.4, 1.15, 0), ("4", -1.1, 0.85, "rect", 1.4, 1.15, 0)]
    return c.composant(ref, valeur, "Crystal_SMD_3225-4Pin_3.2x2.5mm", x, y, rot, p, nets, mpn, fabricant, (3.2, 2.5), 0.8)


def bouton(c, ref, x, y, rot, a, b, valeur, mpn, fabricant):
    """Bouton tactile CMS 4,2 × 2,8 mm (C&K KMR2) : broches 1 et 2 à gauche, 3 et 4 à droite."""
    p = [("1", -2.05, 0.8, "rect", 0.9, 1.0, 0), ("2", -2.05, -0.8, "rect", 0.9, 1.0, 0), ("3", 2.05, 0.8, "rect", 0.9, 1.0, 0), ("4", 2.05, -0.8, "rect", 0.9, 1.0, 0)]
    return c.composant(ref, valeur, "SW_SPST_CK_KMR2", x, y, rot, p, {"1": a, "2": a, "3": b, "4": b}, mpn, fabricant, (4.2, 2.8), 1.9)


def barrette(c, ref, x, y, nets, valeur, mpn):
    """Barrette mâle 1 × N au pas de 2,54 mm, horizontale, broche 1 à gauche (centre de la barrette en x, y)."""
    n = len(nets)
    p = [(str(i + 1), (i - (n - 1) / 2) * 2.54, 0, "rect" if i == 0 else "rond", 1.7, 1.7, 1.0) for i in range(n)]
    return c.composant(ref, valeur, f"PinHeader_1x{n:02d}_P2.54mm_Vertical", x, y, 0, p, {str(i + 1): net for i, net in enumerate(nets)}, mpn, "Amphenol", (2.54 * n, 2.54), 2.5, montage="TH")


def usb_c(c, ref, x, y, nets):
    """Embase USB-C 2.0 à 16 contacts (HRO TYPE-C-31-M-12), ouverture vers la gauche (bord x = 0) ; (x, y) : centre
    du corps. Pastilles des contacts en colonne (pas de 0,5 mm), pattes du blindage métallisées, deux plots de
    centrage non métallisés."""
    contacts = [("A1B12", -3.25, 0.6), ("A4B9", -2.45, 0.6), ("B8", -1.75, 0.3), ("A5", -1.25, 0.3), ("B7", -0.75, 0.3), ("A6", -0.25, 0.3),
                ("A7", 0.25, 0.3), ("B6", 0.75, 0.3), ("A8", 1.25, 0.3), ("B5", 1.75, 0.3), ("B4A9", 2.45, 0.6), ("B1A12", 3.25, 0.6)]
    p = [(nom, 3.35, u, "rect", 1.1, w, 0, {"masque": 0.025}) for nom, u, w in contacts]
    for i, (dx, dy) in enumerate([(2.3, -4.32), (2.3, 4.32), (-1.75, -4.32), (-1.75, 4.32)]):
        p.append((f"S{i + 1}", dx, dy, "oval", 1.8 if dx > 0 else 2.0, 1.1, 0.7))
    comp = c.composant(ref, "USB-C", "USB_C_Receptacle_HRO_TYPE-C-31-M-12", x, y, 0, p, nets, "TYPE-C-31-M-12", "Korean Hroparts", (7.3, 8.94), 3.26)
    for dy in (-2.89, 2.89):
        c.trous_npth.append((x + 1.9, y + dy, 0.65))
    return comp


def jst_ph2(c, ref, x, y, nets):
    """Connecteur de batterie JST PH à 2 contacts, CMS, entrée par la gauche ; (x, y) : centre du corps."""
    p = [("1", 2.8, 1.0, "rect", 3.0, 1.0, 0), ("2", 2.8, -1.0, "rect", 3.0, 1.0, 0), ("MP1", -1.7, 3.35, "rect", 2.4, 1.6, 0), ("MP2", -1.7, -3.35, "rect", 2.4, 1.6, 0)]
    return c.composant(ref, "BAT", "JST_PH_S2B-PH-SM4-TB_1x02-1MP_P2.00mm_Horizontal", x, y, 0, p, nets, "S2B-PH-SM4-TB(LF)(SN)", "JST", (6.0, 7.95), 4.8)


def jst_sh4(c, ref, x, y, nets):
    """Connecteur Qwiic (JST SH à 4 contacts, CMS), entrée par la droite, broche 1 en haut ; (x, y) : centre du corps."""
    p = [(str(i + 1), -1.7, 1.5 - i, "rect", 1.55, 0.6, 0) for i in range(4)]
    p += [("MP1", 1.2, 2.8, "rect", 1.8, 1.2, 0), ("MP2", 1.2, -2.8, "rect", 1.8, 1.2, 0)]
    return c.composant(ref, "Qwiic", "JST_SH_SM04B-SRSS-TB_1x04-1MP_P1.00mm_Horizontal", x, y, 0, p, nets, "SM04B-SRSS-TB(LF)(SN)", "JST", (4.25, 6.0), 2.9)


def tag_connect(c, ref, x, y, nets):
    """Pastilles de programmation Tag-Connect TC2030-IDC-NL (sans composant monté) et ses trois trous de centrage."""
    p = []
    for i in range(3):
        p.append((str(2 * i + 1), -1.27 + i * 1.27, -0.635, "rond", 0.787, 0.787, 0, {"pate": None}))
        p.append((str(2 * i + 2), -1.27 + i * 1.27, 0.635, "rond", 0.787, 0.787, 0, {"pate": None}))
    comp = c.composant(ref, "SWD", "Tag-Connect_TC2030-IDC-NL_TestPad", x, y, 0, p, nets, "TC2030-IDC-NL", "Tag-Connect", (3.81, 2.06), 0.0, seri=False)
    for dx, dy in ((-2.54, 0), (2.54, 1.016), (2.54, -1.016)):
        c.trous_npth.append((x + dx, y + dy, 0.99))
    return comp


def mire(c, ref, x, y):
    return c.composant(ref, "Fiducial", "Fiducial_1mm_Mask2mm", x, y, 0, [("1", 0, 0, "rond", 1.0, 1.0, 0, {"masque": 0.5, "pate": None, "fonction": "FiducialPad,Local"})], {}, "", "", (1.0, 1.0), 0.0, seri=False)


SIG, ALIM, FORT = 0.2, 0.4, 0.5  # largeurs de piste : signal, alimentation, courant de charge


def construire(revision):
    """La carte : nœud capteur IoT alimenté par USB-C ou par une batterie Li-ion (chargeur intégré), avec un
    microcontrôleur STM32G0 (USB), un capteur BME280 isolé thermiquement par une découpe, un connecteur Qwiic, des
    pastilles de programmation Tag-Connect et deux barrettes au pas de 2,54 mm. Révision B : voir plus bas."""
    c = Carte(revision)
    B = revision == "B"
    T, D = "top", "bottom"
    piste = c.piste

    # Trous de fixation M2,5 et découpe d'isolation thermique en L autour du capteur
    for x, y in ((3.4, 3.4), (LARGEUR - 3.4, 3.4), (3.4, HAUTEUR - 3.4), (LARGEUR - 3.4, HAUTEUR - 3.4)):
        c.trous_npth.append((x, y, 2.7))
    fente = LineString([(45.5, 33.0), (45.5, 24.5), (54.0, 24.5)]).buffer(0.5, quad_segs=8, join_style="mitre")
    c.decoupes.append([(round(x, 4), round(y, 4)) for x, y in list(fente.exterior.coords)[:-1]])

    # ----------------------------------------------------------------------------------------------- Connecteurs
    usb_c(c, "J1", 3.65, 24.0, {"A1B12": "GND", "B1A12": "GND", "A4B9": "VBUS", "B4A9": "VBUS", "A5": "CC1", "B5": "CC2",
                                "A6": "USB_DP", "B6": "USB_DP", "A7": "USB_DM", "B7": "USB_DM", "S1": "GND", "S2": "GND", "S3": "GND", "S4": "GND"})
    jst_ph2(c, "J4", 4.0, 11.0, {"1": "VBAT", "2": "GND"})
    barrette(c, "J2", 30.0, 2.8, ["NRST", "3V3", "GND", "A0", "A1", "A2", "A3", "SCK", "MOSI", "MISO", "RX", "TX"], "Conn_01x12", "10129378-912001BLF")
    barrette(c, "J3", 30.0, HAUTEUR - 2.8, ["VBAT", "VUSB", "GND", "D5", "D6", "D9", "D10", "SDA", "SCL", "3V3"], "Conn_01x10", "10129378-910001BLF")
    jst_sh4(c, "J5", 55.7, 18.5, {"1": "GND", "2": "3V3", "3": "SDA", "4": "SCL"})
    tag_connect(c, "J6", 46.0, 11.0, {"1": "3V3", "2": "SWDIO", "3": "NRST", "4": "SWCLK", "5": "GND"})

    # ----------------------------------------------------------------------------------- Microcontrôleur, capteur
    broches_u1 = ["3V3", "OSC_OUT", "OSC_IN", "NRST", "GND", "", "USB_DM", "USB_DP",
                  "CHG_STAT", "A0", "A1", "A2", "A3", "SCK", "MOSI", "MISO",
                  "RX", "TX", "SWDIO", "SWCLK", "SCL", "SDA", "LED", "3V3",
                  "BOOT0", "D10", "D9", "D6", "D5", "GND", "VBAT_SENSE", "", "GND"]
    qfn32(c, "U1", 30.0, 20.0, {str(i + 1): n for i, n in enumerate(broches_u1)}, "STM32G0B1KEU6", "STM32G0B1KEU6", "STMicroelectronics")
    lga8(c, "U3", 50.5, 29.5, {"1": "GND", "2": "3V3", "3": "SDA", "4": "SCL", "5": "GND", "6": "3V3", "7": "GND", "8": "3V3"}, "BME280", "BME280", "Bosch Sensortec")
    quartz3225(c, "Y1", 24.4, 21.6, 90, {"1": "OSC_IN", "2": "GND", "3": "OSC_OUT", "4": "GND"}, "8MHz", "ABM8-8.000MHZ-B2-T", "Abracon")

    # --------------------------------------------------------------------------------------------- Alimentation
    sot23(c, "U4", 12.8, 24.0, 270, ["USB_DM", "GND", "USB_DP", "USB_DP", "VBUS", "USB_DM"], "USBLC6-2SC6", "USBLC6-2SC6", "STMicroelectronics", broches=6)
    passif(c, "F1", 12.4, 28.6, 0, "VBUS", "VUSB", "500mA", "0ZCJ0050AF2E", "1206", fabricant="Bel Fuse")
    sod123(c, "D1", 17.6, 28.6, 180, "VSYS", "VUSB", "MBR0530", "MBR0530T1G", "onsemi")
    sot23(c, "Q1", 17.6, 31.8, 0, ["VUSB", "VSYS", "VBAT"], "DMG3415U", "DMG3415U-7", "Diodes Inc.")
    sot23(c, "U2", 22.6, 29.8, 0, ["VSYS", "GND", "VSYS", "", "3V3"], "AP2112K-3.3", "AP2112K-3.3TRG1", "Diodes Inc.", broches=5)
    sot23(c, "U5", 13.0, 12.5, 180, ["CHG_STAT", "GND", "VBAT", "VUSB", "Net-(U5-PROG)"], "MCP73831", "MCP73831T-2ACI/OT", "Microchip", broches=5)
    passif(c, "C1", 21.2, 26.4, 0, "VSYS", "GND", "10u", "GRM21BR61A106KE19L", "0805", fabricant="Murata")
    passif(c, "C2", 22.2, 32.7, 0, "3V3", "GND", "10u", "GRM21BR61A106KE19L", "0805", fabricant="Murata")["reperes"] = [(19.7, 33.35, 0)]
    passif(c, "C3", 10.6, 10.6, 90, "GND", "VUSB", "4u7", "GRM188R61A475KE15D", "0603", fabricant="Murata", hauteur=0.8)
    passif(c, "C4", 10.4, 15.6, 90, "VBAT", "GND", "4u7", "GRM188R61A475KE15D", "0603", fabricant="Murata", hauteur=0.8)
    passif(c, "R1", 10.0, 21.5, 90, "GND", "CC1", "5k1", "RC0402FR-075K1L")
    passif(c, "R2", 10.6, 26.0, 0, "CC2", "GND", "5k1", "RC0402FR-075K1L")
    passif(c, "R3", 13.95, 9.6, 90, "GND", "Net-(U5-PROG)", "2k", "RC0402FR-072KL")
    passif(c, "R4", 17.1, 15.25, 90, "Net-(D4-A)", "VUSB", "1k", "RC0402FR-071KL")
    passif(c, "D4", 16.3, 13.6, 0, "CHG_STAT", "Net-(D4-A)", "CHG", "150060AS75000", "0603", fabricant="Würth Elektronik", hauteur=0.8)
    passif(c, "R5", 25.9, 27.9, 0, "VBAT", "VBAT_SENSE", "100k", "RC0402FR-07100KL")
    passif(c, "R6", 25.9, 26.6, 0, "GND", "VBAT_SENSE", "100k", "RC0402FR-07100KL")
    passif(c, "C5", 25.9, 29.2, 0, "GND", "VBAT_SENSE", "100n", "GRM155R71C104KA88D", fabricant="Murata")
    passif(c, "R7", 14.9, 32.2, 90, "GND", "VUSB", "100k", "RC0402FR-07100KL")
    if not B:
        # Révision B : LED d'alimentation supprimée (consommation en veille)
        passif(c, "D2", 25.3, 31.0, 0, "GND", "Net-(D2-A)", "PWR", "150060VS75000", "0603", fabricant="Würth Elektronik", hauteur=0.8)
        passif(c, "R11", 25.0, 33.6, 0, "3V3", "Net-(D2-A)", "1k", "RC0402FR-071KL")["reperes"] = [(27.25, 33.5, 0)]

    # ---------------------------------------------------------------------- Autour du microcontrôleur et du capteur
    passif(c, "C6", 24.4, 19.0, 0, "GND", "OSC_IN", "12p", "GRM1555C1H120JA01D", fabricant="Murata")
    passif(c, "C7", 23.55, 24.9, 90, "OSC_OUT", "GND", "12p", "GRM1555C1H120JA01D", fabricant="Murata")
    passif(c, "C8", 27.0, 23.6, 90, "3V3", "GND", "100n", "GRM155R71C104KA88D", fabricant="Murata")
    passif(c, "C9", 33.4, 23.0, 90, "3V3", "GND", "100n", "GRM155R71C104KA88D", fabricant="Murata")
    passif(c, "C10", 12.0, 8.0, 0, "GND", "NRST", "100n", "GRM155R71C104KA88D", fabricant="Murata")
    passif(c, "C11", 52.9, 31.4, 0, "3V3", "GND", "100n", "GRM155R71C104KA88D", fabricant="Murata")
    passif(c, "C12", 52.9, 27.8, 0, "3V3", "GND", "100n", "GRM155R71C104KA88D", fabricant="Murata")
    if B:
        # Révision B : découplage ajouté au plus près du connecteur Qwiic
        passif(c, "C13", 50.6, 20.4, 0, "3V3", "GND", "1u", "GRM155R61A105KE15D", fabricant="Murata")
    passif(c, "R8", 33.15, 27.3, 90, "GND", "BOOT0", "10k", "RC0402FR-0710KL")
    rappel = "2k2" if B else "4k7"  # Révision B : rappels I²C plus forts (bus plus long avec le Qwiic)
    passif(c, "R9", 40.1, 32.5, 90, "3V3", "SCL", rappel, f"RC0402FR-07{rappel.upper()}L")
    passif(c, "R10", 37.6, 32.5, 90, "3V3", "SDA", rappel, f"RC0402FR-07{rappel.upper()}L")
    passif(c, "R12", 34.5, 22.9, 90, "LED", "Net-(D3-A)", "1k", "RC0402FR-071KL")
    passif(c, "D3", 34.5, 25.2, 270, "GND", "Net-(D3-A)", "USER", "150060BS75000", "0603", fabricant="Würth Elektronik", hauteur=0.8)
    bouton(c, "SW1", 12.0, 5.6, 0, "GND", "NRST", "RESET", "KMR221GLFS", "C&K")
    bouton(c, "SW2", 35.2, 29.7, 0, "BOOT0", "3V3", "BOOT", "KMR221GLFS", "C&K")
    for ref, x, y in (("FID1", 9.5, 34.0), ("FID2", 57.4, 7.0), ("FID3", 50.0, 35.6)):
        mire(c, ref, x, y)

    # ======================================================================================================= Routage
    # USB-C : CC, VBUS, et les deux paires D+/D− réunies (D− sur le dessus autour du via de D+, D+ par le dessous)
    piste(T, [(7.0, 21.55), (8.0, 21.55), (8.5, 21.2)], ALIM, "VBUS")
    c.via(8.5, 21.2, "VBUS")
    piste(T, [(7.0, 26.45), (7.8, 26.45), (8.4, 27.05), (9.9, 27.05), (10.95, 28.1), (10.95, 28.6)], ALIM, "VBUS")
    c.via(10.4, 27.55, "VBUS")
    piste(T, [(7.0, 22.75), (7.75, 22.75), (8.25, 22.25), (9.6, 22.25), (10.0, 22.0)], SIG, "CC1")
    piste(T, [(7.0, 25.75), (7.8, 25.75), (8.05, 26.0), (10.1, 26.0)], SIG, "CC2")
    piste(T, [(7.0, 23.25), (7.9, 23.25), (8.2, 22.95), (9.3, 22.95), (9.6, 23.25), (9.6, 24.25), (10.3, 24.95), (13.9, 24.95)], SIG, "USB_DM")
    piste(T, [(7.0, 24.25), (7.9, 24.25), (8.2, 24.55), (9.3, 24.55), (9.6, 24.25)], SIG, "USB_DM")
    piste(T, [(7.0, 23.75), (8.7, 23.75)], SIG, "USB_DP")
    piste(T, [(7.0, 24.75), (7.75, 24.75), (8.15, 25.15), (8.6, 25.35)], SIG, "USB_DP")
    for x, y in ((8.7, 23.75), (8.6, 25.35), (10.6, 23.05)):
        c.via(x, y, "USB_DP")
    piste(D, [(8.6, 25.35), (8.7, 23.75), (10.6, 23.05)], SIG, "USB_DP")
    piste(T, [(10.6, 23.05), (13.9, 23.05)], SIG, "USB_DP")
    piste(T, [(11.7, 24.0), (10.5, 24.0)], ALIM, "GND")
    c.via(10.5, 24.0, "GND")
    piste(T, [(13.9, 24.0), (14.9, 24.0)], ALIM, "VBUS")
    c.via(14.9, 24.0, "VBUS")
    piste(D, [(8.5, 21.2), (8.5, 19.6), (14.0, 19.6), (14.9, 20.5), (14.9, 25.6), (12.95, 27.55), (10.4, 27.55)], ALIM, "VBUS")
    c.via(10.0, 20.35, "GND")
    piste(T, [(10.0, 21.0), (10.0, 20.35)], SIG, "GND")
    c.via(11.9, 26.0, "GND")
    piste(T, [(11.1, 26.0), (11.9, 26.0)], SIG, "GND")
    # Paire USB vers le microcontrôleur
    piste(T, [(13.9, 23.05), (16.2, 23.05), (16.2, 20.0), (18.95, 17.25), (26.0, 17.25), (27.0, 18.25), (27.55, 18.25)], SIG, "USB_DP")
    piste(T, [(13.9, 24.95), (16.7, 24.95), (16.7, 20.2), (19.15, 17.75), (25.8, 17.75), (26.8, 18.75), (27.55, 18.75)], SIG, "USB_DM")

    # Alimentation : fusible, diode, transistor de commutation batterie, régulateur
    piste(T, [(13.85, 28.6), (15.95, 28.6)], FORT, "VUSB")
    piste(T, [(15.0, 28.6), (15.0, 27.7)], FORT, "VUSB")
    c.via(15.0, 27.7, "VUSB")
    piste(T, [(15.95, 28.6), (15.95, 29.5), (16.65, 30.2), (16.65, 30.7)], SIG, "VUSB")
    piste(T, [(16.65, 30.7), (15.9, 31.45), (15.9, 33.9), (21.11, 33.9), (21.11, 35.2)], ALIM, "VUSB")
    piste(T, [(14.9, 32.7), (15.9, 32.7)], SIG, "VUSB")
    c.via(14.9, 30.9, "GND")
    piste(T, [(14.9, 31.7), (14.9, 30.9)], SIG, "GND")
    piste(T, [(18.5, 27.75), (23.55, 27.75), (23.55, 28.7)], ALIM, "VSYS")
    piste(T, [(19.25, 28.6), (19.25, 27.75)], ALIM, "VSYS")
    piste(T, [(21.65, 27.75), (21.65, 28.7)], ALIM, "VSYS")
    piste(T, [(20.25, 26.4), (20.25, 27.75)], ALIM, "VSYS")
    piste(T, [(18.55, 30.7), (18.55, 29.9), (19.25, 29.2)], ALIM, "VSYS")
    c.via(18.5, 27.75, "VSYS")
    c.via(22.6, 29.8, "GND")
    piste(T, [(22.6, 28.7), (22.6, 29.8)], ALIM, "GND")
    c.via(22.15, 25.4, "GND")
    piste(T, [(22.15, 26.4), (22.15, 25.4)], ALIM, "GND")
    piste(T, [(21.65, 30.9), (21.65, 31.5), (21.25, 31.9), (21.25, 32.7)], ALIM, "3V3")
    c.via(20.4, 31.9, "3V3")
    piste(T, [(20.4, 31.9), (21.25, 31.9)], ALIM, "3V3")
    c.via(23.15, 33.75, "GND")
    piste(T, [(23.15, 32.7), (23.15, 33.75)], ALIM, "GND")
    # Batterie : Q1 (drain) vers J3 et, par le dessous, vers le chargeur, J4 et le pont diviseur
    c.via(18.6, 32.9, "VBAT")
    piste(T, [(17.6, 32.9), (18.6, 32.9)], ALIM, "VBAT")
    piste(D, [(18.6, 32.9), (18.57, 35.2)], ALIM, "VBAT")
    piste(D, [(18.6, 32.9), (5.0, 32.9), (4.0, 31.9), (4.0, 16.0), (5.0, 15.0), (8.2, 15.0), (9.6, 13.6)], ALIM, "VBAT")
    piste(D, [(18.6, 32.9), (20.6, 32.9), (23.9, 29.6), (23.9, 28.6), (24.6, 27.9)], SIG, "VBAT")
    c.via(24.6, 27.9, "VBAT")
    piste(T, [(24.6, 27.9), (25.4, 27.9)], SIG, "VBAT")
    piste(T, [(26.4, 29.2), (26.4, 26.6), (27.2, 26.6), (28.75, 25.05), (28.75, 22.45)], SIG, "VBAT_SENSE")
    for x, y in ((24.6, 26.6), (24.6, 29.2)):
        c.via(x, y, "GND")
        piste(T, [(x, y), (25.4, y)], SIG, "GND")
    if not B:
        c.via(24.5, 32.75, "3V3")
        piste(T, [(24.5, 32.75), (24.5, 33.6)], SIG, "3V3")
        piste(T, [(25.5, 33.6), (26.1, 33.0), (26.1, 31.0)], SIG, "Net-(D2-A)")
        c.via(24.5, 30.1, "GND")
        piste(T, [(24.5, 31.0), (24.5, 30.1)], SIG, "GND")

    # Chargeur
    piste(T, [(12.05, 13.6), (12.05, 14.4), (10.4, 14.4), (8.8, 12.8), (8.0, 12.0), (6.8, 12.0)], FORT, "VBAT")
    c.via(9.6, 13.6, "VBAT")
    piste(T, [(10.4, 14.4), (10.4, 14.8)], FORT, "VBAT")
    c.via(10.4, 17.3, "GND")
    piste(T, [(10.4, 16.4), (10.4, 17.3)], ALIM, "GND")
    piste(T, [(12.05, 11.4), (10.6, 11.4)], FORT, "VUSB")
    c.via(12.05, 10.3, "VUSB")
    piste(T, [(12.05, 11.4), (12.05, 10.3)], FORT, "VUSB")
    c.via(10.6, 8.95, "GND")
    piste(T, [(10.6, 9.8), (10.6, 8.95)], ALIM, "GND")
    piste(T, [(13.95, 11.4), (13.95, 10.1)], SIG, "Net-(U5-PROG)")
    c.via(13.95, 8.3, "GND")
    piste(T, [(13.95, 9.1), (13.95, 8.3)], SIG, "GND")
    c.via(13.0, 14.6, "GND")
    piste(T, [(13.0, 13.6), (13.0, 14.6)], ALIM, "GND")
    piste(T, [(13.95, 13.6), (15.5, 13.6)], SIG, "CHG_STAT")
    piste(T, [(17.1, 13.6), (17.1, 14.75)], SIG, "Net-(D4-A)")
    c.via(18.0, 15.75, "VUSB")
    piste(T, [(17.1, 15.75), (18.0, 15.75)], SIG, "VUSB")
    piste(D, [(15.0, 27.7), (16.0, 26.7), (16.0, 17.75), (18.0, 15.75), (17.4, 15.15), (12.55, 10.3), (12.05, 10.3)], FORT, "VUSB")
    piste(T, [(28.25, 17.55), (28.25, 17.1), (27.65, 16.5), (20.0, 16.5), (19.6, 16.1), (19.6, 12.6), (15.9, 12.6), (15.5, 13.0), (15.5, 13.6)], SIG, "CHG_STAT")
    # J4 : masse
    c.via(8.9, 10.0, "GND")
    piste(T, [(6.8, 10.0), (8.9, 10.0)], FORT, "GND")

    # Quartz, découplage du microcontrôleur, NRST
    piste(T, [(25.25, 20.75), (27.55, 20.75)], SIG, "OSC_IN")
    piste(T, [(24.9, 19.0), (25.25, 19.35), (25.25, 20.5)], SIG, "OSC_IN")
    piste(T, [(23.55, 22.7), (23.55, 24.4)], SIG, "OSC_OUT")
    piste(T, [(23.55, 23.8), (26.3, 23.8), (26.3, 21.65), (26.7, 21.25), (27.55, 21.25)], SIG, "OSC_OUT")
    piste(T, [(25.25, 22.7), (23.55, 20.5)], SIG, "GND")
    piste(T, [(23.55, 20.5), (23.55, 19.3), (23.9, 19.0)], SIG, "GND")
    # DÉFAUT 3 : via au diamètre de 0,40 mm pour un trou de 0,30 mm (anneau de 0,05 mm)
    c.via(22.8, 19.0, "GND", d=0.4)
    piste(T, [(23.9, 19.0), (22.8, 19.0)], SIG, "GND")
    c.via(23.55, 26.2, "GND")
    piste(T, [(23.55, 25.4), (23.55, 26.2)], SIG, "GND")
    piste(T, [(27.55, 21.75), (26.95, 21.75), (26.95, 22.45)], SIG, "3V3")
    c.via(26.95, 22.45, "3V3")
    piste(T, [(26.95, 22.45), (27.0, 23.1)], SIG, "3V3")
    c.via(27.0, 24.9, "GND")
    piste(T, [(27.0, 24.1), (27.0, 24.9)], SIG, "GND")
    piste(T, [(27.55, 20.25), (26.6, 20.25), (26.3, 19.95)], SIG, "NRST")
    c.via(26.3, 19.95, "NRST")
    piste(T, [(27.55, 19.75), (28.6, 19.75)], SIG, "GND")
    piste(T, [(29.25, 22.45), (29.25, 21.4)], SIG, "GND")
    for x, y in ((29.4, 19.4), (30.6, 19.4), (29.4, 20.6), (30.6, 20.6)):
        c.via(x, y, "GND")  # vias thermiques sous la pastille centrale
    piste(D, [(26.3, 19.95), (22.6, 16.25), (22.6, 5.4), (16.9, 5.4), (16.03, 4.53), (16.03, 2.8)], SIG, "NRST")
    piste(D, [(22.6, 5.4), (44.4, 5.4), (46.0, 7.0), (46.0, 9.25)], SIG, "NRST")
    c.via(46.0, 9.25, "NRST")
    piste(T, [(46.0, 9.25), (46.0, 10.365)], SIG, "NRST")
    piste(T, [(14.05, 4.8), (15.2, 4.8), (16.03, 3.97), (16.03, 2.8)], SIG, "NRST")
    piste(T, [(14.05, 4.8), (14.05, 6.4)], SIG, "NRST")
    piste(T, [(12.5, 8.0), (13.3, 8.0), (14.05, 7.25), (14.05, 6.4)], SIG, "NRST")
    c.via(8.9, 5.6, "GND")
    piste(T, [(9.95, 6.4), (9.95, 4.8)], SIG, "GND")
    piste(T, [(9.95, 5.6), (8.9, 5.6)], SIG, "GND")
    piste(T, [(11.5, 8.0), (10.6, 8.95)], SIG, "GND")

    # Éventail vers J2 (sorties analogiques, SPI, UART)
    piste(T, [(28.75, 17.55), (28.75, 16.6), (23.65, 11.5), (23.65, 2.8)], SIG, "A0")
    piste(T, [(29.25, 17.55), (29.25, 16.2), (26.19, 13.14), (26.19, 2.8)], SIG, "A1")
    piste(T, [(29.75, 17.55), (29.75, 15.8), (28.73, 14.78), (28.73, 2.8)], SIG, "A2")
    piste(T, [(30.25, 17.55), (30.25, 15.4), (31.27, 14.38), (31.27, 2.8)], SIG, "A3")
    piste(T, [(30.75, 17.55), (30.75, 15.8), (33.81, 12.74), (33.81, 2.8)], SIG, "SCK")
    piste(T, [(31.25, 17.55), (31.25, 16.2), (36.35, 11.1), (36.35, 2.8)], SIG, "MOSI")
    piste(T, [(31.75, 17.55), (31.75, 16.6), (38.89, 9.46), (38.89, 2.8)], SIG, "MISO")
    piste(T, [(32.45, 18.25), (33.4, 18.25), (41.43, 10.22), (41.43, 2.8)], SIG, "RX")
    piste(T, [(32.45, 18.75), (34.0, 18.75), (43.97, 8.78), (43.97, 2.8)], SIG, "TX")
    # Programmation (Tag-Connect)
    piste(T, [(32.45, 19.25), (44.73, 19.25), (44.73, 11.635)], SIG, "SWDIO")
    piste(T, [(32.45, 19.75), (46.0, 19.75), (46.0, 11.635)], SIG, "SWCLK")
    for x, net in ((44.73, "3V3"), (47.27, "GND")):
        c.via(x, 9.25, net)
        piste(T, [(x, 9.25), (x, 10.365)], SIG, net)

    # Haut du microcontrôleur : vers J3, BOOT0, LED, découplage
    piste(T, [(29.75, 22.45), (29.75, 30.6), (26.19, 34.16), (26.19, 35.2)], SIG, "D5")
    piste(T, [(30.25, 22.45), (30.25, 32.4), (28.73, 33.92), (28.73, 35.2)], SIG, "D6")
    piste(T, [(30.75, 22.45), (30.75, 32.9), (31.27, 33.42), (31.27, 35.2)], SIG, "D9")
    piste(T, [(31.25, 22.45), (31.25, 30.9), (33.81, 33.46), (33.81, 35.2)], SIG, "D10")
    piste(T, [(31.75, 22.45), (31.75, 29.9), (32.35, 30.5), (33.15, 30.5)], SIG, "BOOT0")
    piste(T, [(33.15, 30.5), (33.15, 27.8)], SIG, "BOOT0")
    c.via(33.15, 26.0, "GND")
    piste(T, [(33.15, 26.8), (33.15, 26.0)], SIG, "GND")
    piste(T, [(32.45, 21.75), (33.1, 21.75), (33.4, 22.05), (33.4, 22.5)], SIG, "3V3")
    c.via(32.55, 23.2, "3V3")
    piste(T, [(32.55, 23.2), (33.1, 22.65)], SIG, "3V3")
    c.via(33.4, 24.3, "GND")
    piste(T, [(33.4, 23.5), (33.4, 24.3)], SIG, "GND")
    piste(T, [(32.45, 21.25), (33.9, 21.25), (34.5, 21.85), (34.5, 22.4)], SIG, "LED")
    piste(T, [(34.5, 23.4), (34.5, 24.4)], SIG, "Net-(D3-A)")
    c.via(35.4, 26.0, "GND")
    piste(T, [(34.5, 26.0), (35.4, 26.0)], SIG, "GND")
    piste(T, [(37.25, 28.9), (37.25, 30.5), (37.6, 31.2)], SIG, "3V3")
    c.via(37.6, 31.2, "3V3")
    piste(T, [(37.6, 31.2), (37.6, 32.0)], SIG, "3V3")
    # I²C : vers J3 sur le dessus, puis par le dessous vers le capteur et le Qwiic
    piste(T, [(32.45, 20.75), (35.85, 20.75), (36.35, 21.25), (36.35, 35.2)], SIG, "SDA")
    piste(T, [(37.6, 33.0), (36.35, 33.0)], SIG, "SDA")
    if B:
        # Révision B : SCL déroutée vers la droite (dégage la zone sous le bouton BOOT)
        piste(T, [(32.45, 20.25), (38.39, 20.25), (38.89, 20.75), (38.89, 23.0), (39.8, 23.9), (39.8, 29.6), (38.89, 30.5), (38.89, 35.2)], SIG, "SCL")
    else:
        piste(T, [(32.45, 20.25), (38.39, 20.25), (38.89, 20.75), (38.89, 35.2)], SIG, "SCL")
    piste(T, [(40.1, 33.0), (38.89, 33.0)], SIG, "SCL")
    piste(T, [(40.1, 32.0), (41.43, 32.0), (41.43, 35.2)], SIG, "3V3")
    piste(D, [(36.35, 35.2), (36.35, 33.0), (43.0, 33.0), (44.4, 34.4), (47.3, 34.4), (47.3, 29.6)], SIG, "SDA")
    piste(D, [(38.89, 35.2), (38.89, 33.5), (42.3, 33.5), (43.7, 34.9), (48.1, 34.9), (48.1, 28.0)], SIG, "SCL")
    c.via(47.3, 29.6, "SDA")
    c.via(48.1, 28.0, "SCL")
    piste(T, [(47.3, 29.6), (47.75, 29.175), (49.475, 29.175)], SIG, "SDA")
    piste(T, [(48.1, 28.0), (48.625, 28.525), (49.475, 28.525)], SIG, "SCL")
    piste(D, [(47.3, 29.6), (47.3, 25.8), (55.1, 25.8), (55.1, 18.6), (54.5, 18.0), (52.4, 18.0)], SIG, "SDA")
    piste(D, [(48.1, 28.0), (48.1, 26.4), (55.7, 26.4), (55.7, 17.6), (55.1, 17.0), (52.4, 17.0)], SIG, "SCL")
    for y, net in ((20.0, "GND"), (19.0, "3V3"), (18.0, "SDA"), (17.0, "SCL")):
        c.via(52.4, y, net)
        piste(T, [(52.4, y), (54.0, y)], SIG, net)
    if B:
        piste(T, [(50.1, 20.4), (50.1, 19.0), (52.4, 19.0)], SIG, "3V3")
        piste(T, [(51.1, 20.4), (52.4, 20.0)], SIG, "GND")
    # Capteur : alimentation et masse (sous le boîtier)
    piste(T, [(51.525, 29.825), (50.9, 29.825), (50.9, 28.525), (51.525, 28.525)], SIG, "GND")
    piste(T, [(49.475, 29.825), (50.1, 29.825), (50.1, 30.475), (51.525, 30.475)], SIG, "3V3")
    piste(T, [(51.525, 29.175), (52.3, 29.175), (52.3, 30.475), (51.525, 30.475)], SIG, "3V3")
    # DÉFAUT 1 : liaison de 0,10 mm de large entre le capteur et son condensateur de découplage
    piste(T, [(52.3, 30.475), (52.4, 31.4)], 0.10, "3V3")
    piste(T, [(52.3, 29.175), (52.4, 27.8)], SIG, "3V3")
    c.via(53.3, 29.8, "3V3")
    piste(T, [(52.3, 29.8), (53.3, 29.8)], SIG, "3V3")
    for x, y in ((54.3, 31.4), (54.3, 27.8)):
        c.via(x, y, "GND")
        piste(T, [(53.4, y), (x, y)], SIG, "GND")
    c.via(51.525, 27.6, "GND")
    piste(T, [(51.525, 28.525), (51.525, 27.6)], SIG, "GND")
    c.via(49.475, 31.4, "GND")
    piste(T, [(49.475, 30.475), (49.475, 31.4)], SIG, "GND")

    # Vias de couture de la masse
    for x, y in ((40.5, 26.5), (42.5, 22.5), (20.0, 20.5), (6.5, 31.0), (6.5, 17.5), (37.6, 8.0), (50.0, 15.0), (57.5, 13.0), (57.5, 23.0), (42.6, 28.4) if B else (42.0, 30.0)):
        c.via(x, y, "GND")
    # DÉFAUT 2 : via de masse à 0,09 mm de la piste NRST du dessous
    c.via(35.0, 5.89, "GND")
    # DÉFAUT 4 : via de masse à 0,25 mm du bord de la carte
    c.via(30.0, 0.55, "GND")
    # DÉFAUT 5 : via percé à 0,25 mm
    c.via(40.5, 14.5, "GND", trou=0.25)
    return finir(c)


def finir(c):
    """Zones de cuivre, sérigraphie, textes."""
    tout = Polygon(c.contour())
    c.zones.append({"couche": "gnd", "net": "GND", "contour": tout})
    vsys = box(0, 26.6, 19.6, HAUTEUR)
    c.zones.append({"couche": "pwr", "net": "VSYS", "contour": vsys})
    c.zones.append({"couche": "pwr", "net": "3V3", "contour": tout.difference(vsys)})
    c.zones.append({"couche": "top", "net": "GND", "contour": tout})
    c.zones.append({"couche": "bottom", "net": "GND", "contour": tout})
    couler(c)
    rev = c.revision
    textes = [
        ("TYPON", 54.3, 13.2, 2.0, 0, False), ("IOT SENSOR", 54.3, 10.9, 0.9, 0, False), (f"REV {rev}", 54.3, 9.3, 0.8, 0, False),
        ("USB", 3.6, 30.6, 0.8, 0, False), ("+", 9.1, 12.0, 0.8, 0, False), ("-", 9.1, 10.0, 0.8, 0, False),
        ("QWIIC", 55.7, 22.75, 0.8, 0, False), ("BOOT", 34.5, 32.0, 0.8, 0, False), ("CHG", 16.4, 12.2, 0.8, 0, False),
        ("SWD", 46.0, 13.2, 0.8, 0, False),
        # DÉFAUT 6 : étiquette « RST » posée en partie sur la pastille 3 du bouton SW1
        ("RST", 14.82, 7.15, 0.8, 0, True),
    ]
    if rev == "A":
        textes.append(("PWR", 28.2, 31.0, 0.8, 0, False))
    serigraphier(c, textes)
    dessous = [("TYPON", 30.0, 22.5, 2.4, 0), ("IOT SENSOR NODE", 30.0, 19.2, 1.0, 0), (f"REV {rev}  2026-11", 30.0, 17.2, 0.8, 0)]
    for comp, haut in (("J2", True), ("J3", False)):
        for p in next(k for k in c.composants if k["ref"] == comp)["broches"]:
            nom = p["net"].replace("_", "")
            lt = largeur_texte(nom, 0.6)
            y = p["y"] + 0.85 + 0.45 + lt / 2 if haut else p["y"] - 0.85 - 0.45 - lt / 2
            dessous.append((nom, p["x"], y, 0.6, 90))
    serigraphier_dessous(c, dessous)
    return c


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
