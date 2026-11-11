//! Vérifications de fabricabilité (DFM) d'un jeu de couches : chaque couche est d'abord aplatie en îlots (union des
//! éléments sombres, moins les éléments clairs, dans l'ordre du fichier), puis les règles sont mesurées sur ces îlots,
//! sur les pistes et sur les trous. Les distances entre bords sont cherchées avec une grille, segment contre segment.
//!
//! Règles : largeur de piste, isolement entre îlots de cuivre, courts-circuits possibles (un îlot qui porte deux nets),
//! anneau annulaire, pastille absente, perçage minimum, écart entre trous, cuivre au bord de la carte, cuivre près
//! d'un trou non métallisé, pont de vernis, sérigraphie sur une ouverture du vernis, sérigraphie hors de la carte,
//! largeur des traits de sérigraphie.

use crate::Couche;
use crate::couches::{Cote, Genre};
use crate::geom::{self, BBox, Dessin, Prim, Pt, pt};
use serde_json::{Value, json};
use std::collections::HashMap;

type Forme = Vec<Vec<[f64; 2]>>;

/// Seuils, en mm.
pub struct Regles {
    pub piste: f64,
    pub isolement: f64,
    pub anneau: f64,
    pub percage: f64,
    pub trou_trou: f64,
    pub cuivre_bord: f64,
    pub cuivre_npth: f64,
    pub pont_vernis: f64,
    pub serigraphie: f64,
}

impl Regles {
    pub fn depuis(v: &Value) -> Regles {
        let f = |k: &str, d: f64| v.get(k).and_then(|x| x.as_f64()).filter(|x| *x >= 0.0).unwrap_or(d);
        Regles {
            piste: f("piste", 0.127),
            isolement: f("isolement", 0.127),
            anneau: f("anneau", 0.13),
            percage: f("percage", 0.3),
            trou_trou: f("trouTrou", 0.25),
            cuivre_bord: f("cuivreBord", 0.3),
            cuivre_npth: f("cuivreNpth", 0.2),
            pont_vernis: f("pontVernis", 0.1),
            serigraphie: f("serigraphie", 0.12),
        }
    }
}

struct Resultat {
    regle: &'static str,
    couche: String,
    x: f64,
    y: f64,
    valeur: f64,
    limite: f64,
    details: String,
}

/// Îlot : anneaux (le premier est le bord extérieur) et boîte englobante.
pub struct Ilot {
    pub anneaux: Vec<Vec<Pt>>,
    pub boite: BBox,
}

impl Ilot {
    fn contient(&self, p: Pt) -> bool {
        if !self.boite.contient(p, 0.0) {
            return false;
        }
        let mut dedans = false;
        for a in &self.anneaux {
            if geom::point_dans_anneau(p, a) {
                dedans = !dedans;
            }
        }
        dedans
    }
    /// Distance du point au bord de l'îlot.
    fn distance_bord(&self, p: Pt) -> f64 {
        let mut d = f64::INFINITY;
        for a in &self.anneaux {
            for i in 0..a.len() {
                d = d.min(geom::dist_segment(p, a[i], a[(i + 1) % a.len()]));
            }
        }
        d
    }
    fn aire(&self) -> f64 {
        self.anneaux.iter().map(|a| geom::aire(a)).sum::<f64>().abs()
    }
}

fn orienter(mut forme: Forme) -> Forme {
    for (i, anneau) in forme.iter_mut().enumerate() {
        let pts: Vec<Pt> = anneau.iter().map(|q| pt(q[0], q[1])).collect();
        let a = geom::aire(&pts);
        // Bord extérieur dans le sens trigonométrique, trous dans le sens horaire (règle non nulle).
        if (i == 0 && a < 0.0) || (i > 0 && a > 0.0) {
            anneau.reverse();
        }
    }
    forme
}

/// Formes (polygones avec trous) d'une primitive, orientées.
fn formes_prim(p: &Prim) -> Vec<Forme> {
    match p {
        Prim::Poly { anneaux } => {
            // Rempli pair-impair, même avec un seul contour : les plans de masse exportés en un contour unique ont leurs
            // dégagements reliés au bord par des coupures, qu'il faut résoudre.
            if anneaux.len() == 1 && anneaux[0].len() <= 8 {
                return vec![orienter(vec![anneaux[0].iter().map(|q| [q.x, q.y]).collect()])];
            }
            use i_overlay::core::fill_rule::FillRule;
            use i_overlay::core::overlay_rule::OverlayRule;
            use i_overlay::float::single::SingleFloatOverlay;
            let f: Vec<Forme> = vec![anneaux.iter().map(|a| a.iter().map(|q| [q.x, q.y]).collect()).collect()];
            f.overlay(&Vec::<Forme>::new(), OverlayRule::Subject, FillRule::EvenOdd).into_iter().map(orienter).collect()
        }
        autre => {
            let c = geom::contours(autre);
            if c.is_empty() { vec![] } else { vec![orienter(c)] }
        }
    }
}

fn en_ilots(formes: Vec<Forme>) -> Vec<Ilot> {
    // Les contours sont regroupés d'après leur sens (bords extérieurs dans le sens trigonométrique, trous dans le sens
    // horaire) : une forme rendue par les opérations booléennes peut contenir des îlots imbriqués dans ses trous.
    let mut exterieurs: Vec<(Vec<Pt>, f64, BBox)> = Vec::new();
    let mut trous: Vec<Vec<Pt>> = Vec::new();
    for f in formes {
        for a in f {
            if a.len() < 3 {
                continue;
            }
            let pts: Vec<Pt> = a.into_iter().map(|q| pt(q[0], q[1])).collect();
            let aire = geom::aire(&pts);
            if aire > 0.0 {
                let mut boite = BBox::vide();
                for p in &pts {
                    boite.ajoute(*p);
                }
                exterieurs.push((pts, aire, boite));
            } else if aire < 0.0 {
                trous.push(pts);
            }
        }
    }
    let mut ilots: Vec<Ilot> = exterieurs.iter().map(|(a, _, b)| Ilot { anneaux: vec![a.clone()], boite: *b }).collect();
    let reperage = Reperage::nouveau(&ilots);
    for t in trous {
        // Le plus petit bord extérieur qui contient le trou.
        let p = t[0];
        let mut choix: Option<usize> = None;
        for i in reperage.candidats(p) {
            if ilots[i].boite.contient(p, 1e-9) && geom::point_dans_anneau(pt((t[0].x + t[1].x) / 2.0, (t[0].y + t[1].y) / 2.0), &ilots[i].anneaux[0]) {
                if choix.map(|c| exterieurs[i].1 < exterieurs[c].1).unwrap_or(true) {
                    choix = Some(i);
                }
            }
        }
        if let Some(i) = choix {
            ilots[i].anneaux.push(t);
        }
    }
    ilots
}

/// Union des éléments sombres moins les éléments clairs, dans l'ordre : les formes résultantes, orientées.
pub fn aplatir_formes(d: &Dessin) -> Vec<Forme> {
    use i_overlay::core::fill_rule::FillRule;
    use i_overlay::core::overlay_rule::OverlayRule;
    use i_overlay::float::single::SingleFloatOverlay;
    let mut acc: Vec<Forme> = Vec::new();
    let mut groupe: Vec<Forme> = Vec::new();
    let mut sombre = true;
    let vider = |acc: &mut Vec<Forme>, groupe: &mut Vec<Forme>, sombre: bool| {
        if groupe.is_empty() {
            return;
        }
        let g = std::mem::take(groupe);
        if sombre {
            *acc = if acc.is_empty() { g.overlay(&Vec::<Forme>::new(), OverlayRule::Subject, FillRule::NonZero) } else { acc.overlay(&g, OverlayRule::Union, FillRule::NonZero) };
        } else if !acc.is_empty() {
            *acc = acc.overlay(&g, OverlayRule::Difference, FillRule::NonZero);
        }
    };
    for e in &d.elements {
        if e.sombre != sombre {
            vider(&mut acc, &mut groupe, sombre);
            sombre = e.sombre;
        }
        // Les traits de largeur nulle (contours dessinés en trait fin) ne sont pas du cuivre : un cadre fermé de
        // largeur infime serait rempli par l'arrondi des opérations booléennes.
        if matches!(e.prim, Prim::Cap { r, .. } if r < 1e-4) {
            continue;
        }
        groupe.extend(formes_prim(&e.prim));
    }
    vider(&mut acc, &mut groupe, sombre);
    acc.into_iter().map(orienter).collect()
}

pub fn aplatir(d: &Dessin) -> Vec<Ilot> {
    en_ilots(aplatir_formes(d))
}

/// Distance entre deux segments, et le milieu des points les plus proches.
fn dist_segments(a: Pt, b: Pt, c: Pt, d: Pt) -> (f64, Pt) {
    let croise = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2, d3, d4) = (croise(c, d, a), croise(c, d, b), croise(a, b, c), croise(a, b, d));
    if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) && d1 != 0.0 && d2 != 0.0 && d3 != 0.0 && d4 != 0.0 {
        let t = d1 / (d1 - d2);
        return (0.0, pt(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
    }
    let proj = |p: Pt, s: Pt, e: Pt| {
        let (vx, vy) = (e.x - s.x, e.y - s.y);
        let l2 = vx * vx + vy * vy;
        let t = if l2 > 0.0 { (((p.x - s.x) * vx + (p.y - s.y) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
        pt(s.x + t * vx, s.y + t * vy)
    };
    let mut meilleur = (f64::INFINITY, a);
    for (p, s, e) in [(a, c, d), (b, c, d), (c, a, b), (d, a, b)] {
        let q = proj(p, s, e);
        let dd = p.dist(q);
        if dd < meilleur.0 {
            meilleur = (dd, pt((p.x + q.x) / 2.0, (p.y + q.y) / 2.0));
        }
    }
    meilleur
}

/// Paires de propriétaires dont des segments sont à moins de `seuil` : distance minimale et lieu. `garder(i, j)`
/// filtre les paires (i < j).
/// Paires de propriétaires dont des segments sont à moins de `seuil` : pour chaque paire, les endroits distincts
/// (à plus de 1,5 mm les uns des autres, 20 au plus), avec la distance minimale à chacun.
fn paires_proches(segments: &[(Pt, Pt, usize)], seuil: f64, garder: &dyn Fn(usize, usize) -> bool) -> HashMap<(usize, usize), Vec<(f64, Pt)>> {
    let mut res: HashMap<(usize, usize), Vec<(f64, Pt)>> = HashMap::new();
    if segments.is_empty() || seuil <= 0.0 {
        return res;
    }
    let cote = (seuil * 2.0).max(0.25);
    // Les segments longs (bords de plans, pistes en diagonale) sont découpés en morceaux de quelques cases : sinon leur
    // boîte couvrirait des milliers de cases.
    let pas = cote * 4.0;
    let mut morceaux: Vec<(Pt, Pt, usize)> = Vec::with_capacity(segments.len());
    for &(a, b, o) in segments {
        let n = (a.dist(b) / pas).ceil().clamp(1.0, 100_000.0) as usize;
        for k in 0..n {
            let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
            morceaux.push((pt(a.x + (b.x - a.x) * t0, a.y + (b.y - a.y) * t0), pt(a.x + (b.x - a.x) * t1, a.y + (b.y - a.y) * t1), o));
        }
    }
    let segments = &morceaux;
    let cle = |x: f64| (x / cote).floor() as i64;
    let mut grille: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, b, _)) in segments.iter().enumerate() {
        let (x0, x1) = (a.x.min(b.x) - seuil, a.x.max(b.x) + seuil);
        let (y0, y1) = (a.y.min(b.y) - seuil, a.y.max(b.y) + seuil);
        let (cx0, cx1, cy0, cy1) = (cle(x0), cle(x1), cle(y0), cle(y1));
        for cx in cx0..=cx1 {
            for cy in cy0..=cy1 {
                grille.entry((cx, cy)).or_default().push(i);
            }
        }
    }
    for liste in grille.values() {
        for (k, &i) in liste.iter().enumerate() {
            for &j in &liste[k + 1..] {
                let (a, b, oi) = segments[i];
                let (c, d, oj) = segments[j];
                if oi == oj {
                    continue;
                }
                let cle_paire = if oi < oj { (oi, oj) } else { (oj, oi) };
                if !garder(cle_paire.0, cle_paire.1) {
                    continue;
                }
                // Boîtes trop éloignées : inutile de calculer.
                if a.x.min(b.x) > c.x.max(d.x) + seuil || c.x.min(d.x) > a.x.max(b.x) + seuil || a.y.min(b.y) > c.y.max(d.y) + seuil || c.y.min(d.y) > a.y.max(b.y) + seuil {
                    continue;
                }
                let (dd, lieu) = dist_segments(a, b, c, d);
                if dd < seuil {
                    let lieux = res.entry(cle_paire).or_default();
                    match lieux.iter().position(|(_, q)| q.dist(lieu) < 1.5) {
                        Some(k) => {
                            if dd < lieux[k].0 {
                                lieux[k] = (dd, lieu);
                            }
                        }
                        None if lieux.len() < 20 => lieux.push((dd, lieu)),
                        None => {}
                    }
                }
            }
        }
    }
    res
}

fn segments_ilots(ilots: &[Ilot], decalage: usize, sortie: &mut Vec<(Pt, Pt, usize)>) {
    for (i, il) in ilots.iter().enumerate() {
        for a in &il.anneaux {
            for k in 0..a.len() {
                sortie.push((a[k], a[(k + 1) % a.len()], decalage + i));
            }
        }
    }
}

/// Grille des îlots par boîte, pour retrouver ceux qui contiennent un point.
struct Reperage {
    cote: f64,
    grille: HashMap<(i64, i64), Vec<usize>>,
}

impl Reperage {
    fn nouveau(ilots: &[Ilot]) -> Reperage {
        let cote = 2.0;
        let mut grille: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, il) in ilots.iter().enumerate() {
            let b = il.boite;
            let (cx0, cx1, cy0, cy1) = ((b.x0 / cote).floor() as i64, (b.x1 / cote).floor() as i64, (b.y0 / cote).floor() as i64, (b.y1 / cote).floor() as i64);
            if (cx1 - cx0 + 1) * (cy1 - cy0 + 1) > 20000 {
                // Très grand îlot (plan de masse) : rangé dans une case spéciale, toujours examinée.
                grille.entry((i64::MIN, i64::MIN)).or_default().push(i);
                continue;
            }
            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    grille.entry((cx, cy)).or_default().push(i);
                }
            }
        }
        Reperage { cote, grille }
    }
    fn candidats(&self, p: Pt) -> Vec<usize> {
        let mut v = self.grille.get(&((p.x / self.cote).floor() as i64, (p.y / self.cote).floor() as i64)).cloned().unwrap_or_default();
        if let Some(g) = self.grille.get(&(i64::MIN, i64::MIN)) {
            v.extend(g);
        }
        v
    }
    fn contenant(&self, ilots: &[Ilot], p: Pt) -> Option<usize> {
        self.candidats(p).into_iter().find(|&i| ilots[i].contient(p))
    }
}

fn point_representatif(p: &Prim) -> Pt {
    match p {
        Prim::Cap { a, b, .. } => pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        Prim::Boite { c, .. } => *c,
        Prim::Poly { anneaux } => {
            // Juste à l'intérieur du bord extérieur : milieu de l'arête la plus longue, décalé vers l'intérieur.
            let a = &anneaux[0];
            if a.len() < 3 {
                return a.first().copied().unwrap_or(pt(0.0, 0.0));
            }
            let (mut k, mut lmax) = (0, 0.0);
            for i in 0..a.len() {
                let l = a[i].dist(a[(i + 1) % a.len()]);
                if l > lmax {
                    (k, lmax) = (i, l);
                }
            }
            let (p, q) = (a[k], a[(k + 1) % a.len()]);
            let m = pt((p.x + q.x) / 2.0, (p.y + q.y) / 2.0);
            let (nx, ny) = (-(q.y - p.y) / lmax, (q.x - p.x) / lmax);
            let e = (lmax * 0.01).clamp(1e-4, 5e-3);
            let c = pt(m.x + nx * e, m.y + ny * e);
            if geom::point_dans_anneau(c, a) { c } else { pt(m.x - nx * e, m.y - ny * e) }
        }
    }
}

fn net_de(d: &Dessin, objet: u32) -> Option<&str> {
    d.info_objet(objet)?.iter().find(|(k, _)| k == "Net").map(|(_, v)| v.as_str()).filter(|v| !v.is_empty() && !matches!(*v, "$NONE$" | "N/C" | "NC" | "No Net" | "<No Net>"))
}

/// Pastilles d'une couche de cuivre (disques, rectangles, petits polygones), rangées dans une grille.
struct Pastilles<'a> {
    prims: Vec<&'a Prim>,
    grille: HashMap<(i64, i64), Vec<usize>>,
}

impl<'a> Pastilles<'a> {
    fn nouvelles(d: &'a Dessin) -> Pastilles<'a> {
        let mut prims = Vec::new();
        let mut grille: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for e in d.elements.iter().filter(|e| e.sombre) {
            let b = e.prim.boite_englobante();
            let pastille = match &e.prim {
                Prim::Cap { a, b: q, r } => a.dist(*q) < 1e-9 && *r > 0.0,
                Prim::Boite { .. } => true,
                Prim::Poly { anneaux } => anneaux.len() == 1 && b.x1 - b.x0 < 8.0 && b.y1 - b.y0 < 8.0,
            };
            if !pastille {
                continue;
            }
            let i = prims.len();
            prims.push(&e.prim);
            let c = pt((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
            grille.entry(((c.x / 2.0).floor() as i64, (c.y / 2.0).floor() as i64)).or_default().push(i);
        }
        Pastilles { prims, grille }
    }

    /// Distance du centre du trou au bord de la plus grande pastille qui le contient.
    fn anneau(&self, p: Pt) -> Option<f64> {
        let (cx, cy) = ((p.x / 2.0).floor() as i64, (p.y / 2.0).floor() as i64);
        let mut meilleur: Option<f64> = None;
        for dx in -2..=2 {
            for dy in -2..=2 {
                for &i in self.grille.get(&(cx + dx, cy + dy)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let d = match self.prims[i] {
                        Prim::Cap { a, r, .. } => r - a.dist(p),
                        Prim::Boite { c, hw, hh, ang, rad } => {
                            let (s, co) = ang.sin_cos();
                            let (dx, dy) = (p.x - c.x, p.y - c.y);
                            let (lx, ly) = ((dx * co + dy * s).abs(), (-dx * s + dy * co).abs());
                            let (qx, qy) = (lx - (hw - rad), ly - (hh - rad));
                            -((qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad)
                        }
                        Prim::Poly { anneaux } => {
                            if geom::point_dans_anneau(p, &anneaux[0]) {
                                let a = &anneaux[0];
                                (0..a.len()).map(|k| geom::dist_segment(p, a[k], a[(k + 1) % a.len()])).fold(f64::INFINITY, f64::min)
                            } else {
                                -1.0
                            }
                        }
                    };
                    if d > 0.0 && meilleur.map(|m| d > m).unwrap_or(true) {
                        meilleur = Some(d);
                    }
                }
            }
        }
        meilleur
    }
}

struct Trou {
    c: Pt,
    r: f64,
    metallise: bool,
    /// Le perçage atteint-il la face du dessus, celle du dessous (vias borgnes et enterrés) ?
    dessus: bool,
    dessous: bool,
}

/// Couches de cuivre reliées par un fichier de perçage, numérotées du dessus (1) au dessous (n) : d'après les couches
/// de début et de fin (ODB++), ou « 1–4 » du libellé X2. None : traversant (ou inconnu).
pub fn etendue_percage(c: &Couche, couches: &[Couche]) -> Option<(u32, u32)> {
    let mut cuivres: Vec<&Couche> = couches.iter().filter(|k| k.role.genre == Genre::Cuivre).collect();
    cuivres.sort_by(|a, b| b.role.rang.cmp(&a.role.rang));
    let n = cuivres.len() as u32;
    let debut = c.meta.get("debut").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
    let fin = c.meta.get("fin").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
    if !debut.is_empty() {
        let rang = |nom: &str| cuivres.iter().position(|k| k.meta.get("nomOdb").and_then(|v| v.as_str()).map(|s| s.eq_ignore_ascii_case(nom)).unwrap_or(false)).map(|i| i as u32 + 1);
        let (a, b) = (rang(&debut)?, rang(&fin)?);
        return Some((a.min(b), a.max(b)));
    }
    let nombres: Vec<u32> = c.role.libelle.split(|ch: char| !ch.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect();
    if nombres.len() >= 2 && n > 0 {
        let (a, b) = (nombres[0].min(nombres[1]).max(1), nombres[0].max(nombres[1]).min(n));
        return Some((a, b));
    }
    None
}

/// Un trou de ce diamètre est-il métallisé ? D'après l'outil de même diamètre (ODB++, IPC-2581), sinon `defaut`
/// (la couche entière).
pub fn metallise_trou(c: &Couche, diametre: f64, defaut: bool) -> bool {
    c.meta.get("metallisationOutils").and_then(|o| o.as_array()).and_then(|outils| {
        outils.iter().filter_map(|t| Some((t.get(0)?.as_f64()?, t.get(1)?.as_bool()?))).find(|(d, _)| (d - diametre).abs() < 0.003).map(|(_, m)| m)
    }).unwrap_or(defaut)
}

/// Métallisation d'une couche de perçage, d'après son libellé (sans information par outil).
pub fn couche_metallisee(c: &Couche) -> bool {
    let libelle = format!("{} {}", c.role.libelle, c.nom).to_lowercase();
    !(libelle.contains("non métallisé") || libelle.contains("npth") || libelle.contains("non-plated") || libelle.contains("non plated"))
}

/// Faces atteintes par un fichier de perçage (vias borgnes et enterrés : pas toutes).
fn faces_atteintes(c: &Couche, couches: &[Couche]) -> (bool, bool) {
    let n = couches.iter().filter(|k| k.role.genre == Genre::Cuivre).count() as u32;
    match etendue_percage(c, couches) {
        Some((a, b)) => (a <= 1, n == 0 || b >= n),
        None => (true, true),
    }
}

pub fn verifier(couches: &[Couche], regles: &Regles) -> Value {
    let mut res: Vec<Resultat> = Vec::new();
    let mut verifiees: Vec<&'static str> = Vec::new();
    let mut note = |r: &'static str| {
        if !verifiees.contains(&r) {
            verifiees.push(r);
        }
    };

    // Contour de la carte
    let mut bord: Vec<Vec<Pt>> = Vec::new();
    for c in couches.iter().filter(|c| c.role.genre == Genre::Contour) {
        bord = geom::anneaux_contour(&c.dessin);
        if !bord.is_empty() {
            break;
        }
    }
    bord.sort_by(|a, b| geom::aire(b).abs().total_cmp(&geom::aire(a).abs()));
    let forme_carte: Vec<Forme> = if bord.is_empty() {
        vec![]
    } else {
        use i_overlay::core::fill_rule::FillRule;
        use i_overlay::core::overlay_rule::OverlayRule;
        use i_overlay::float::single::SingleFloatOverlay;
        let f: Vec<Forme> = vec![bord.iter().map(|a| a.iter().map(|q| [q.x, q.y]).collect()).collect()];
        f.overlay(&Vec::<Forme>::new(), OverlayRule::Subject, FillRule::EvenOdd)
    };

    // Trous
    let mut trous: Vec<Trou> = Vec::new();
    for c in couches.iter().filter(|c| c.role.genre == Genre::Percage || c.role.genre == Genre::Fraisage) {
        let metallise = couche_metallisee(c);
        let (dessus, dessous) = faces_atteintes(c, couches);
        for e in c.dessin.elements.iter().filter(|e| e.sombre) {
            if let Prim::Cap { a, b, r } = e.prim {
                if a.dist(b) < 1e-6 && r > 0.0005 {
                    trous.push(Trou { c: a, r, metallise: metallise_trou(c, 2.0 * r, metallise), dessus, dessous });
                }
            }
        }
    }
    if !trous.is_empty() {
        note("percage");
        note("trouTrou");
        for t in &trous {
            if 2.0 * t.r < regles.percage - 1e-6 {
                res.push(Resultat { regle: "percage", couche: String::new(), x: t.c.x, y: t.c.y, valeur: 2.0 * t.r, limite: regles.percage, details: if t.metallise { "métallisé".into() } else { "non métallisé".into() } });
            }
        }
        // Écart entre trous (bord à bord) : voisinage assez large pour les plus gros trous.
        let cote = 2.0;
        let rmax = trous.iter().map(|t| t.r).fold(0.0, f64::max);
        let portee = ((regles.trou_trou + 2.0 * rmax) / cote).ceil().clamp(1.0, 50.0) as i64;
        let mut grille: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, t) in trous.iter().enumerate() {
            grille.entry(((t.c.x / cote).floor() as i64, (t.c.y / cote).floor() as i64)).or_default().push(i);
        }
        for (i, t) in trous.iter().enumerate() {
            let (cx, cy) = ((t.c.x / cote).floor() as i64, (t.c.y / cote).floor() as i64);
            for dx in -portee..=portee {
                for dy in -portee..=portee {
                    for &j in grille.get(&(cx + dx, cy + dy)).map(|v| v.as_slice()).unwrap_or(&[]) {
                        if j <= i {
                            continue;
                        }
                        let u = &trous[j];
                        let dc = t.c.dist(u.c);
                        if dc < 0.01 {
                            continue; // même trou, dans deux fichiers
                        }
                        let ecart = dc - t.r - u.r;
                        if ecart < regles.trou_trou {
                            res.push(Resultat { regle: "trouTrou", couche: String::new(), x: (t.c.x + u.c.x) / 2.0, y: (t.c.y + u.c.y) / 2.0, valeur: ecart.max(0.0), limite: regles.trou_trou, details: String::new() });
                        }
                    }
                }
            }
        }
    }

    // Cuivre
    let mut ilots_externes: Vec<(Cote, Vec<Ilot>)> = Vec::new();
    for c in couches.iter().filter(|c| c.role.genre == Genre::Cuivre && !c.dessin.elements.is_empty()) {
        note("piste");
        note("isolement");
        note("court");
        // Largeur de piste, une fois par objet (au plus étroit).
        let mut par_objet: HashMap<u32, (f64, Pt)> = HashMap::new();
        for e in c.dessin.elements.iter().filter(|e| e.sombre) {
            if let Prim::Cap { a, b, r } = e.prim {
                if a.dist(b) > 1e-6 && r > 0.0 && 2.0 * r < regles.piste - 1e-6 {
                    let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                    let v = par_objet.entry(e.objet).or_insert((2.0 * r, m));
                    if 2.0 * r < v.0 {
                        *v = (2.0 * r, m);
                    }
                }
            }
        }
        for (_, (l, m)) in par_objet {
            res.push(Resultat { regle: "piste", couche: c.nom.clone(), x: m.x, y: m.y, valeur: l, limite: regles.piste, details: String::new() });
        }
        let ilots = aplatir(&c.dessin);
        let reperage = Reperage::nouveau(&ilots);
        // Nets portés par chaque îlot
        let mut nets: Vec<Vec<String>> = vec![Vec::new(); ilots.len()];
        // Lieu de chaque net dans l'îlot (un élément de ce net), pour placer la marque d'un court-circuit.
        let mut lieux: Vec<Vec<Pt>> = vec![Vec::new(); ilots.len()];
        for e in c.dessin.elements.iter().filter(|e| e.sombre && !matches!(e.prim, Prim::Cap { r, .. } if r <= 0.0)) {
            if let Some(n) = net_de(&c.dessin, e.objet) {
                if let Some(i) = reperage.contenant(&ilots, point_representatif(&e.prim)) {
                    if !nets[i].iter().any(|x| x == n) {
                        nets[i].push(n.to_string());
                        lieux[i].push(point_representatif(&e.prim));
                    }
                }
            }
        }
        for (i, n) in nets.iter().enumerate() {
            if n.len() > 1 {
                // Sur l'élément du second net le plus proche d'un élément du premier.
                let mut lieu = lieux[i][1];
                let mut dmin = f64::INFINITY;
                for &q in &lieux[i][1..] {
                    let d = q.dist(lieux[i][0]);
                    if d < dmin {
                        dmin = d;
                        lieu = q;
                    }
                }
                res.push(Resultat { regle: "court", couche: c.nom.clone(), x: lieu.x, y: lieu.y, valeur: 0.0, limite: 0.0, details: n.iter().take(6).cloned().collect::<Vec<_>>().join(", ") });
            }
        }
        // Isolement entre îlots (sauf deux îlots du même net)
        let mut segs = Vec::new();
        segments_ilots(&ilots, 0, &mut segs);
        let memes = |i: usize, j: usize| -> bool { !nets[i].is_empty() && nets[i] == nets[j] };
        for ((i, j), lieux) in paires_proches(&segs, regles.isolement, &|i, j| !memes(i, j)) {
            for (d, lieu) in lieux {
            res.push(Resultat { regle: "isolement", couche: c.nom.clone(), x: lieu.x, y: lieu.y, valeur: d, limite: regles.isolement, details: [nets[i].first(), nets[j].first()].iter().flatten().map(|s| s.as_str()).collect::<Vec<_>>().join(" / ") });
            }
        }
        // Cuivre au bord de la carte
        if !bord.is_empty() {
            note("cuivreBord");
            let mut s2 = Vec::new();
            segments_ilots(&ilots, 0, &mut s2);
            let n = ilots.len();
            for a in &bord {
                for k in 0..a.len() {
                    s2.push((a[k], a[(k + 1) % a.len()], n));
                }
            }
            for ((i, _), (d, lieu)) in paires_proches(&s2, regles.cuivre_bord, &|_, j| j == n).into_iter().flat_map(|(k, v)| v.into_iter().map(move |x| (k, x))) {
                // Un îlot qui touche le bord est aussi signalé (distance nulle).
                let _ = i;
                res.push(Resultat { regle: "cuivreBord", couche: c.nom.clone(), x: lieu.x, y: lieu.y, valeur: d, limite: regles.cuivre_bord, details: String::new() });
            }
        }
        // Trous non métallisés trop près du cuivre
        if trous.iter().any(|t| !t.metallise) {
            note("cuivreNpth");
            for t in trous.iter().filter(|t| !t.metallise) {
                let mut pire = f64::INFINITY;
                for i in reperage.candidats(t.c) {
                    let il = &ilots[i];
                    let mut b = il.boite;
                    b.gonfle(t.r + regles.cuivre_npth);
                    if !b.contient(t.c, 0.0) {
                        continue;
                    }
                    let d = if il.contient(t.c) { 0.0 } else { (il.distance_bord(t.c) - t.r).max(0.0) };
                    pire = pire.min(d);
                }
                // Les îlots voisins rangés dans d'autres cases : on regarde autour.
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    let p = pt(t.c.x + dx * (t.r + regles.cuivre_npth), t.c.y + dy * (t.r + regles.cuivre_npth));
                    for i in reperage.candidats(p) {
                        let il = &ilots[i];
                        let d = if il.contient(t.c) { 0.0 } else { (il.distance_bord(t.c) - t.r).max(0.0) };
                        pire = pire.min(d);
                    }
                }
                if pire < regles.cuivre_npth {
                    res.push(Resultat { regle: "cuivreNpth", couche: c.nom.clone(), x: t.c.x, y: t.c.y, valeur: pire, limite: regles.cuivre_npth, details: format!("Ø {:.3} mm", 2.0 * t.r) });
                }
            }
        }
        if c.role.cote == Cote::Dessus || c.role.cote == Cote::Dessous {
            ilots_externes.push((c.role.cote, ilots));
        }
    }

    // Anneau annulaire et pastille absente, sur les couches externes, pour les trous métallisés. L'anneau se mesure sur
    // la pastille posée sur le trou (la plus grande), comme le font les fabricants ; sans pastille, sur le cuivre
    // qui entoure le trou.
    if trous.iter().any(|t| t.metallise) && !ilots_externes.is_empty() {
        note("anneau");
        let noms: HashMap<Cote, String> = couches.iter().filter(|c| c.role.genre == Genre::Cuivre && (c.role.cote == Cote::Dessus || c.role.cote == Cote::Dessous)).map(|c| (c.role.cote, c.nom.clone())).collect();
        for (cote, ilots) in &ilots_externes {
            let reperage = Reperage::nouveau(ilots);
            let nom = noms.get(cote).cloned().unwrap_or_default();
            let pastilles = couches.iter().find(|c| c.role.genre == Genre::Cuivre && c.role.cote == *cote).map(|c| Pastilles::nouvelles(&c.dessin));
            for t in trous.iter().filter(|t| t.metallise && if *cote == Cote::Dessus { t.dessus } else { t.dessous }) {
                if let Some(a) = pastilles.as_ref().and_then(|p| p.anneau(t.c)) {
                    let anneau = a - t.r;
                    if anneau < regles.anneau - 1e-6 {
                        res.push(Resultat { regle: "anneau", couche: nom.clone(), x: t.c.x, y: t.c.y, valeur: anneau.max(0.0), limite: regles.anneau, details: format!("Ø {:.3} mm", 2.0 * t.r) });
                    }
                    continue;
                }
                match reperage.contenant(ilots, t.c) {
                    Some(i) => {
                        let anneau = ilots[i].distance_bord(t.c) - t.r;
                        if anneau < regles.anneau - 1e-6 {
                            res.push(Resultat { regle: "anneau", couche: nom.clone(), x: t.c.x, y: t.c.y, valeur: anneau.max(0.0), limite: regles.anneau, details: format!("Ø {:.3} mm", 2.0 * t.r) });
                        }
                    }
                    None => res.push(Resultat { regle: "pastilleAbsente", couche: nom.clone(), x: t.c.x, y: t.c.y, valeur: 0.0, limite: 0.0, details: format!("Ø {:.3} mm", 2.0 * t.r) }),
                }
            }
        }
    }

    // Vernis et sérigraphie, par côté
    use i_overlay::core::fill_rule::FillRule;
    use i_overlay::core::overlay_rule::OverlayRule;
    use i_overlay::float::single::SingleFloatOverlay;
    for cote in [Cote::Dessus, Cote::Dessous] {
        let masque = couches.iter().find(|c| c.role.genre == Genre::Masque && c.role.cote == cote && !c.dessin.elements.is_empty());
        let serigraphie = couches.iter().find(|c| c.role.genre == Genre::Serigraphie && c.role.cote == cote && !c.dessin.elements.is_empty());
        let ouvertures = masque.map(|m| aplatir_formes(&m.dessin)).unwrap_or_default();
        if let Some(m) = masque {
            note("pontVernis");
            let ilots = en_ilots(ouvertures.clone());
            let mut segs = Vec::new();
            segments_ilots(&ilots, 0, &mut segs);
            for (d, lieu) in paires_proches(&segs, regles.pont_vernis, &|_, _| true).into_values().flatten() {
                res.push(Resultat { regle: "pontVernis", couche: m.nom.clone(), x: lieu.x, y: lieu.y, valeur: d, limite: regles.pont_vernis, details: String::new() });
            }
        }
        if let Some(s) = serigraphie {
            note("serigraphie");
            let mut par_objet: HashMap<u32, (f64, Pt)> = HashMap::new();
            for e in s.dessin.elements.iter().filter(|e| e.sombre) {
                if let Prim::Cap { a, b, r } = e.prim {
                    if a.dist(b) > 1e-6 && r > 0.0 && 2.0 * r < regles.serigraphie - 1e-6 {
                        let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                        let v = par_objet.entry(e.objet).or_insert((2.0 * r, m));
                        if 2.0 * r < v.0 {
                            *v = (2.0 * r, m);
                        }
                    }
                }
            }
            for (_, (l, m)) in par_objet {
                res.push(Resultat { regle: "serigraphie", couche: s.nom.clone(), x: m.x, y: m.y, valeur: l, limite: regles.serigraphie, details: String::new() });
            }
            let formes = aplatir_formes(&s.dessin);
            if !ouvertures.is_empty() {
                note("serigraphiePastille");
                for f in formes.overlay(&ouvertures, OverlayRule::Intersect, FillRule::NonZero) {
                    let il = en_ilots(vec![f]);
                    if let Some(il) = il.first() {
                        if il.aire() > 0.002 {
                            let b = il.boite;
                            res.push(Resultat { regle: "serigraphiePastille", couche: s.nom.clone(), x: (b.x0 + b.x1) / 2.0, y: (b.y0 + b.y1) / 2.0, valeur: il.aire(), limite: 0.0, details: String::new() });
                        }
                    }
                }
            }
            if !forme_carte.is_empty() {
                note("serigraphieHors");
                for f in formes.overlay(&forme_carte, OverlayRule::Difference, FillRule::NonZero) {
                    let il = en_ilots(vec![f]);
                    if let Some(il) = il.first() {
                        if il.aire() > 0.002 {
                            let b = il.boite;
                            res.push(Resultat { regle: "serigraphieHors", couche: s.nom.clone(), x: (b.x0 + b.x1) / 2.0, y: (b.y0 + b.y1) / 2.0, valeur: il.aire(), limite: 0.0, details: String::new() });
                        }
                    }
                }
            }
        }
    }

    // Défauts de surface voisins (les lettres d'un même texte) : regroupés, à moins de 1,5 mm les uns des autres.
    let mut regroupes: Vec<Resultat> = Vec::new();
    for x in res.into_iter() {
        if matches!(x.regle, "serigraphiePastille" | "serigraphieHors") {
            if let Some(g) = regroupes.iter_mut().find(|g| g.regle == x.regle && g.couche == x.couche && (g.x - x.x).hypot(g.y - x.y) < 1.5) {
                g.valeur += x.valeur;
                continue;
            }
        }
        regroupes.push(x);
    }
    let mut res = regroupes;
    // Tri : par règle, le plus grave d'abord (plus petite valeur), et au plus 2000 résultats par règle.
    res.sort_by(|a, b| a.regle.cmp(b.regle).then(a.valeur.partial_cmp(&b.valeur).unwrap_or(std::cmp::Ordering::Equal)));
    let mut comptes: HashMap<&'static str, usize> = HashMap::new();
    let mut liste = Vec::new();
    for r in &res {
        let n = comptes.entry(r.regle).or_default();
        *n += 1;
        if *n <= 2000 {
            liste.push(json!({"regle": r.regle, "couche": r.couche, "x": r.x, "y": r.y, "valeur": r.valeur, "limite": r.limite, "details": r.details}));
        }
    }
    json!({"resultats": liste, "comptes": comptes, "verifiees": verifiees, "contour": !bord.is_empty(), "trous": trous.len()})
}
