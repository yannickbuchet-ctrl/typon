//! Lecture des fichiers IPC-2581 (révisions A, B et C ; XML) : couches et empilage, profil de la carte, éléments
//! graphiques (traits, arcs, polylignes, contours, pastilles par primitives standard ou utilisateur, piles de
//! pastilles), perçages et rainures, composants et boîtiers, nomenclature, nets, en-tête logistique.
//!
//! Le résultat prend la même forme que celui de l'ODB++ (odb::Odb), pour être présenté de la même façon.

use crate::couches::{self, Cote, Genre, Role};
use crate::geom::*;
use crate::gerber::{enveloppe_convexe, nombre};
use crate::odb::{Composant, CoucheOdb, Odb, OutilOdb};
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

/// Le texte est-il un fichier IPC-2581 ?
pub fn reconnaitre(debut: &str) -> bool {
    let d: String = debut.chars().take(4000).collect();
    d.contains("<IPC-2581")
}

fn facteur_unite(u: Option<&str>) -> f64 {
    match u.unwrap_or("MILLIMETER").to_ascii_uppercase().as_str() {
        "MICRON" => 0.001,
        "INCH" => 25.4,
        "MILS" | "MIL" => 0.0254,
        _ => 1.0,
    }
}

fn enfants<'a, 'i>(n: Node<'a, 'i>, tag: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == tag)
}

fn enfant<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == tag)
}

fn f(n: &Node, a: &str) -> f64 {
    n.attribute(a).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0)
}

fn b(n: &Node, a: &str) -> bool {
    n.attribute(a).map(|v| v.eq_ignore_ascii_case("true") || v == "1").unwrap_or(false)
}

/// Description d'un trait : largeur (mm) et extrémités.
#[derive(Clone, Copy)]
struct Trait {
    largeur: f64,
    bout_carre: bool,
}

/// Une primitive (standard ou utilisateur) déjà convertie en forme, centrée sur son origine, en mm.
type Forme = Vec<(Prim, bool)>;

struct Lecteur<'a, 'i> {
    /// Facteur des coordonnées de la conception (CadHeader).
    k: f64,
    standard: HashMap<String, (Node<'a, 'i>, f64)>,
    utilisateur: HashMap<String, (Node<'a, 'i>, f64)>,
    traits: HashMap<String, Trait>,
    remplissages: HashMap<String, bool>,
    formes: HashMap<String, Forme>,
    piles: HashMap<String, Node<'a, 'i>>,
    alertes: Vec<String>,
    textes_ignores: usize,
}

impl<'a, 'i> Lecteur<'a, 'i> {
    fn alerte(&mut self, m: String) {
        if self.alertes.len() < 30 && !self.alertes.contains(&m) {
            self.alertes.push(m);
        }
    }

    /// Transformation d'un élément Xform : décalage, miroir (x → −x), rotation (sens trigonométrique), échelle.
    fn xform(&self, n: Option<Node>, k: f64) -> Transfo {
        let Some(x) = n else { return Transfo::IDENTITE };
        let echelle = x.attribute("scale").and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
        Transfo::translation(f(&x, "xOffset") * k, f(&x, "yOffset") * k)
            .apres(&Transfo::rotation(f(&x, "rotation").to_radians()))
            .apres(&Transfo::miroir(b(&x, "mirror"), false))
            .apres(&Transfo::echelle(echelle))
    }

    fn trait_de(&self, n: Node, k: f64) -> Trait {
        if let Some(ld) = enfant(n, "LineDesc") {
            return Trait { largeur: f(&ld, "lineWidth") * k, bout_carre: ld.attribute("lineEnd") == Some("SQUARE") };
        }
        if let Some(r) = enfant(n, "LineDescRef") {
            if let Some(t) = r.attribute("id").and_then(|id| self.traits.get(id)) {
                return *t;
            }
        }
        Trait { largeur: 0.0, bout_carre: false }
    }

    /// La primitive est-elle « creuse » (FillDesc HOLLOW) : on n'en dessine que le contour ?
    fn creuse(&self, n: Node) -> bool {
        if let Some(fd) = enfant(n, "FillDesc") {
            return fd.attribute("fillProperty") == Some("HOLLOW");
        }
        if let Some(r) = enfant(n, "FillDescRef") {
            return r.attribute("id").and_then(|id| self.remplissages.get(id)).copied().unwrap_or(false);
        }
        false
    }

    /// Contour d'un élément Polygon ou Cutout (PolyBegin, PolyStepSegment, PolyStepCurve), en mm.
    fn contour(&self, n: Node, k: f64) -> Vec<Pt> {
        let mut pts: Vec<Pt> = Vec::new();
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "PolyBegin" | "PolyStepSegment" => pts.push(pt(f(&c, "x") * k, f(&c, "y") * k)),
                "PolyStepCurve" => {
                    let fin = pt(f(&c, "x") * k, f(&c, "y") * k);
                    let centre = pt(f(&c, "centerX") * k, f(&c, "centerY") * k);
                    let debut = *pts.last().unwrap_or(&fin);
                    let a = arc(debut, fin, centre, b(&c, "clockwise"));
                    pts.extend_from_slice(&a[1..]);
                }
                _ => {}
            }
        }
        pts
    }

    fn tracer(&self, pts: &[Pt], t: Trait, sortie: &mut Forme) {
        let r = t.largeur / 2.0;
        for w in pts.windows(2) {
            if t.bout_carre && r > 0.0 {
                let mut coins = Vec::new();
                for p in w {
                    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        coins.push(pt(p.x + sx * r, p.y + sy * r));
                    }
                }
                sortie.push((Prim::Poly { anneaux: vec![enveloppe_convexe(coins)] }, true));
            } else {
                sortie.push((Prim::Cap { a: w[0], b: w[1], r }, true));
            }
        }
    }

    /// Forme d'une primitive standard (Circle, RectCenter…), avec son Xform éventuel, en mm (facteur k).
    fn standard_forme(&mut self, n: Node, k: f64) -> Forme {
        let o = pt(0.0, 0.0);
        let g = |a: &str| f(&n, a) * k;
        let poly = |pts: Vec<Pt>| vec![(Prim::Poly { anneaux: vec![pts] }, true)];
        let mut forme: Forme = match n.tag_name().name() {
            "Circle" => vec![(Prim::Cap { a: o, b: o, r: g("diameter") / 2.0 }, true)],
            "RectCenter" => vec![(Prim::Boite { c: o, hw: g("width") / 2.0, hh: g("height") / 2.0, ang: 0.0, rad: 0.0 }, true)],
            "RectCorner" => {
                let (x0, y0, x1, y1) = (g("lowerLeftX"), g("lowerLeftY"), g("upperRightX"), g("upperRightY"));
                vec![(Prim::Boite { c: pt((x0 + x1) / 2.0, (y0 + y1) / 2.0), hw: (x1 - x0).abs() / 2.0, hh: (y1 - y0).abs() / 2.0, ang: 0.0, rad: 0.0 }, true)]
            }
            "RectRound" | "RectCham" => {
                let (w, h) = (g("width"), g("height"));
                let r = if n.tag_name().name() == "RectRound" { g("radius") } else { g("chamfer") };
                let coins = [("upperRight", 1u8), ("upperLeft", 2), ("lowerLeft", 4), ("lowerRight", 8)];
                // Sans attributs de coins, tous les coins sont concernés.
                let precises = coins.iter().any(|(a, _)| n.attribute(*a).is_some());
                let masque = if precises { coins.iter().filter(|(a, _)| b(&n, a)).fold(0u8, |m, (_, bit)| m | bit) } else { 0b1111 };
                if n.tag_name().name() == "RectRound" && masque == 0b1111 {
                    vec![(Prim::Boite { c: o, hw: w / 2.0, hh: h / 2.0, ang: 0.0, rad: r }, true)]
                } else {
                    poly(rectangle_coins(o, w, h, r, n.tag_name().name() == "RectCham", masque))
                }
            }
            "Oval" => {
                let (w, h) = (g("width"), g("height"));
                vec![(Prim::Boite { c: o, hw: w / 2.0, hh: h / 2.0, ang: 0.0, rad: w.min(h) / 2.0 }, true)]
            }
            "Diamond" => {
                let (w, h) = (g("width"), g("height"));
                poly(vec![pt(w / 2.0, 0.0), pt(0.0, h / 2.0), pt(-w / 2.0, 0.0), pt(0.0, -h / 2.0)])
            }
            "Ellipse" => {
                let (w, h) = (g("width"), g("height"));
                let m = segments_cercle(w.max(h) / 2.0, 2.0 * PI);
                poly((0..m).map(|i| {
                    let a = 2.0 * PI * i as f64 / m as f64;
                    pt(w / 2.0 * a.cos(), h / 2.0 * a.sin())
                }).collect())
            }
            "Hexagon" => poly(polygone_regulier(o, g("length"), 6, 0.0)),
            "Octagon" => poly(polygone_regulier(o, g("length"), 8, PI / 8.0)),
            "Triangle" => {
                let (bse, h) = (g("base"), g("height"));
                poly(vec![pt(-bse / 2.0, -h / 2.0), pt(bse / 2.0, -h / 2.0), pt(0.0, h / 2.0)])
            }
            "Donut" | "Thermal" => {
                let forme = n.attribute("shape").unwrap_or("ROUND");
                let (de, di) = (g("outerDiameter"), g("innerDiameter"));
                let figure = |d: f64| match forme {
                    "SQUARE" => rectangle_coins(o, d, d, 0.0, false, 0),
                    "HEXAGON" => polygone_regulier(o, d, 6, 0.0),
                    "OCTAGON" => polygone_regulier(o, d, 8, PI / 8.0),
                    _ => cercle(o, d / 2.0),
                };
                if n.tag_name().name() == "Donut" {
                    vec![(Prim::Poly { anneaux: vec![figure(de), figure(di)] }, true)]
                } else {
                    let branches = n.attribute("spokeCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(4);
                    let angle = f(&n, "spokeStartAngle").to_radians();
                    thermique(o, &figure(de), &figure(di), branches, angle, g("gap")).into_iter().map(|p| (p, true)).collect()
                }
            }
            "Butterfly" => {
                let carre = n.attribute("shape") == Some("SQUARE");
                let d = if carre { g("side") } else { g("diameter") };
                let r = d / 2.0;
                if carre {
                    vec![(Prim::Poly { anneaux: vec![vec![o, pt(r, 0.0), pt(r, r), pt(0.0, r)]] }, true), (Prim::Poly { anneaux: vec![vec![o, pt(-r, 0.0), pt(-r, -r), pt(0.0, -r)]] }, true)]
                } else {
                    let sect = |a0: f64| {
                        let mut p = vec![o];
                        p.extend(arc(pt(r * a0.cos(), r * a0.sin()), pt(r * (a0 + PI / 2.0).cos(), r * (a0 + PI / 2.0).sin()), o, false));
                        (Prim::Poly { anneaux: vec![p] }, true)
                    };
                    vec![sect(0.0), sect(PI)]
                }
            }
            "Moire" => {
                let mut r = Vec::new();
                let (d, rw, rg) = (g("diameter"), g("ringWidth"), g("ringGap"));
                let nb = n.attribute("ringNumber").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                let mut e = d / 2.0;
                for _ in 0..nb.max(1) {
                    if e <= 0.0 {
                        break;
                    }
                    r.push((Prim::Poly { anneaux: vec![cercle(o, e), cercle(o, (e - rw).max(0.0))] }, true));
                    e -= rw + rg;
                }
                let (lw, ll) = (g("lineWidth"), g("lineLength"));
                let t = Transfo::rotation(f(&n, "lineAngle").to_radians());
                r.push((Prim::Boite { c: o, hw: ll / 2.0, hh: lw / 2.0, ang: 0.0, rad: 0.0 }.transformee(&t), true));
                r.push((Prim::Boite { c: o, hw: lw / 2.0, hh: ll / 2.0, ang: 0.0, rad: 0.0 }.transformee(&t), true));
                r
            }
            "Contour" => {
                let mut anneaux = Vec::new();
                if let Some(p) = enfant(n, "Polygon") {
                    let t = self.xform(enfant(p, "Xform"), k);
                    anneaux.push(self.contour(p, k).into_iter().map(|q| t.applique(q)).collect::<Vec<_>>());
                }
                for c in enfants(n, "Cutout") {
                    let t = self.xform(enfant(c, "Xform"), k);
                    anneaux.push(self.contour(c, k).into_iter().map(|q| t.applique(q)).collect());
                }
                if anneaux.is_empty() { Vec::new() } else { vec![(Prim::Poly { anneaux }, true)] }
            }
            autre => {
                self.alerte(format!("Primitive IPC-2581 « {autre} » non prise en charge."));
                Vec::new()
            }
        };
        // Primitive « creuse » : seulement son contour, avec la largeur de son LineDesc.
        if self.creuse(n) {
            let t = self.trait_de(n, k);
            let mut contour = Vec::new();
            for (p, _) in &forme {
                for a in contours(p) {
                    let mut pts: Vec<Pt> = a.iter().map(|q| pt(q[0], q[1])).collect();
                    if let Some(&premier) = pts.first() {
                        pts.push(premier);
                    }
                    self.tracer(&pts, t, &mut contour);
                }
            }
            forme = contour;
        }
        let t = self.xform(enfant(n, "Xform"), k);
        forme.into_iter().map(|(p, e)| (p.transformee(&t), e)).collect()
    }

    /// Éléments graphiques d'un conteneur (UserSpecial, Features…) : traits, arcs, polylignes, polygones, primitives.
    fn elements(&mut self, n: Node<'a, 'i>, k: f64, sortie: &mut Forme) {
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "Line" => {
                    let t = self.trait_de(c, k);
                    self.tracer(&[pt(f(&c, "startX") * k, f(&c, "startY") * k), pt(f(&c, "endX") * k, f(&c, "endY") * k)], t, sortie);
                }
                "Arc" => {
                    let t = self.trait_de(c, k);
                    let pts = arc(pt(f(&c, "startX") * k, f(&c, "startY") * k), pt(f(&c, "endX") * k, f(&c, "endY") * k), pt(f(&c, "centerX") * k, f(&c, "centerY") * k), b(&c, "clockwise"));
                    self.tracer(&pts, t, sortie);
                }
                "Polyline" => {
                    let t = self.trait_de(c, k);
                    let pts = self.contour(c, k);
                    self.tracer(&pts, t, sortie);
                }
                "Outline" => {
                    if let Some(p) = enfant(c, "Polygon") {
                        let t = self.trait_de(c, k);
                        let mut pts = self.contour(p, k);
                        if let Some(&q) = pts.first() {
                            pts.push(q);
                        }
                        self.tracer(&pts, t, sortie);
                    }
                }
                "Polygon" => {
                    let pts = self.contour(c, k);
                    if self.creuse(c) {
                        let t = self.trait_de(c, k);
                        self.tracer(&pts, t, sortie);
                    } else if pts.len() >= 3 {
                        sortie.push((Prim::Poly { anneaux: vec![pts] }, true));
                    }
                }
                "UserSpecial" => self.elements(c, k, sortie),
                "StandardPrimitiveRef" | "UserPrimitiveRef" => {
                    if let Some(id) = c.attribute("id") {
                        let forme = self.forme_ref(c.tag_name().name() == "UserPrimitiveRef", id);
                        sortie.extend(forme);
                    }
                }
                "Text" => self.textes_ignores += 1,
                "Xform" | "Location" | "LineDesc" | "LineDescRef" | "FillDesc" | "FillDescRef" | "ColorRef" | "PinRef" => {}
                nom_primitive => {
                    let forme = self.standard_forme(c, k);
                    let _ = nom_primitive;
                    sortie.extend(forme);
                }
            }
        }
    }

    /// Forme d'une primitive du dictionnaire (standard ou utilisateur), mise en cache.
    fn forme_ref(&mut self, utilisateur: bool, id: &str) -> Forme {
        let cle = format!("{}{id}", if utilisateur { "u:" } else { "s:" });
        if let Some(f) = self.formes.get(&cle) {
            return f.clone();
        }
        // Garde contre une primitive qui se référence elle-même (directement ou non) : vide pendant le calcul.
        self.formes.insert(cle.clone(), Vec::new());
        let entree = if utilisateur { self.utilisateur.get(id).copied() } else { self.standard.get(id).copied() };
        let forme = match entree {
            Some((n, k)) => {
                if utilisateur {
                    let mut s = Vec::new();
                    self.elements(n, k, &mut s);
                    s
                } else {
                    n.children().find(|c| c.is_element()).map(|p| self.standard_forme(p, k)).unwrap_or_default()
                }
            }
            None => {
                self.alerte(format!("Primitive « {id} » absente du dictionnaire."));
                Vec::new()
            }
        };
        self.formes.insert(cle, forme.clone());
        forme
    }

    /// Forme d'une pastille ou d'un élément : Xform + Location du parent, puis la primitive (référence ou intégrée).
    fn placement(&self, n: Node, k: f64) -> Transfo {
        let l = enfant(n, "Location").map(|l| (f(&l, "x") * k, f(&l, "y") * k)).unwrap_or((0.0, 0.0));
        Transfo::translation(l.0, l.1).apres(&self.xform(enfant(n, "Xform"), k))
    }
}

/// Lit un fichier IPC-2581.
pub fn lire(texte: &str) -> Result<Odb, String> {
    let doc = Document::parse(texte).map_err(|e| format!("Fichier IPC-2581 illisible : {e}"))?;
    let racine = doc.root_element();
    if racine.tag_name().name() != "IPC-2581" {
        return Err("Ce fichier XML n'est pas un fichier IPC-2581.".into());
    }
    let revision = racine.attribute("revision").unwrap_or("?").to_string();
    let contenu = enfant(racine, "Content");
    let ecad = enfant(racine, "Ecad").ok_or("Fichier IPC-2581 sans données de conception (élément Ecad).")?;
    let cad_data = enfant(ecad, "CadData").ok_or("Fichier IPC-2581 sans CadData.")?;
    let k = facteur_unite(enfant(ecad, "CadHeader").and_then(|h| h.attribute("units")));

    let mut l = Lecteur { k, standard: HashMap::new(), utilisateur: HashMap::new(), traits: HashMap::new(), remplissages: HashMap::new(), formes: HashMap::new(), piles: HashMap::new(), alertes: Vec::new(), textes_ignores: 0 };

    // Dictionnaires
    if let Some(c) = contenu {
        for d in c.children().filter(|x| x.is_element()) {
            let kd = facteur_unite(d.attribute("units"));
            match d.tag_name().name() {
                "DictionaryStandard" => {
                    for e in enfants(d, "EntryStandard") {
                        if let Some(id) = e.attribute("id") {
                            l.standard.insert(id.to_string(), (e, kd));
                        }
                    }
                }
                "DictionaryUser" => {
                    for e in enfants(d, "EntryUser") {
                        if let (Some(id), Some(us)) = (e.attribute("id"), enfant(e, "UserSpecial")) {
                            l.utilisateur.insert(id.to_string(), (us, kd));
                        }
                    }
                }
                "DictionaryLineDesc" => {
                    for e in enfants(d, "EntryLineDesc") {
                        if let (Some(id), Some(ld)) = (e.attribute("id"), enfant(e, "LineDesc")) {
                            l.traits.insert(id.to_string(), Trait { largeur: f(&ld, "lineWidth") * kd, bout_carre: ld.attribute("lineEnd") == Some("SQUARE") });
                        }
                    }
                }
                "DictionaryFillDesc" => {
                    for e in enfants(d, "EntryFillDesc") {
                        if let (Some(id), Some(fd)) = (e.attribute("id"), enfant(e, "FillDesc")) {
                            l.remplissages.insert(id.to_string(), fd.attribute("fillProperty") == Some("HOLLOW"));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Informations : révision, mode, logiciel, auteur, conception
    let mut info: Vec<(String, String)> = vec![("Format".into(), format!("IPC-2581 révision {revision}"))];
    if let Some(m) = contenu.and_then(|c| enfant(c, "FunctionMode")) {
        info.push(("Mode".into(), traduire_mode(m.attribute("mode").unwrap_or(""))));
    }
    info.push(("Conception".into(), ecad.attribute("name").unwrap_or("").to_string()));
    if let Some(lh) = enfant(racine, "LogisticHeader") {
        if let Some(e) = enfant(lh, "Enterprise") {
            info.push(("Entreprise".into(), e.attribute("name").unwrap_or("").to_string()));
        }
        if let Some(p) = enfant(lh, "Person") {
            info.push(("Auteur".into(), p.attribute("name").unwrap_or("").to_string()));
        }
    }
    if let Some(h) = enfant(racine, "HistoryRecord") {
        if let Some(d) = h.attribute("origination") {
            info.push(("Date de création".into(), d.replace('T', " ").chars().take(16).collect()));
        }
        if let Some(d) = h.attribute("lastChange") {
            info.push(("Dernière modification".into(), d.replace('T', " ").chars().take(16).collect()));
        }
        if let Some(sp) = enfant(h, "FileRevision").and_then(|r| enfant(r, "SoftwarePackage")) {
            let logiciel = [sp.attribute("vendor"), sp.attribute("name"), sp.attribute("revision")].iter().flatten().copied().collect::<Vec<_>>().join(" ");
            info.push(("Logiciel d'origine".into(), logiciel));
        }
    }
    info.push(("Unités".into(), match enfant(ecad, "CadHeader").and_then(|h| h.attribute("units")).unwrap_or("MILLIMETER") {
        "INCH" => "pouces".into(),
        "MICRON" => "microns".into(),
        _ => "millimètres".into(),
    }));
    info.retain(|(_, v)| !v.trim().is_empty());

    // Couches, dans l'ordre du fichier (du dessus vers le dessous en général)
    let couches_xml: Vec<Node> = enfants(cad_data, "Layer").collect();
    let fonctions: Vec<(String, String, String, bool)> = couches_xml
        .iter()
        .map(|c| (c.attribute("name").unwrap_or("").to_string(), c.attribute("layerFunction").unwrap_or("").to_ascii_uppercase(), c.attribute("side").unwrap_or("").to_ascii_uppercase(), c.attribute("polarity") == Some("NEGATIVE")))
        .collect();
    let cuivres: Vec<&String> = fonctions.iter().filter(|(_, fct, _, _)| est_cuivre(fct)).map(|(n, ..)| n).collect();

    // Étape : celle de StepRef, sinon la première.
    let nom_etape = contenu.and_then(|c| enfant(c, "StepRef")).and_then(|s| s.attribute("name")).map(|s| s.to_string());
    let etapes: Vec<Node> = enfants(cad_data, "Step").collect();
    let etape = nom_etape.as_ref().and_then(|n| etapes.iter().find(|s| s.attribute("name") == Some(n.as_str())).copied()).or(etapes.first().copied()).ok_or("Fichier IPC-2581 sans étape (Step).")?;
    for p in enfants(etape, "PadStackDef") {
        if let Some(n) = p.attribute("name") {
            l.piles.insert(n.to_string(), p);
        }
    }

    // Dessins par couche
    let mut dessins: BTreeMap<String, Dessin> = BTreeMap::new();
    let mut comptes: HashMap<String, usize> = HashMap::new();
    let mut trous: HashMap<String, Vec<(f64, String)>> = HashMap::new();
    let mut nets_set: BTreeMap<String, usize> = BTreeMap::new();
    for lf in enfants(etape, "LayerFeature") {
        let couche = lf.attribute("layerRef").unwrap_or("").to_string();
        let negative = fonctions.iter().find(|(n, ..)| *n == couche).map(|x| x.3).unwrap_or(false);
        let mut d = dessins.remove(&couche).unwrap_or_default();
        let mut nb = 0usize;
        for set in enfants(lf, "Set") {
            let net = set.attribute("net").unwrap_or("").to_string();
            let sombre = set.attribute("polarity").map(|p| p != "NEGATIVE").unwrap_or(true) != negative;
            let usage = set.attribute("padUsage").unwrap_or("");
            if !net.is_empty() && net != "No Net" {
                *nets_set.entry(net.clone()).or_default() += 1;
            }
            for e in set.children().filter(|c| c.is_element()) {
                let genre = e.tag_name().name();
                let mut forme: Forme = Vec::new();
                let mut info: Info = Vec::new();
                match genre {
                    "Features" => {
                        let t = l.placement(e, k);
                        l.elements(e, k, &mut forme);
                        forme = forme.into_iter().map(|(p, x)| (p.transformee(&t), x)).collect();
                        info.push(("Objet".into(), "Élément".into()));
                    }
                    "Pad" => {
                        let t = l.placement(e, k);
                        if let Some(pile) = e.attribute("padstackDefRef").and_then(|p| l.piles.get(p).copied()) {
                            // Pile de pastilles (KiCad) : la pastille de cette couche.
                            for pd in enfants(pile, "PadstackPadDef").filter(|pd| pd.attribute("layerRef") == Some(couche.as_str())) {
                                let tp = l.placement(pd, k);
                                let mut f2 = Vec::new();
                                l.elements(pd, k, &mut f2);
                                forme.extend(f2.into_iter().map(|(p, x)| (p.transformee(&tp), x)));
                            }
                        }
                        let mut f2 = Vec::new();
                        l.elements(e, k, &mut f2);
                        forme.extend(f2);
                        forme = forme.into_iter().map(|(p, x)| (p.transformee(&t), x)).collect();
                        info.push(("Objet".into(), "Pastille".into()));
                        if let Some(pr) = enfant(e, "PinRef") {
                            info.push(("Composant".into(), pr.attribute("componentRef").unwrap_or("").into()));
                            info.push(("Broche".into(), pr.attribute("pin").unwrap_or("").into()));
                        }
                    }
                    "Hole" => {
                        let c = pt(f(&e, "x") * k, f(&e, "y") * k);
                        let dia = f(&e, "diameter") * k;
                        let plaque = e.attribute("platingStatus").unwrap_or("").to_string();
                        forme.push((Prim::Cap { a: c, b: c, r: dia / 2.0 }, true));
                        trous.entry(couche.clone()).or_default().push((dia, plaque.clone()));
                        info.push(("Objet".into(), "Trou".into()));
                        info.push(("Diamètre".into(), format!("{} mm", nombre(dia))));
                        info.push(("Métallisation".into(), traduire_metallisation(&plaque)));
                    }
                    "SlotCavity" => {
                        let t = l.placement(e, k);
                        if let Some(p) = enfant(e, "Outline").and_then(|o| enfant(o, "Polygon")) {
                            let pts = l.contour(p, k);
                            forme.push((Prim::Poly { anneaux: vec![pts] }.transformee(&t), true));
                        }
                        info.push(("Objet".into(), "Rainure".into()));
                        info.push(("Métallisation".into(), traduire_metallisation(e.attribute("platingStatus").unwrap_or(""))));
                    }
                    _ => continue,
                }
                if forme.is_empty() {
                    continue;
                }
                if !net.is_empty() {
                    info.push(("Net".into(), net.clone()));
                }
                if !usage.is_empty() && usage != "NONE" {
                    info.push(("Usage".into(), usage.to_ascii_lowercase()));
                }
                if let Some(geo) = set.attribute("geometry") {
                    info.push(("Géométrie".into(), geo.into()));
                }
                let objet = d.nouvel_objet(info);
                for (p, expose) in forme {
                    d.ajoute(p, if expose { sombre } else { !sombre }, objet);
                }
                nb += 1;
            }
        }
        *comptes.entry(couche.clone()).or_default() += nb;
        dessins.insert(couche, d);
    }

    // Révision A (Altium) : piles de pastilles placées (PadStack, LayerPad, LayerHole)
    for ps in enfants(etape, "PadStack") {
        let net = ps.attribute("net").unwrap_or("").to_string();
        for lp in enfants(ps, "LayerPad") {
            let couche = lp.attribute("layerRef").unwrap_or("").to_string();
            let t = l.placement(lp, k);
            let mut forme = Vec::new();
            l.elements(lp, k, &mut forme);
            let d = dessins.entry(couche.clone()).or_default();
            let mut info: Info = vec![("Objet".into(), "Pastille".into())];
            if let Some(pr) = enfant(lp, "PinRef") {
                info.push(("Composant".into(), pr.attribute("componentRef").unwrap_or("").into()));
                info.push(("Broche".into(), pr.attribute("pin").unwrap_or("").into()));
            }
            if !net.is_empty() {
                info.push(("Net".into(), net.clone()));
            }
            let objet = d.nouvel_objet(info);
            for (p, _) in forme {
                d.ajoute(p.transformee(&t), true, objet);
            }
            *comptes.entry(couche).or_default() += 1;
        }
        for lh in enfants(ps, "LayerHole") {
            // Le trou va sur la couche de perçage déclarée, ou sur une couche créée pour l'occasion.
            let couche = fonctions.iter().find(|(_, fct, ..)| fct == "DRILL").map(|(n, ..)| n.clone()).unwrap_or_else(|| "Perçages (piles de pastilles)".into());
            let c = pt(f(&lh, "x") * k, f(&lh, "y") * k);
            let dia = f(&lh, "diameter") * k;
            let d = dessins.entry(couche.clone()).or_default();
            let objet = d.nouvel_objet(vec![("Objet".into(), "Trou".into()), ("Diamètre".into(), format!("{} mm", nombre(dia)))]);
            d.ajoute(Prim::Cap { a: c, b: c, r: dia / 2.0 }, true, objet);
            trous.entry(couche.clone()).or_default().push((dia, lh.attribute("platingStatus").unwrap_or("").to_string()));
            *comptes.entry(couche).or_default() += 1;
        }
    }

    // Couches au format commun
    let mut couches: Vec<CoucheOdb> = Vec::new();
    let mut deja: Vec<String> = Vec::new();
    for (nom_c, fct, cote_c, negative) in &fonctions {
        if fct.starts_with("DIEL") || fct == "STACKUP" {
            continue;
        }
        let numero = cuivres.iter().position(|c| *c == nom_c).map(|i| i as u32 + 1);
        let mut role = role_ipc(fct, cote_c, numero, cuivres.len(), nom_c);
        if fct == "CONDUCTOR" && cote_c == "NONE" {
            continue; // âme diélectrique écrite comme conducteur (Altium, révision A)
        }
        let dessin = dessins.remove(nom_c).unwrap_or_default();
        let outils = outils_de(trous.get(nom_c));
        if role.genre == Genre::Percage && !outils.is_empty() {
            let non = outils.iter().all(|t| t.genre == "non métallisé");
            role.libelle = format!("Perçage {} « {nom_c} »", if non { "non métallisé" } else { "métallisé" });
        }
        deja.push(nom_c.clone());
        couches.push(CoucheOdb {
            nom: nom_c.clone(),
            type_odb: fct.clone(),
            contexte: "BOARD".into(),
            polarite_negative: *negative,
            debut: String::new(),
            fin: String::new(),
            role,
            elements: *comptes.get(nom_c).unwrap_or(&0),
            dessin,
            outils,
        });
    }
    // Couches dessinées mais non déclarées (perçages des piles de pastilles)
    for (nom_c, dessin) in dessins {
        if deja.contains(&nom_c) || dessin.elements.is_empty() {
            continue;
        }
        let mut role = couches::depuis_x2(&["Plated".into()]).unwrap();
        role.libelle = nom_c.clone();
        role.source = "IPC-2581";
        couches.push(CoucheOdb { nom: nom_c.clone(), type_odb: "DRILL".into(), contexte: "BOARD".into(), polarite_negative: false, debut: String::new(), fin: String::new(), role, elements: *comptes.get(&nom_c).unwrap_or(&0), outils: outils_de(trous.get(&nom_c)), dessin });
    }

    // Profil de la carte
    if let Some(p) = enfant(etape, "Profile") {
        let mut d = Dessin::default();
        let objet = d.nouvel_objet(vec![("Objet".into(), "Contour de la carte (profil IPC-2581)".into())]);
        let mut anneaux = Vec::new();
        if let Some(pg) = enfant(p, "Polygon") {
            anneaux.push(l.contour(pg, k));
        }
        for c in enfants(p, "Cutout") {
            anneaux.push(l.contour(c, k));
        }
        for a in &anneaux {
            for w in a.windows(2) {
                d.ajoute(Prim::Cap { a: w[0], b: w[1], r: 0.0 }, true, objet);
            }
        }
        if !d.elements.is_empty() {
            let mut role = couches::depuis_x2(&["Profile".into()]).unwrap();
            role.source = "IPC-2581";
            couches.push(CoucheOdb { nom: "profile".into(), type_odb: "PROFILE".into(), contexte: "BOARD".into(), polarite_negative: false, debut: String::new(), fin: String::new(), role, elements: 1, dessin: d, outils: Vec::new() });
        }
    }

    // Boîtiers et composants
    let mut boitiers: HashMap<String, Forme> = HashMap::new();
    let mut hauteurs_boitiers: HashMap<String, f64> = HashMap::new();
    for pk in enfants(etape, "Package") {
        if let Some(h) = pk.attribute("height").and_then(|v| v.parse::<f64>().ok()) {
            hauteurs_boitiers.insert(pk.attribute("name").unwrap_or("").to_string(), h * k);
        }
        let mut forme = Vec::new();
        if let Some(o) = enfant(pk, "Outline") {
            if let Some(pg) = enfant(o, "Polygon") {
                let mut pts = l.contour(pg, k);
                if let Some(&q) = pts.first() {
                    pts.push(q);
                }
                l.tracer(&pts, Trait { largeur: 0.0, bout_carre: false }, &mut forme);
            }
        }
        for pin in enfants(pk, "Pin") {
            let t = l.placement(pin, k);
            let mut f2 = Vec::new();
            l.elements(pin, k, &mut f2);
            forme.extend(f2.into_iter().map(|(p, x)| (p.transformee(&t), x)));
        }
        boitiers.insert(pk.attribute("name").unwrap_or("").to_string(), forme);
    }
    // Valeurs et descriptions de la nomenclature
    let mut valeurs: HashMap<String, (String, String)> = HashMap::new();
    for bom in enfants(racine, "Bom") {
        for item in enfants(bom, "BomItem") {
            let mut valeur = String::new();
            if let Some(ch) = enfant(item, "Characteristics") {
                for t in enfants(ch, "Textual") {
                    if t.attribute("textualCharacteristicName").map(|n| n.eq_ignore_ascii_case("value")).unwrap_or(false) {
                        valeur = t.attribute("textualCharacteristicValue").unwrap_or("").to_string();
                    }
                }
            }
            let description = item.attribute("description").unwrap_or("").to_string();
            for rd in enfants(item, "RefDes") {
                valeurs.insert(rd.attribute("name").unwrap_or("").to_string(), (valeur.clone(), description.clone()));
            }
        }
    }
    let cotes: HashMap<&str, &str> = fonctions.iter().map(|(n, _, s, _)| (n.as_str(), s.as_str())).collect();
    let mut composants = Vec::new();
    let mut dessin_dessus = Dessin::default();
    let mut dessin_dessous = Dessin::default();
    for c in enfants(etape, "Component") {
        let reference = c.attribute("refDes").unwrap_or("").to_string();
        let boitier = c.attribute("packageRef").unwrap_or("").to_string();
        let couche = c.attribute("layerRef").unwrap_or("");
        let xf = enfant(c, "Xform");
        let miroir = xf.map(|x| b(&x, "mirror")).unwrap_or(false);
        let rotation = xf.map(|x| f(&x, "rotation")).unwrap_or(0.0);
        let dessous = miroir || cotes.get(couche).map(|s| *s == "BOTTOM").unwrap_or(false) || couche.to_ascii_lowercase().starts_with("b.") || couche.to_ascii_lowercase().contains("bot");
        let loc = enfant(c, "Location").map(|p| (f(&p, "x") * k, f(&p, "y") * k)).unwrap_or((0.0, 0.0));
        let (mut valeur, description) = valeurs.get(&reference).cloned().unwrap_or_default();
        if valeur.is_empty() {
            if let Some(a) = enfants(c, "NonstandardAttribute").find(|a| a.attribute("name").map(|n| n.eq_ignore_ascii_case("value")).unwrap_or(false)) {
                valeur = a.attribute("value").unwrap_or("").to_string();
            }
        }
        let mut proprietes = vec![("montage".to_string(), c.attribute("mountType").unwrap_or("").to_lowercase())];
        if !description.is_empty() {
            proprietes.push(("description".into(), description));
        }
        let t = Transfo::translation(loc.0, loc.1).apres(&Transfo::rotation(rotation.to_radians())).apres(&Transfo::miroir(miroir, false));
        let d = if dessous { &mut dessin_dessous } else { &mut dessin_dessus };
        let objet = d.nouvel_objet(vec![
            ("Objet".into(), "Composant".into()),
            ("Référence".into(), reference.clone()),
            ("Valeur".into(), valeur.clone()),
            ("Pièce".into(), c.attribute("part").unwrap_or("").to_string()),
            ("Boîtier".into(), boitier.clone()),
            ("Rotation".into(), format!("{}° (sens trigonométrique)", nombre(rotation))),
            ("Côté".into(), if dessous { "dessous".into() } else { "dessus".into() }),
        ]);
        match boitiers.get(&boitier) {
            Some(f) if !f.is_empty() => {
                for (p, _) in f {
                    d.ajoute(p.transformee(&t), true, objet);
                }
            }
            _ => d.ajoute(Prim::Cap { a: pt(loc.0, loc.1), b: pt(loc.0, loc.1), r: 0.3 }, true, objet),
        }
        let corps = boitiers.get(&boitier).and_then(|f| {
            let mut bb = BBox::vide();
            for (p, _) in f {
                bb.union(&p.boite_englobante());
            }
            (!bb.est_vide()).then(|| bb.tableau())
        });
        let hauteur = c.attribute("height").and_then(|v| v.parse::<f64>().ok()).map(|h| h * k).or_else(|| hauteurs_boitiers.get(&boitier).copied()).filter(|h| *h > 0.0);
        composants.push(Composant {
            corps,
            hauteur,
            reference,
            boitier,
            piece: c.attribute("part").unwrap_or("").to_string(),
            valeur,
            x: loc.0,
            y: loc.1,
            // Odb::Composant garde la rotation dans le sens horaire (convention ODB++) : lib.rs la retourne.
            rotation: (360.0 - rotation).rem_euclid(360.0),
            dessous,
            broches: 0,
            proprietes,
        });
    }
    for (d, dessous) in [(dessin_dessus, false), (dessin_dessous, true)] {
        if d.elements.is_empty() {
            continue;
        }
        let mut role = couches::depuis_x2(&["Component".into(), "L1".into(), if dessous { "Bot".into() } else { "Top".into() }]).unwrap();
        role.source = "IPC-2581";
        let n = composants.iter().filter(|c| c.dessous == dessous).count();
        couches.push(CoucheOdb { nom: if dessous { "comp_bot".into() } else { "comp_top".into() }, type_odb: "COMPONENT".into(), contexte: "BOARD".into(), polarite_negative: false, debut: String::new(), fin: String::new(), role, elements: n, dessin: d, outils: Vec::new() });
    }

    // Nets : réseaux logiques (broches), sinon noms portés par les éléments
    let mut nets: Vec<(String, usize)> = enfants(etape, "LogicalNet").map(|n| (n.attribute("name").unwrap_or("").to_string(), enfants(n, "PinRef").count())).collect();
    if nets.is_empty() {
        nets = nets_set.into_iter().collect();
    }
    nets.retain(|(n, _)| !n.is_empty());
    nets.sort();
    nets.dedup_by(|a, b| a.0 == b.0);

    if l.textes_ignores > 0 {
        l.alerte(format!("{} texte(s) IPC-2581 (élément Text) non dessiné(s).", l.textes_ignores));
    }
    let mut epaisseur: Option<f64> = None;
    // Empilage (Stackup) : épaisseurs, dans l'ordre
    if let Some(st) = enfant(ecad, "CadData").and_then(|cd| enfant(cd, "Stackup")) {
        let mut couches_st: Vec<(i32, String, f64)> = Vec::new();
        for g in enfants(st, "StackupGroup") {
            for s in enfants(g, "StackupLayer") {
                couches_st.push((s.attribute("sequence").and_then(|v| v.parse().ok()).unwrap_or(0), s.attribute("layerOrGroupRef").unwrap_or("").to_string(), f(&s, "thickness") * k));
            }
        }
        couches_st.sort_by_key(|c| c.0);
        let total: f64 = couches_st.iter().map(|c| c.2).sum();
        epaisseur = st.attribute("overallThickness").and_then(|v| v.parse::<f64>().ok()).map(|t| t * k).or((total > 0.0).then_some(total)).filter(|e| *e > 0.05 && *e < 20.0);
        if total > 0.0 {
            info.push(("Épaisseur de l'empilage".into(), format!("{} mm", nombre(total))));
        }
        if let Some(t) = st.attribute("overallThickness").and_then(|v| v.parse::<f64>().ok()) {
            info.push(("Épaisseur totale".into(), format!("{} mm", nombre(t * k))));
        }
    }

    Ok(Odb {
        info,
        etape: etape.attribute("name").unwrap_or("").to_string(),
        etapes: etapes.iter().filter_map(|s| s.attribute("name")).map(|s| s.to_string()).collect(),
        couches,
        composants,
        nets,
        alertes: l.alertes,
        unite: if k == 25.4 { "INCH".into() } else { "MM".into() },
        format: "IPC-2581",
        epaisseur,
    })
}

fn est_cuivre(fct: &str) -> bool {
    matches!(fct, "CONDUCTOR" | "SIGNAL" | "PLANE" | "MIXED" | "CONDFILM" | "CONDFOIL" | "POWER_GROUND")
}

fn role_ipc(fct: &str, cote: &str, numero: Option<u32>, nb_cuivres: usize, nom: &str) -> Role {
    let c = match cote {
        "TOP" => Cote::Dessus,
        "BOTTOM" => Cote::Dessous,
        "INTERNAL" => Cote::Interne,
        _ => Cote::Aucun,
    };
    let x2 = |v: &[&str]| couches::depuis_x2(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let face = if c == Cote::Dessous { "Bot" } else { "Top" };
    let r = if est_cuivre(fct) {
        let n = numero.unwrap_or(1);
        let face = if n == 1 { "Top" } else if n as usize == nb_cuivres { "Bot" } else { "Inr" };
        x2(&["Copper", &format!("L{n}"), face])
    } else {
        match fct {
            "SOLDERMASK" => x2(&["Soldermask", face]),
            "SOLDERPASTE" | "PASTEMASK" => x2(&["Paste", face]),
            "SILKSCREEN" | "LEGEND" => x2(&["Legend", face]),
            "DRILL" => x2(&["Plated", "1", "2", "PTH"]),
            "ROUT" | "V_CUT" | "SCORE" => x2(&["Plated", "1", "2", "PTH", "Rout"]),
            "BOARD_OUTLINE" | "BOARDOUTLINE" | "PROFILE" => x2(&["Profile", "NP"]),
            "COMPONENT_TOP" | "COMPONENT_BOTTOM" => x2(&["Component", "L1", if fct == "COMPONENT_BOTTOM" || c == Cote::Dessous { "Bot" } else { "Top" }]),
            // Plan d'assemblage (F.Fab de KiCad), courtyard, colle : documents, masqués par défaut.
            "ASSEMBLY" => x2(&["AssemblyDrawing", face]),
            "COURTYARD" | "GLUE" | "DOCUMENT" => None,
            _ => None,
        }
    };
    let mut r = r.unwrap_or_else(|| {
        let mut r = couches::depuis_nom(nom);
        r.genre = Genre::Autre;
        r.libelle = nom.to_string();
        r.couleur = couches::couleur(Genre::Autre, Cote::Aucun, None);
        r.rang = 280;
        r
    });
    if r.genre == Genre::Fraisage {
        r.libelle = format!("Fraisage « {nom} »");
    }
    if r.genre == Genre::Plan {
        r.libelle = format!("Plan d'assemblage « {nom} »");
    }
    if r.genre == Genre::Contour && fct != "PROFILE" {
        r.libelle = format!("Contour « {nom} »");
    }
    r.source = "IPC-2581";
    r
}

fn traduire_mode(m: &str) -> String {
    match m {
        "FABRICATION" => "fabrication".into(),
        "ASSEMBLY" => "assemblage".into(),
        "TEST" => "test".into(),
        "STACKUP" => "empilage".into(),
        "DESIGN" => "conception".into(),
        "USERDEF" => "défini par l'utilisateur".into(),
        "FULL" => "complet".into(),
        autre => autre.to_lowercase(),
    }
}

fn traduire_metallisation(p: &str) -> String {
    match p {
        "PLATED" => "métallisé".into(),
        "NONPLATED" => "non métallisé".into(),
        "VIA" => "via".into(),
        autre => autre.to_lowercase(),
    }
}

/// Outils de perçage d'une couche : un par diamètre et par métallisation.
fn outils_de(trous: Option<&Vec<(f64, String)>>) -> Vec<OutilOdb> {
    let mut r: Vec<OutilOdb> = Vec::new();
    for (d, p) in trous.into_iter().flatten() {
        let genre = traduire_metallisation(p);
        match r.iter_mut().find(|o| (o.percage - d).abs() < 1e-6 && o.genre == genre) {
            Some(o) => o.trous += 1,
            None => r.push(OutilOdb { numero: String::new(), genre, fini: *d, percage: *d, trous: 1 }),
        }
    }
    r.sort_by(|a, b| a.percage.total_cmp(&b.percage));
    for (i, o) in r.iter_mut().enumerate() {
        o.numero = format!("{}", i + 1);
    }
    r
}
