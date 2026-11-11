//! Écriture de fichiers de fabrication à partir des couches lues : Gerber X2 (traits, pastilles, régions, polarités)
//! et Excellon (trous et fraisages), réunis dans une archive ZIP. Sert à la mise en panneau : la carte est répétée en
//! lignes et en colonnes, avec des bords de manutention, des mires, des trous d'outillage, et des rainures en V ou des
//! languettes à perforations (« mouse bites »).

use crate::Couche;
use crate::couches::{Cote, Genre};
use crate::geom::{self, BBox, Dessin, Prim, Pt, pt};
use serde_json::{Value, json};
use std::fmt::Write as _;

/* --- Gerber X2 ------------------------------------------------------------------------------------------------- */

fn coord(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

/// Fonction X2 d'une couche (.FileFunction), d'après son rôle.
fn fonction_x2(c: &Couche, cuivres: u32) -> Option<String> {
    let cote = |c: Cote| match c {
        Cote::Dessus => "Top",
        Cote::Dessous => "Bot",
        _ => "Inr",
    };
    let r = &c.role;
    Some(match r.genre {
        Genre::Cuivre => {
            let n = match r.cote {
                Cote::Dessus => 1,
                Cote::Dessous => cuivres.max(2),
                _ => r.numero.unwrap_or(2),
            };
            format!("Copper,L{n},{}", cote(r.cote))
        }
        Genre::Masque => format!("Soldermask,{}", cote(r.cote)),
        Genre::Serigraphie => format!("Legend,{}", cote(r.cote)),
        Genre::Pate => format!("Paste,{}", cote(r.cote)),
        Genre::Contour => "Profile,NP".into(),
        Genre::Composants => format!("Component,L{},{}", if r.cote == Cote::Dessous { cuivres.max(2) } else { 1 }, cote(r.cote)),
        _ => return None,
    })
}

/// Chemin d'une région : bord extérieur, et chaque trou rattaché par une « coupure » (cut-in) sans croiser ni le bord
/// ni les autres trous : les trous sont traités du plus à droite au plus à gauche, chacun relié depuis son sommet le
/// plus à droite au premier bord rencontré vers la droite (méthode d'earcut). La norme Gerber exige des contours
/// qui ne se croisent pas.
fn chemin_region(anneaux: &[Vec<Pt>]) -> Vec<Pt> {
    let Some(ext) = anneaux.first() else { return Vec::new() };
    if ext.len() < 3 {
        return Vec::new();
    }
    let mut bord: Vec<Pt> = ext.clone();
    let mut trous: Vec<&Vec<Pt>> = anneaux[1..].iter().filter(|t| t.len() >= 3).collect();
    let xmax = |t: &Vec<Pt>| t.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    trous.sort_by(|a, b| xmax(b).total_cmp(&xmax(a)));
    for trou in trous {
        // Sommet le plus à droite du trou.
        let (im, m) = trou.iter().copied().enumerate().max_by(|a, b| a.1.x.total_cmp(&b.1.x)).unwrap();
        // Arête du bord coupée par le rayon horizontal vers la droite, la plus proche.
        let mut meilleur: Option<(f64, usize)> = None;
        for k in 0..bord.len() {
            let (a, b) = (bord[k], bord[(k + 1) % bord.len()]);
            if (a.y > m.y) == (b.y > m.y) {
                continue;
            }
            let x = a.x + (m.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if x >= m.x && meilleur.map(|(xm, _)| x < xm).unwrap_or(true) {
                meilleur = Some((x, if a.x > b.x { k } else { (k + 1) % bord.len() }));
            }
        }
        let Some((_, ip)) = meilleur else { continue };
        let mut nouveau: Vec<Pt> = Vec::with_capacity(bord.len() + trou.len() + 2);
        nouveau.extend_from_slice(&bord[..=ip]);
        for k in 0..=trou.len() {
            nouveau.push(trou[(im + k) % trou.len()]);
        }
        nouveau.push(bord[ip]);
        nouveau.extend_from_slice(&bord[ip + 1..]);
        bord = nouveau;
    }
    bord.push(bord[0]);
    bord
}

/// Gerber X2 d'un dessin répété aux décalages donnés, avec des éléments ajoutés (bords du panneau…).
pub fn gerber(dessins: &[(&Dessin, Vec<(f64, f64)>)], fonction: Option<&str>, nom: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "%TF.GenerationSoftware,Typon,Typon,1.0*%");
    if let Some(f) = fonction {
        let _ = writeln!(s, "%TF.FileFunction,{f}*%");
    }
    let _ = writeln!(s, "%TF.Part,Array*%");
    let _ = writeln!(s, "G04 {}*", nom.replace('*', ""));
    let _ = writeln!(s, "%FSLAX46Y46*%");
    let _ = writeln!(s, "%MOMM*%");
    let _ = writeln!(s, "%LPD*%");
    // Ouvertures : un cercle par diamètre de trait.
    let mut diametres: Vec<i64> = Vec::new();
    for (d, _) in dessins {
        for e in &d.elements {
            if let Prim::Cap { r, .. } = e.prim {
                let k = (2.0 * r * 1e6).round() as i64;
                if !diametres.contains(&k) {
                    diametres.push(k);
                }
            }
        }
    }
    for (i, k) in diametres.iter().enumerate() {
        let _ = writeln!(s, "%ADD{}C,{:.6}*%", 10 + i, *k as f64 / 1e6);
    }
    let mut sombre = true;
    let mut ouverture = -1i64;
    for (d, decalages) in dessins {
        for &(dx, dy) in decalages {
            for e in &d.elements {
                if e.sombre != sombre {
                    sombre = e.sombre;
                    let _ = writeln!(s, "%LP{}*%", if sombre { "D" } else { "C" });
                }
                let p = |q: Pt| (coord(q.x + dx), coord(q.y + dy));
                match &e.prim {
                    Prim::Cap { a, b, r } => {
                        let k = (2.0 * r * 1e6).round() as i64;
                        let i = diametres.iter().position(|x| *x == k).unwrap_or(0) as i64 + 10;
                        if i != ouverture {
                            let _ = writeln!(s, "D{i}*");
                            ouverture = i;
                        }
                        let (ax, ay) = p(*a);
                        if a.dist(*b) < 1e-9 {
                            let _ = writeln!(s, "X{ax}Y{ay}D03*");
                        } else {
                            let (bx, by) = p(*b);
                            let _ = writeln!(s, "X{ax}Y{ay}D02*");
                            let _ = writeln!(s, "X{bx}Y{by}D01*");
                        }
                    }
                    autre => {
                        let anneaux: Vec<Vec<Pt>> = match autre {
                            Prim::Poly { anneaux } => anneaux.clone(),
                            _ => geom::contours(autre).into_iter().map(|a| a.into_iter().map(|q| pt(q[0], q[1])).collect()).collect(),
                        };
                        // Un polygone pair-impair à plusieurs bords extérieurs : on l'aplatit d'abord.
                        let formes = if anneaux.len() > 1 { crate::dfm::aplatir_formes(&{
                            let mut d = Dessin::default();
                            let o = d.nouvel_objet(Vec::new());
                            d.ajoute(Prim::Poly { anneaux: anneaux.clone() }, true, o);
                            d
                        }).into_iter().map(|f| f.into_iter().map(|a| a.into_iter().map(|q| pt(q[0], q[1])).collect()).collect()).collect() } else { vec![anneaux] };
                        for f in formes {
                            let chemin = chemin_region(&f);
                            if chemin.len() < 4 {
                                continue;
                            }
                            let _ = writeln!(s, "G36*");
                            let (x0, y0) = p(chemin[0]);
                            let _ = writeln!(s, "X{x0}Y{y0}D02*");
                            let _ = writeln!(s, "G01*");
                            for q in &chemin[1..] {
                                let (x, y) = p(*q);
                                let _ = writeln!(s, "X{x}Y{y}D01*");
                            }
                            let _ = writeln!(s, "G37*");
                        }
                    }
                }
            }
        }
    }
    s.push_str("M02*\n");
    s
}

/* --- Excellon ---------------------------------------------------------------------------------------------------- */

/// Trous (centre, diamètre), rainures (début, fin, diamètre) et chemins de fraisage (points, diamètre).
#[derive(Default)]
pub struct Percages {
    pub trous: Vec<(Pt, f64)>,
    pub fraisages: Vec<(Pt, Pt, f64)>,
    pub chemins: Vec<(Vec<Pt>, f64)>,
}

/// Excellon avec l'attribut X2 de la fonction du fichier : métallisé ou non, de la couche `de` à la couche `a`.
pub fn excellon(p: &Percages, metallise: bool, de: u32, a: u32) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "M48");
    let _ = writeln!(s, "; #@! TF.GenerationSoftware,Typon,Typon,1.0");
    let fraisage = !p.chemins.is_empty() && p.trous.is_empty() && p.fraisages.is_empty();
    let _ = writeln!(s, "; #@! TF.FileFunction,{},{de},{a},{}{}", if metallise { "Plated" } else { "NonPlated" }, if metallise { "PTH" } else { "NPTH" }, if fraisage { ",Rout" } else { "" });
    let _ = writeln!(s, "FMAT,2");
    let _ = writeln!(s, "METRIC");
    let mut outils: Vec<i64> = Vec::new();
    for d in p.trous.iter().map(|t| t.1).chain(p.fraisages.iter().map(|f| f.2)).chain(p.chemins.iter().map(|c| c.1)) {
        let k = (d * 1000.0).round() as i64;
        if !outils.contains(&k) {
            outils.push(k);
        }
    }
    outils.sort();
    for (i, k) in outils.iter().enumerate() {
        let _ = writeln!(s, "T{}C{:.3}", i + 1, *k as f64 / 1000.0);
    }
    let _ = writeln!(s, "%");
    let _ = writeln!(s, "G90");
    let mesure = |d: f64| (d * 1000.0).round() as i64;
    for (i, k) in outils.iter().enumerate() {
        let trous: Vec<_> = p.trous.iter().filter(|t| mesure(t.1) == *k).collect();
        let fraisages: Vec<_> = p.fraisages.iter().filter(|f| mesure(f.2) == *k).collect();
        let chemins: Vec<_> = p.chemins.iter().filter(|c| mesure(c.1) == *k && c.0.len() >= 2).collect();
        if trous.is_empty() && fraisages.is_empty() && chemins.is_empty() {
            continue;
        }
        let _ = writeln!(s, "T{}", i + 1);
        let _ = writeln!(s, "G05");
        for (c, _) in trous {
            let _ = writeln!(s, "X{:.4}Y{:.4}", c.x, c.y);
        }
        for (a, b, _) in fraisages {
            let _ = writeln!(s, "X{:.4}Y{:.4}G85X{:.4}Y{:.4}", a.x, a.y, b.x, b.y);
        }
        // Fraisage : déplacement outil levé (G00), plongée (M15), trajet (G01), remontée (M16).
        for (pts, _) in chemins {
            let _ = writeln!(s, "G00X{:.4}Y{:.4}", pts[0].x, pts[0].y);
            let _ = writeln!(s, "M15");
            for q in &pts[1..] {
                let _ = writeln!(s, "G01X{:.4}Y{:.4}", q.x, q.y);
            }
            let _ = writeln!(s, "M16");
        }
        let _ = writeln!(s, "G05");
    }
    let _ = writeln!(s, "T0");
    let _ = writeln!(s, "M30");
    s
}

/// Enchaîne des segments bout à bout (fraisages lus segment par segment) en chemins continus.
fn chainer(segments: &[(Pt, Pt, f64)]) -> Vec<(Vec<Pt>, f64)> {
    let mut chemins: Vec<(Vec<Pt>, f64)> = Vec::new();
    for &(a, b, d) in segments {
        if let Some(c) = chemins.last_mut() {
            if (c.1 - d).abs() < 1e-6 && c.0.last().map(|q| q.dist(a) < 1e-4).unwrap_or(false) {
                c.0.push(b);
                continue;
            }
        }
        chemins.push((vec![a, b], d));
    }
    chemins
}

/* --- ZIP (sans compression) ------------------------------------------------------------------------------------ */

fn crc32(d: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in d {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

pub fn zip(fichiers: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let u16le = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let u32le = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    for (nom, donnees) in fichiers {
        let crc = crc32(donnees);
        let decalage = out.len() as u32;
        let n = nom.as_bytes();
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 20);
        u16le(&mut out, 0x0800); // noms en UTF-8
        u16le(&mut out, 0); // stocké
        u16le(&mut out, 0);
        u16le(&mut out, 0x21);
        u32le(&mut out, crc);
        u32le(&mut out, donnees.len() as u32);
        u32le(&mut out, donnees.len() as u32);
        u16le(&mut out, n.len() as u16);
        u16le(&mut out, 0);
        out.extend_from_slice(n);
        out.extend_from_slice(donnees);
        u32le(&mut central, 0x0201_4b50);
        u16le(&mut central, 20);
        u16le(&mut central, 20);
        u16le(&mut central, 0x0800);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0x21);
        u32le(&mut central, crc);
        u32le(&mut central, donnees.len() as u32);
        u32le(&mut central, donnees.len() as u32);
        u16le(&mut central, n.len() as u16);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u32le(&mut central, 0);
        u32le(&mut central, decalage);
        central.extend_from_slice(n);
    }
    let debut = out.len() as u32;
    out.extend_from_slice(&central);
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    u16le(&mut out, fichiers.len() as u16);
    u16le(&mut out, fichiers.len() as u16);
    u32le(&mut out, central.len() as u32);
    u32le(&mut out, debut);
    u16le(&mut out, 0);
    out
}

/* --- Panneau ----------------------------------------------------------------------------------------------------- */

fn nom_fichier(c: &Couche, ext: &str) -> String {
    let base = c.fichier.rsplit(['/', '\\']).next().unwrap_or(&c.fichier).to_string();
    let base = if base.is_empty() || c.format != "Gerber" && c.format != "Excellon" {
        format!("{}.{ext}", c.nom.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '«', '»'], "").trim().replace(' ', "_"))
    } else {
        base
    };
    format!("panneau-{base}")
}

/// Forme d'un rectangle (x0, y0, x1, y1), pour les opérations booléennes.
fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> Prim {
    Prim::Poly { anneaux: vec![vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)]] }
}

/// Panneau de `nx` × `ny` cartes. Paramètres (mm) : ecartX, ecartY (écart entre cartes, et entre cartes et bords en
/// fraisage), bord (largeur des bords de manutention), bords (« haut-bas », « gauche-droite », « tous », « aucun »),
/// separation (« vscore » ou « fraisage »), mires et trousOutil (booléens).
///
/// Le contour du panneau (fichier « Profile ») est le bord de la matière réelle : cadre, cartes avec leurs découpes,
/// languettes. En fraisage, les canaux entre les cartes sont donc des découpes du contour, comme le produisent les
/// logiciels de mise en panneau, et les languettes portent une rangée de perforations.
pub fn panneau(couches: &[Couche], p: &Value) -> Result<(Value, Vec<u8>), String> {
    let f = |k: &str, d: f64| p.get(k).and_then(|x| x.as_f64()).filter(|x| x.is_finite()).unwrap_or(d);
    let nx = f("nx", 2.0).clamp(1.0, 30.0) as usize;
    let ny = f("ny", 2.0).clamp(1.0, 30.0) as usize;
    let vscore = p.get("separation").and_then(|v| v.as_str()) == Some("vscore");
    let ex = f("ecartX", if vscore { 0.0 } else { 2.0 }).clamp(0.0, 50.0);
    let ey = f("ecartY", if vscore { 0.0 } else { 2.0 }).clamp(0.0, 50.0);
    let bord = f("bord", 5.0).clamp(0.0, 50.0);
    let (bh, bv) = match p.get("bords").and_then(|v| v.as_str()).unwrap_or("haut-bas") {
        "tous" => (bord, bord),
        "gauche-droite" => (0.0, bord),
        "aucun" => (0.0, 0.0),
        _ => (bord, 0.0),
    };
    // Écart entre les cartes et les bords : celui du fraisage (aucun pour les rainures en V).
    let (gx, gy) = (if !vscore && bv > 0.0 { ex } else { 0.0 }, if !vscore && bh > 0.0 { ey } else { 0.0 });
    let mires = p.get("mires").and_then(|v| v.as_bool()).unwrap_or(true) && (bh > 0.0 || bv > 0.0);
    let trous_outil = p.get("trousOutil").and_then(|v| v.as_bool()).unwrap_or(true) && (bh >= 4.0 || bv >= 4.0);
    let cuivres = couches.iter().filter(|c| c.role.genre == Genre::Cuivre).count() as u32;

    // Carte : bord extérieur et découpes, d'après le contour ; sinon le rectangle des couches de cuivre.
    let mut anneaux: Vec<Vec<Pt>> = Vec::new();
    if let Some(c) = couches.iter().find(|c| c.role.genre == Genre::Contour && !c.dessin.elements.is_empty()) {
        anneaux = geom::anneaux_contour(&c.dessin);
    }
    anneaux.sort_by(|a, b| geom::aire(b).abs().total_cmp(&geom::aire(a).abs()));
    let mut b = BBox::vide();
    if let Some(ext) = anneaux.first() {
        for q in ext {
            b.ajoute(*q);
        }
        // Le bord extérieur, puis les découpes qui sont à l'intérieur (le reste : cartouches, textes… ignorés).
        let ext = ext.clone();
        let decoupes: Vec<Vec<Pt>> = anneaux[1..].iter().filter(|a| geom::point_dans_anneau(a[0], &ext)).cloned().collect();
        anneaux = std::iter::once(ext).chain(decoupes).collect();
    } else {
        for c in couches.iter().filter(|c| c.role.genre == Genre::Cuivre) {
            b.union(&c.dessin.boite());
        }
        if !b.est_vide() {
            anneaux = vec![vec![pt(b.x0, b.y0), pt(b.x1, b.y0), pt(b.x1, b.y1), pt(b.x0, b.y1)]];
        }
    }
    if b.est_vide() || b.x1 - b.x0 < 0.5 || b.y1 - b.y0 < 0.5 {
        return Err("Dimensions de la carte introuvables.".into());
    }
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    let largeur = 2.0 * (bv + gx) + nx as f64 * w + (nx - 1) as f64 * ex;
    let hauteur = 2.0 * (bh + gy) + ny as f64 * h + (ny - 1) as f64 * ey;
    let origine = |i: usize, j: usize| (bv + gx + i as f64 * (w + ex), bh + gy + j as f64 * (h + ey));
    let mut decalages: Vec<(f64, f64)> = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let (x, y) = origine(i, j);
            decalages.push((x - b.x0, y - b.y0));
        }
    }

    // Matière du panneau : cartes (avec leurs découpes), bords, languettes. Trous des languettes (perforations).
    let mut matiere = Dessin::default();
    let o = matiere.nouvel_objet(Vec::new());
    let mut perforations: Vec<(Pt, f64)> = Vec::new();
    if vscore || (ex == 0.0 && ey == 0.0) {
        matiere.ajoute(rectangle(0.0, 0.0, largeur, hauteur), true, o);
        for &(dx, dy) in &decalages {
            for a in &anneaux[1..] {
                matiere.ajoute(Prim::Poly { anneaux: vec![a.iter().map(|q| pt(q.x + dx, q.y + dy)).collect()] }, false, o);
            }
        }
    } else {
        if bh > 0.0 {
            matiere.ajoute(rectangle(0.0, 0.0, largeur, bh), true, o);
            matiere.ajoute(rectangle(0.0, hauteur - bh, largeur, hauteur), true, o);
        }
        if bv > 0.0 {
            matiere.ajoute(rectangle(0.0, 0.0, bv, hauteur), true, o);
            matiere.ajoute(rectangle(largeur - bv, 0.0, largeur, hauteur), true, o);
        }
        for j in 0..ny {
            for i in 0..nx {
                let (x0, y0) = origine(i, j);
                let (dx, dy) = (x0 - b.x0, y0 - b.y0);
                matiere.ajoute(Prim::Poly { anneaux: anneaux.iter().map(|a| a.iter().map(|q| pt(q.x + dx, q.y + dy)).collect()).collect() }, true, o);
                // Languettes au milieu de chaque côté qui fait face à une autre carte ou à un bord.
                let (x1, y1) = (x0 + w, y0 + h);
                let lt = |l: f64| (l / 3.0).min(5.0);
                let mut ajouter = |a: Pt, c: Pt, nx_: f64, ny_: f64, ecart: f64| {
                    let l = a.dist(c);
                    let m = pt((a.x + c.x) / 2.0, (a.y + c.y) / 2.0);
                    let (ux, uy) = ((c.x - a.x) / l, (c.y - a.y) / l);
                    let demi = lt(l) / 2.0;
                    // Rectangle de la languette : du bord de la carte (1 mm à l'intérieur) au voisin (1 mm au-delà).
                    let coins = [(-demi, -1.0), (demi, -1.0), (demi, ecart + 1.0), (-demi, ecart + 1.0)].map(|(s, t)| pt(m.x + ux * s + nx_ * t, m.y + uy * s + ny_ * t));
                    matiere.ajoute(Prim::Poly { anneaux: vec![coins.to_vec()] }, true, o);
                    let n = ((lt(l) - 1.0) / 0.8).floor().max(1.0) as i32;
                    for k in 0..n {
                        let s = (k as f64 - (n - 1) as f64 / 2.0) * 0.8;
                        perforations.push((pt(m.x + ux * s, m.y + uy * s), 0.5));
                    }
                };
                if i > 0 || bv > 0.0 {
                    ajouter(pt(x0, y0), pt(x0, y1), -1.0, 0.0, if i > 0 { ex } else { gx });
                }
                if i + 1 < nx || bv > 0.0 {
                    ajouter(pt(x1, y0), pt(x1, y1), 1.0, 0.0, if i + 1 < nx { ex } else { gx });
                }
                if j > 0 || bh > 0.0 {
                    ajouter(pt(x0, y0), pt(x1, y0), 0.0, -1.0, if j > 0 { ey } else { gy });
                }
                if j + 1 < ny || bh > 0.0 {
                    ajouter(pt(x0, y1), pt(x1, y1), 0.0, 1.0, if j + 1 < ny { ey } else { gy });
                }
            }
        }
    }
    // Contour : bords de la matière (et de ses trous), en trait.
    let mut contour = Dessin::default();
    let o = contour.nouvel_objet(vec![("Fonction".into(), "Profile".into())]);
    for forme in crate::dfm::aplatir_formes(&matiere) {
        for a in forme {
            for k in 0..a.len() {
                let (p0, p1) = (a[k], a[(k + 1) % a.len()]);
                contour.ajoute(Prim::Cap { a: pt(p0[0], p0[1]), b: pt(p1[0], p1[1]), r: 0.05 }, true, o);
            }
        }
    }

    // Mires (à 12 mm des coins, loin des trous d'outillage) et trous d'outillage (Ø 3 mm, au coin des bords).
    let mut positions_mires: Vec<Pt> = Vec::new();
    if mires {
        let mx = 12.0_f64.min(largeur / 4.0);
        let my = 12.0_f64.min(hauteur / 4.0);
        if bh > 0.0 {
            positions_mires.extend([pt(mx, bh / 2.0), pt(largeur - mx, bh / 2.0), pt(mx, hauteur - bh / 2.0)]);
        } else {
            positions_mires.extend([pt(bv / 2.0, my), pt(bv / 2.0, hauteur - my), pt(largeur - bv / 2.0, my)]);
        }
    }
    let disque = |r: f64, pts: &[Pt]| {
        let mut d = Dessin::default();
        let o = d.nouvel_objet(vec![("Fonction".into(), "Fiducial,Panel".into())]);
        for c in pts {
            d.ajoute(Prim::Cap { a: *c, b: *c, r }, true, o);
        }
        d
    };
    let mires_cuivre = disque(0.5, &positions_mires);
    let mires_vernis = disque(1.0, &positions_mires);
    let mut npth_panneau = Percages::default();
    if trous_outil {
        let m = if bh >= 4.0 { pt(3.5_f64.min(largeur / 8.0), bh / 2.0) } else { pt(bv / 2.0, 3.5_f64.min(hauteur / 8.0)) };
        for c in [pt(m.x, m.y), pt(largeur - m.x, m.y), pt(m.x, hauteur - m.y), pt(largeur - m.x, hauteur - m.y)] {
            npth_panneau.trous.push((c, 3.0));
        }
    }
    npth_panneau.trous.extend(perforations);

    // Rainures en V : à chaque bord de carte intérieur au panneau.
    let mut rainures = Dessin::default();
    if vscore {
        let o = rainures.nouvel_objet(vec![("Fonction".into(), "V-score".into())]);
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for i in 0..nx {
            let (x, _) = origine(i, 0);
            xs.extend([x, x + w]);
        }
        for j in 0..ny {
            let (_, y) = origine(0, j);
            ys.extend([y, y + h]);
        }
        let interieur = |v: f64, max: f64| v > 0.01 && v < max - 0.01;
        xs.retain(|x| interieur(*x, largeur));
        ys.retain(|y| interieur(*y, hauteur));
        xs.sort_by(f64::total_cmp);
        ys.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        for x in xs {
            rainures.ajoute(Prim::Cap { a: pt(x, 0.0), b: pt(x, hauteur), r: 0.1 }, true, o);
        }
        for y in ys {
            rainures.ajoute(Prim::Cap { a: pt(0.0, y), b: pt(largeur, y), r: 0.1 }, true, o);
        }
    }

    // Fichiers
    let mut fichiers: Vec<(String, Vec<u8>)> = Vec::new();
    let ajouter_fichier = |fichiers: &mut Vec<(String, Vec<u8>)>, mut nom: String, contenu: Vec<u8>| {
        // Deux couches au même nom (formats d'origine différents) : suffixe numéroté.
        let base = nom.clone();
        let mut k = 2;
        while fichiers.iter().any(|f| f.0 == nom) {
            nom = match base.rsplit_once('.') {
                Some((a, e)) => format!("{a}-{k}.{e}"),
                None => format!("{base}-{k}"),
            };
            k += 1;
        }
        fichiers.push((nom, contenu));
    };
    let mut a_contour = false;
    let mut a_npth = false;
    let alertes: Vec<String> = Vec::new();
    for c in couches.iter().filter(|c| !c.dessin.elements.is_empty()) {
        match c.role.genre {
            Genre::Percage | Genre::Fraisage => {
                let metallise = crate::dfm::couche_metallisee(c);
                let (de, a) = crate::dfm::etendue_percage(c, couches).unwrap_or((1, cuivres.max(2)));
                let mut pc = Percages::default();
                // Trous d'une autre métallisation que la couche (fichier mixte, d'après ses outils) : fichier à part.
                let mut autres = Percages::default();
                let mut segments: Vec<(Pt, Pt, f64)> = Vec::new();
                for e in c.dessin.elements.iter().filter(|e| e.sombre) {
                    if let Prim::Cap { a, b: q, r } = e.prim {
                        if a.dist(q) < 1e-9 {
                            if crate::dfm::metallise_trou(c, 2.0 * r, metallise) == metallise {
                                pc.trous.push((a, 2.0 * r));
                            } else {
                                autres.trous.push((a, 2.0 * r));
                            }
                        } else {
                            segments.push((a, q, 2.0 * r));
                        }
                    }
                }
                // Fraisages lus segment par segment : réécrits en chemins continus (G00, M15, G01…, M16).
                let mut chemins = chainer(&segments);
                if c.role.genre == Genre::Percage {
                    // Dans un fichier de perçage, un chemin d'un seul segment est une rainure (G85).
                    for (pts, d) in std::mem::take(&mut chemins) {
                        if pts.len() == 2 {
                            pc.fraisages.push((pts[0], pts[1], d));
                        } else {
                            chemins.push((pts, d));
                        }
                    }
                }
                let mut copie = Percages::default();
                for &(dx, dy) in &decalages {
                    let t = |q: Pt| pt(q.x + dx, q.y + dy);
                    copie.trous.extend(pc.trous.iter().map(|(q, d)| (t(*q), *d)));
                    copie.fraisages.extend(pc.fraisages.iter().map(|(q, r, d)| (t(*q), t(*r), *d)));
                    copie.chemins.extend(chemins.iter().map(|(pts, d)| (pts.iter().map(|q| t(*q)).collect(), *d)));
                }
                if !metallise && !a_npth {
                    copie.trous.extend(npth_panneau.trous.iter().copied());
                    a_npth = true;
                }
                let nom = nom_fichier(c, "drl");
                let nom = if nom.ends_with(".gbr") { nom.replace(".gbr", ".drl") } else { nom };
                if !autres.trous.is_empty() {
                    let mut copie_autres = Percages::default();
                    for &(dx, dy) in &decalages {
                        copie_autres.trous.extend(autres.trous.iter().map(|(q, d)| (pt(q.x + dx, q.y + dy), *d)));
                    }
                    let suffixe = if metallise { "-NPTH" } else { "-PTH" };
                    let nom_autres = match nom.rsplit_once('.') {
                        Some((b, e)) => format!("{b}{suffixe}.{e}"),
                        None => format!("{nom}{suffixe}"),
                    };
                    ajouter_fichier(&mut fichiers, nom_autres, excellon(&copie_autres, !metallise, de, a).into_bytes());
                }
                ajouter_fichier(&mut fichiers, nom, excellon(&copie, metallise, de, a).into_bytes());
            }
            Genre::Contour => {
                if a_contour {
                    continue;
                }
                a_contour = true;
                ajouter_fichier(&mut fichiers, nom_fichier(c, "gbr"), gerber(&[(&contour, vec![(0.0, 0.0)])], Some("Profile,NP"), "Contour du panneau").into_bytes());
            }
            Genre::Netlist | Genre::Job | Genre::Plan => {}
            _ => {
                let fonction = fonction_x2(c, cuivres);
                let mut sources: Vec<(&Dessin, Vec<(f64, f64)>)> = vec![(&c.dessin, decalages.clone())];
                if mires && matches!(c.role.genre, Genre::Cuivre) && matches!(c.role.cote, Cote::Dessus | Cote::Dessous) {
                    sources.push((&mires_cuivre, vec![(0.0, 0.0)]));
                }
                if mires && matches!(c.role.genre, Genre::Masque) {
                    sources.push((&mires_vernis, vec![(0.0, 0.0)]));
                }
                ajouter_fichier(&mut fichiers, nom_fichier(c, "gbr"), gerber(&sources, fonction.as_deref(), &c.nom).into_bytes());
            }
        }
    }
    if !a_contour {
        ajouter_fichier(&mut fichiers, "panneau-contour.gbr".into(), gerber(&[(&contour, vec![(0.0, 0.0)])], Some("Profile,NP"), "Contour du panneau").into_bytes());
    }
    if !a_npth && !npth_panneau.trous.is_empty() {
        ajouter_fichier(&mut fichiers, "panneau-NPTH.drl".into(), excellon(&npth_panneau, false, 1, cuivres.max(2)).into_bytes());
    }
    if !rainures.elements.is_empty() {
        ajouter_fichier(&mut fichiers, "panneau-rainurage-V.gbr".into(), gerber(&[(&rainures, vec![(0.0, 0.0)])], Some("Vcut"), "Rainures en V").into_bytes());
    }
    let noms: Vec<&str> = fichiers.iter().map(|f| f.0.as_str()).collect();
    let j = json!({"fichiers": noms, "largeur": largeur, "hauteur": hauteur, "cartes": nx * ny, "carte": [w, h], "alertes": alertes});
    Ok((j, zip(&fichiers)))
}
