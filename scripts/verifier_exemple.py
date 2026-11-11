"""Vérificateur indépendant des cartes d'exemple de Typon : relit les fichiers Gerber X2 et Excellon produits par
generer_exemples.py (sans rien lui emprunter) et contrôle les règles de fabrication, la connectivité des nets et
l'absence de courts-circuits. Il doit trouver exactement les défauts volontaires décrits dans docs/defauts-exemple.md,
et rien d'autre.

    pip install shapely pillow       # outils de vérification seulement (pas des dépendances du site)
    python scripts/verifier_exemple.py [archive.zip…] [--images dossier]

Deux jeux de seuils : ceux du contrôleur DFM de Typon (un défaut volontaire doit être en dessous) et des seuils plus
stricts de « propreté » (tout le reste de la carte doit les respecter largement).
"""
import math
import os
import re
import sys
import zipfile

from shapely.geometry import LineString, Point, Polygon, box
from shapely.ops import unary_union
from shapely.strtree import STRtree

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Seuils du contrôleur DFM de Typon
DFM = {"piste": 0.127, "isolement": 0.127, "anneau": 0.13, "percage": 0.3, "ecart_trous": 0.25, "bord": 0.3, "npth": 0.2, "pont_vernis": 0.1, "trait_seri": 0.12}
# Seuils de propreté de la carte (en dehors des défauts volontaires)
PROPRE = {"piste": 0.15, "isolement": 0.15, "anneau": 0.15, "percage": 0.3, "ecart_trous": 0.3, "bord": 0.4, "npth": 0.3, "pont_vernis": 0.1, "trait_seri": 0.15, "seri_vernis": 0.1, "seri_bord": 0.3}

# Défauts volontaires attendus : (règle, couches, x, y) ; constats à 0,8 mm près (docs/defauts-exemple.md).
ATTENDUS = [
    ("piste", ("F_Cu",), 52.35, 30.94),
    ("isolement", ("B_Cu",), 35.00, 5.54),
    ("anneau", ("F_Cu", "B_Cu"), 22.80, 19.00),
    ("bord", ("F_Cu", "B_Cu"), 30.00, 0.30),
    ("percage", ("PTH",), 40.50, 14.50),
    ("seri_vernis", ("F_Silkscreen",), 14.20, 6.85),
]

QS = 8


class Couche:
    def __init__(self):
        self.objets = []  # dict : geo, sombre, net, broche, comp, genre ("flash", "trait", "region"), largeur, fonction, x, y
        self.attributs = {}


def lire_gerber(texte):
    c = Couche()
    ouvertures = {}
    fonction_ouv = {}
    attrs_objet = {}
    fonction_courante = None
    courante = None
    sombre = True
    region = False
    contour = []
    contours = []
    x = y = 0.0
    k = 1e-6
    for brut in re.findall(r"%[^%]*%|[^*%\n]+\*", texte):
        cmd = brut.strip("%").strip()
        for m in [m for m in cmd.split("*") if m.strip()] if brut.startswith("%") else [cmd.rstrip("*")]:
            m = m.strip()
            if m.startswith("FS"):
                f = re.search(r"X(\d)(\d)", m)
                k = 10 ** -int(f.group(2))
            elif m.startswith("TF."):
                n, _, v = m[3:].partition(",")
                c.attributs[n] = v
            elif m.startswith("TA.AperFunction"):
                fonction_courante = m.split(",", 1)[1]
            elif m.startswith("TA."):
                pass
            elif m.startswith("TO."):
                n, _, v = m[3:].partition(",")
                attrs_objet[n] = v
            elif m.startswith("TD"):
                n = m[2:].lstrip(".")
                if not n:
                    attrs_objet = {}
                    fonction_courante = None
                elif n == "AperFunction":
                    fonction_courante = None
                else:
                    attrs_objet.pop(n, None)
            elif m.startswith("ADD"):
                r = re.match(r"ADD(\d+)([A-Z]+),(.*)", m)
                n, forme, params = int(r.group(1)), r.group(2), [float(v) for v in r.group(3).split("X")]
                ouvertures[n] = (forme, params)
                fonction_ouv[n] = fonction_courante
            elif m in ("LPD",):
                sombre = True
            elif m in ("LPC",):
                sombre = False
            elif m == "G36":
                region, contours, contour = True, [], []
            elif m == "G37":
                if len(contour) >= 3:
                    contours.append(contour)
                for ct in contours:
                    c.objets.append({"geo": Polygon(ct).buffer(0), "sombre": sombre, "genre": "region", "net": attrs_objet.get("N"), "fonction": fonction_courante, "attrs": dict(attrs_objet)})
                region = False
            elif re.match(r"^D(\d+)$", m) and int(m[1:]) >= 10:
                courante = int(m[1:])
            elif m.startswith("G04") or m.startswith("MO") or m in ("G01", "G75", "M02") or m.startswith("TF"):
                pass
            else:
                r = re.match(r"^(?:G0?1)?(?:X(-?\d+))?(?:Y(-?\d+))?D0?([123])$", m)
                if not r:
                    continue
                nx = int(r.group(1)) * k if r.group(1) else x
                ny = int(r.group(2)) * k if r.group(2) else y
                d = int(r.group(3))
                if region:
                    if d == 2:
                        if len(contour) >= 3:
                            contours.append(contour)
                        contour = [(nx, ny)]
                    elif d == 1:
                        contour.append((nx, ny))
                elif d == 1:
                    forme, params = ouvertures[courante]
                    largeur = params[0]
                    geo = LineString([(x, y), (nx, ny)]).buffer(largeur / 2, quad_segs=QS) if (x, y) != (nx, ny) else Point(x, y).buffer(largeur / 2, quad_segs=QS)
                    c.objets.append({"geo": geo, "sombre": sombre, "genre": "trait", "net": attrs_objet.get("N"), "largeur": largeur, "fonction": fonction_ouv.get(courante), "a": (x, y), "b": (nx, ny), "attrs": dict(attrs_objet)})
                elif d == 3:
                    forme, params = ouvertures[courante]
                    if forme == "C":
                        geo = Point(nx, ny).buffer(params[0] / 2, quad_segs=QS)
                    elif forme == "R":
                        geo = box(nx - params[0] / 2, ny - params[1] / 2, nx + params[0] / 2, ny + params[1] / 2)
                    else:
                        w, h = params
                        r_ = min(w, h) / 2
                        seg = [(nx - w / 2 + r_, ny), (nx + w / 2 - r_, ny)] if w >= h else [(nx, ny - h / 2 + r_), (nx, ny + h / 2 - r_)]
                        geo = LineString(seg).buffer(r_, quad_segs=QS)
                    c.objets.append({"geo": geo, "sombre": sombre, "genre": "flash", "net": attrs_objet.get("N"), "broche": attrs_objet.get("P"), "comp": attrs_objet.get("C"), "fonction": fonction_ouv.get(courante), "x": nx, "y": ny, "attrs": dict(attrs_objet)})
                x, y = nx, ny
    return c


def lire_excellon(texte):
    outils, trous, t = {}, [], None
    metrique = True
    for l in texte.splitlines():
        l = l.strip()
        if l == "INCH" or l.startswith("INCH,"):
            metrique = False
        m = re.match(r"^T(\d+)C([\d.]+)$", l)
        if m:
            outils[int(m.group(1))] = float(m.group(2)) * (1 if metrique else 25.4)
            continue
        m = re.match(r"^T(\d+)$", l)
        if m:
            t = int(m.group(1))
            continue
        m = re.match(r"^X(-?[\d.]+)Y(-?[\d.]+)$", l)
        if m and t:
            trous.append((float(m.group(1)), float(m.group(2)), outils[t]))
    return trous


def geometrie(couche):
    """Cuivre final d'une couche (polarités appliquées), par objet sombre : [(geo, objet)]."""
    restants = []
    for o in couche.objets:
        if o["sombre"]:
            restants.append([o["geo"], o])
        else:
            g = o["geo"]
            for r in restants:
                if r[0].intersects(g):
                    r[0] = r[0].difference(g)
    return [(g, o) for g, o in restants if not g.is_empty]


def ilots(elements):
    tout = unary_union([g for g, _ in elements])
    polys = [tout] if tout.geom_type == "Polygon" else list(tout.geoms)
    arbre = STRtree(polys)
    nets = [set() for _ in polys]
    objets = [[] for _ in polys]
    for i, (g, o) in enumerate(elements):
        for j in arbre.query(g):
            if polys[j].intersection(g).area > 1e-7:
                net = o.get("net")
                if net in (None, "", "N/C"):
                    net = f"N/C:{o.get('broche') or i}"
                nets[j].add(net)
                objets[j].append(o)
    return polys, nets, objets


class Rapport:
    def __init__(self, nom):
        self.nom = nom
        self.dfm = []  # (règle, couche, x, y, valeur, seuil, texte)
        self.propre = []
        self.erreurs = []

    def constat(self, regle, couche, x, y, valeur, texte):
        if regle in DFM and valeur < DFM[regle] - 0.002:
            self.dfm.append((regle, couche, x, y, valeur, DFM[regle], texte))
        elif regle == "seri_vernis":
            self.dfm.append((regle, couche, x, y, valeur, 0.0, texte))
        elif valeur < PROPRE.get(regle, 0) - 0.002:
            self.propre.append((regle, couche, x, y, valeur, PROPRE[regle], texte))


def point_proche(g1, g2):
    from shapely.ops import nearest_points
    a, b = nearest_points(g1, g2)
    return (a.x + b.x) / 2, (a.y + b.y) / 2


def verifier(chemin, images=None):
    z = zipfile.ZipFile(chemin)
    fichiers = {n: z.read(n).decode("utf-8") for n in z.namelist()}
    rap = Rapport(os.path.basename(chemin))
    trouver = lambda suffixe: next((t for n, t in fichiers.items() if n.endswith(suffixe)), None)
    contour = lire_gerber(trouver("Edge_Cuts.gbr"))
    # Contour : traits enchaînés → polylignes fermées
    lignes = [LineString([o["a"], o["b"]]) for o in contour.objets if o["genre"] == "trait"]
    from shapely.ops import polygonize
    polys = sorted(polygonize(lignes), key=lambda p: -p.area)
    plaque = polys[0]
    for p in polys[1:]:
        if plaque.contains(p):
            plaque = plaque.difference(p)
    bords = unary_union(lignes)
    pth = lire_excellon(trouver("-PTH.drl"))
    npth = lire_excellon(trouver("-NPTH.drl"))
    couches = {}
    for nom in ("F_Cu", "In1_Cu", "In2_Cu", "B_Cu"):
        couches[nom] = lire_gerber(trouver(f"-{nom}.gbr"))
    resultats = {}
    for nom, cl in couches.items():
        elements = geometrie(cl)
        polys_, nets, objets = ilots(elements)
        resultats[nom] = (polys_, nets, objets, elements)
        # Courts-circuits
        for p, n, objs in zip(polys_, nets, objets):
            if len(n) > 1:
                # Localiser les contacts entre objets de nets différents
                lieux = []
                geos = [(g, o) for g, o in elements if o in objs]
                for a in range(len(geos)):
                    for b in range(a + 1, len(geos)):
                        (ga, oa), (gb, ob) = geos[a], geos[b]
                        if oa.get("net") != ob.get("net") and ga.intersects(gb):
                            q = ga.intersection(gb).representative_point()
                            lieux.append(f"{oa.get('net')}/{ob.get('net')} ({q.x:.2f} ; {q.y:.2f})")
                rap.erreurs.append(f"court-circuit sur {nom} : {sorted(n)} : " + ", ".join(lieux[:6]))
        # Isolement entre îlots de nets différents
        arbre = STRtree(polys_)
        for i, p in enumerate(polys_):
            for j in arbre.query(p.buffer(PROPRE["isolement"])):
                if j <= i or nets[i] == nets[j] or nets[i] & nets[j]:
                    continue
                d = p.distance(polys_[j])
                if d < PROPRE["isolement"]:
                    x, y = point_proche(p, polys_[j])
                    rap.constat("isolement", nom, x, y, d, f"{sorted(nets[i])} ↔ {sorted(nets[j])}")
        # Largeur des pistes
        for o in cl.objets:
            if o["genre"] == "trait" and o["sombre"] and o["largeur"] < PROPRE["piste"]:
                x, y = (o["a"][0] + o["b"][0]) / 2, (o["a"][1] + o["b"][1]) / 2
                rap.constat("piste", nom, x, y, o["largeur"], f"net {o['net']}")
        # Cuivre au bord
        for p in polys_:
            d = p.distance(bords)
            if d < PROPRE["bord"] or not plaque.buffer(1e-6).contains(p):
                x, y = point_proche(p, bords)
                rap.constat("bord", nom, x, y, d if plaque.buffer(1e-6).contains(p) else 0.0, "cuivre près du contour")
        # Cuivre près des trous non métallisés
        for x, y, d in npth:
            trou = Point(x, y).buffer(d / 2, quad_segs=QS)
            for p in polys_:
                e = p.distance(trou)
                if e < PROPRE["npth"]:
                    rap.constat("npth", nom, x, y, e, f"trou Ø{d}")
    # Perçages : diamètre, écart, anneau, pastille
    tous = [(x, y, d, True) for x, y, d in pth] + [(x, y, d, False) for x, y, d in npth]
    for x, y, d, plaque_ in tous:
        if d < PROPRE["percage"]:
            rap.constat("percage", "PTH" if plaque_ else "NPTH", x, y, d, f"Ø{d}")
    for i in range(len(tous)):
        for j in range(i + 1, len(tous)):
            a, b = tous[i], tous[j]
            e = math.hypot(a[0] - b[0], a[1] - b[1]) - a[2] / 2 - b[2] / 2
            if e < PROPRE["ecart_trous"]:
                rap.constat("ecart_trous", "perçages", (a[0] + b[0]) / 2, (a[1] + b[1]) / 2, e, f"Ø{a[2]} et Ø{b[2]}")
    for nom in ("F_Cu", "B_Cu"):
        flashes = [o for o in couches[nom].objets if o["genre"] == "flash" and o["sombre"]]
        for x, y, d in pth:
            centre = Point(x, y)
            pads = [o for o in flashes if o["geo"].contains(centre)]
            if not pads:
                rap.erreurs.append(f"trou métallisé sans pastille sur {nom} en ({x:.2f} ; {y:.2f})")
                continue
            anneau = max(o["geo"].exterior.distance(centre) for o in pads) - d / 2
            if anneau < PROPRE["anneau"]:
                rap.constat("anneau", nom, x, y, anneau, f"trou Ø{d}")
    # Connectivité : îlots reliés par les trous métallisés
    parent = {}

    def trouve(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    def unir(a, b):
        parent[trouve(a)] = trouve(b)

    for nom, (polys_, nets, objets, _) in resultats.items():
        for i in range(len(polys_)):
            parent[(nom, i)] = (nom, i)
    arbres = {nom: STRtree(r[0]) for nom, r in resultats.items()}
    for x, y, d in pth:
        centre = Point(x, y)
        touches = []
        for nom, (polys_, *_r) in resultats.items():
            for j in arbres[nom].query(centre):
                if polys_[j].buffer(1e-6).contains(centre):
                    touches.append((nom, j))
        for a, b in zip(touches, touches[1:]):
            unir(a, b)
    broches = {}
    for nom, (polys_, nets, objets, _) in resultats.items():
        for i, objs in enumerate(objets):
            for o in objs:
                if o.get("broche") and o.get("net") not in (None, "", "N/C"):
                    broches.setdefault(o["net"], {}).setdefault(trouve((nom, i)), set()).add(o["broche"])
    for net, groupes in sorted(broches.items()):
        if len(groupes) > 1:
            rap.erreurs.append(f"net {net} coupé en {len(groupes)} parties : " + " | ".join(", ".join(sorted(g)) for g in groupes.values()))
    # Vernis : ponts entre ouvertures
    for cote in ("F", "B"):
        masque = lire_gerber(trouver(f"-{cote}_Mask.gbr"))
        ouv = unary_union([o["geo"] for o in masque.objets])
        morceaux = [ouv] if ouv.geom_type == "Polygon" else list(ouv.geoms)
        arbre = STRtree(morceaux)
        for i, p in enumerate(morceaux):
            for j in arbre.query(p.buffer(PROPRE["pont_vernis"])):
                if j > i:
                    d = p.distance(morceaux[j])
                    if d < PROPRE["pont_vernis"]:
                        x, y = point_proche(p, morceaux[j])
                        rap.constat("pont_vernis", f"{cote}_Mask", x, y, d, "pont de vernis")
        # Sérigraphie
        seri = lire_gerber(trouver(f"-{cote}_Silkscreen.gbr"))
        for o in seri.objets:
            if o["genre"] == "trait" and o["largeur"] < PROPRE["trait_seri"]:
                rap.constat("trait_seri", f"{cote}_Silkscreen", o["a"][0], o["a"][1], o["largeur"], "trait fin")
            inter = o["geo"].intersection(ouv)
            if inter.area > 1e-6:
                c = inter.representative_point()
                rap.constat("seri_vernis", f"{cote}_Silkscreen", c.x, c.y, 0.0, "sérigraphie sur une ouverture du vernis")
            elif o["geo"].distance(ouv) < PROPRE["seri_vernis"]:
                x, y = point_proche(o["geo"], ouv)
                rap.propre.append(("seri_vernis", f"{cote}_Silkscreen", x, y, o["geo"].distance(ouv), PROPRE["seri_vernis"], "sérigraphie près d'une ouverture"))
            if not plaque.contains(o["geo"]) or o["geo"].distance(bords) < PROPRE["seri_bord"]:
                c = o["geo"].representative_point()
                rap.propre.append(("seri_bord", f"{cote}_Silkscreen", c.x, c.y, o["geo"].distance(bords), PROPRE["seri_bord"], "sérigraphie au bord"))
    if images:
        dessiner(resultats, couches, plaque, pth, npth, fichiers, trouver, os.path.join(images, os.path.basename(chemin).replace(".zip", "")))
    return rap


def dessiner(resultats, couches, plaque, pth, npth, fichiers, trouver, prefixe, echelle=24):
    from PIL import Image, ImageDraw
    l, h = int(62 * echelle), int(40 * echelle)

    def px(p):
        return ((p[0] + 1) * echelle, (39 - p[1]) * echelle)

    def couleur(net):
        if net is None:
            return (200, 200, 200)
        hsh = sum(ord(ch) * (i + 7) for i, ch in enumerate(net))
        return (80 + hsh * 37 % 170, 80 + hsh * 71 % 170, 80 + hsh * 113 % 170)

    def poly(d, p, c, fond):
        d.polygon([px(q) for q in p.exterior.coords], fill=c)
        for i in p.interiors:
            d.polygon([px(q) for q in i.coords], fill=fond)

    for nom, (polys_, nets, objets, elements) in resultats.items():
        img = Image.new("RGB", (l, h), (20, 30, 25))
        d = ImageDraw.Draw(img)
        poly(d, plaque if plaque.geom_type == "Polygon" else max(plaque.geoms, key=lambda q: q.area), (40, 60, 45), (20, 30, 25))
        for p, n in sorted(zip(polys_, nets), key=lambda pn: -pn[0].area):
            poly(d, p, couleur(sorted(n)[0]) if len(n) == 1 else (255, 0, 0), (40, 60, 45))
        for x, y, dd in pth:
            d.ellipse([px((x - dd / 2, y + dd / 2)), px((x + dd / 2, y - dd / 2))], fill=(0, 0, 0))
        for x, y, dd in npth:
            d.ellipse([px((x - dd / 2, y + dd / 2)), px((x + dd / 2, y - dd / 2))], fill=(0, 0, 80))
        if nom in ("F_Cu", "B_Cu"):
            cote = nom[0]
            seri = lire_gerber(trouver(f"-{cote}_Silkscreen.gbr"))
            for o in seri.objets:
                g = o["geo"]
                for q in ([g] if g.geom_type == "Polygon" else list(g.geoms)):
                    poly(d, q, (255, 255, 255), (255, 255, 255))
            masque = lire_gerber(trouver(f"-{cote}_Mask.gbr"))
            for o in masque.objets:
                d.line([px(q) for q in o["geo"].exterior.coords], fill=(255, 230, 0), width=1)
        img.save(f"{prefixe}-{nom}.png")


def main():
    args = sys.argv[1:]
    images = None
    if "--images" in args:
        i = args.index("--images")
        images = args[i + 1]
        os.makedirs(images, exist_ok=True)
        del args[i:i + 2]
    chemins = args or [os.path.join(RACINE, "public", "exemples", n) for n in ("typon-demo-revA.zip", "typon-demo-revB.zip")]
    echec = False
    for chemin in chemins:
        rap = verifier(chemin, images)
        print(f"== {rap.nom}")
        for e in rap.erreurs:
            print(f"   ERREUR : {e}")
        trouves = []
        attendu = lambda r, c, x, y: any(r == ar and c in ac and math.hypot(x - ax, y - ay) < 0.8 for ar, ac, ax, ay in ATTENDUS)
        for r, c, x, y, v, s, t in sorted(rap.dfm):
            ok = attendu(r, c, x, y)
            trouves.append((r, c, x, y))
            print(f"   DFM {'(volontaire)' if ok else 'INATTENDU'} : {r} sur {c} en ({x:.2f} ; {y:.2f}) : {v:.3f} < {s} ({t})")
            echec |= not ok
        for ar, ac, ax, ay in ATTENDUS:
            for couche in ac:
                if not any(r == ar and c == couche and math.hypot(x - ax, y - ay) < 0.8 for r, c, x, y in trouves):
                    print(f"   MANQUANT : défaut volontaire {ar} sur {couche} en ({ax} ; {ay}) non trouvé")
                    echec = True
        # Les abords immédiats d'un défaut volontaire ne comptent pas comme manque de propreté
        propres = [q for q in rap.propre if not attendu(q[0], q[1], q[2], q[3])]
        for r, c, x, y, v, s, t in propres:
            print(f"   PROPRETÉ : {r} sur {c} en ({x:.2f} ; {y:.2f}) : {v:.3f} < {s} ({t})")
        echec |= bool(rap.erreurs or propres)
    print("\nConforme : seuls les défauts volontaires sont présents." if not echec else "\nNON CONFORME.")
    sys.exit(1 if echec else 0)


if __name__ == "__main__":
    main()
