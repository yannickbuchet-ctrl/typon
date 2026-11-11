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

/// Chemin d'une région : bord extérieur, et chaque trou rattaché au bord par une « coupure » (cut-in).
fn chemin_region(anneaux: &[Vec<Pt>]) -> Vec<Pt> {
    let mut chemin: Vec<Pt> = Vec::new();
    let Some(ext) = anneaux.first() else { return chemin };
    if ext.len() < 3 {
        return chemin;
    }
    chemin.extend(ext.iter().copied());
    chemin.push(ext[0]);
    for trou in &anneaux[1..] {
        if trou.len() < 3 {
            continue;
        }
        chemin.extend(trou.iter().copied());
        chemin.push(trou[0]);
        chemin.push(ext[0]);
    }
    chemin
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

/// Trous (centre, diamètre) et fraisages (début, fin, diamètre).
#[derive(Default)]
pub struct Percages {
    pub trous: Vec<(Pt, f64)>,
    pub fraisages: Vec<(Pt, Pt, f64)>,
}

pub fn excellon(p: &Percages, metallise: bool, cuivres: u32) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "M48");
    let _ = writeln!(s, "; #@! TF.GenerationSoftware,Typon,Typon,1.0");
    let _ = writeln!(s, "; #@! TF.FileFunction,{},1,{},{}", if metallise { "Plated" } else { "NonPlated" }, cuivres.max(2), if metallise { "PTH" } else { "NPTH" });
    let _ = writeln!(s, "FMAT,2");
    let _ = writeln!(s, "METRIC");
    let mut outils: Vec<i64> = Vec::new();
    for d in p.trous.iter().map(|t| t.1).chain(p.fraisages.iter().map(|f| f.2)) {
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
    let _ = writeln!(s, "G05");
    for (i, k) in outils.iter().enumerate() {
        let trous: Vec<_> = p.trous.iter().filter(|t| (t.1 * 1000.0).round() as i64 == *k).collect();
        let fraisages: Vec<_> = p.fraisages.iter().filter(|f| (f.2 * 1000.0).round() as i64 == *k).collect();
        if trous.is_empty() && fraisages.is_empty() {
            continue;
        }
        let _ = writeln!(s, "T{}", i + 1);
        for (c, _) in trous {
            let _ = writeln!(s, "X{:.4}Y{:.4}", c.x, c.y);
        }
        for (a, b, _) in fraisages {
            let _ = writeln!(s, "X{:.4}Y{:.4}G85X{:.4}Y{:.4}", a.x, a.y, b.x, b.y);
        }
    }
    let _ = writeln!(s, "T0");
    let _ = writeln!(s, "M30");
    s
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

/// Panneau de `nx` × `ny` cartes. Paramètres (mm) : ecartX, ecartY, bord (largeur des bords de manutention),
/// bords (« haut-bas », « gauche-droite », « tous », « aucun »), separation (« vscore » ou « fraisage »), mires et
/// trousOutil (booléens).
pub fn panneau(couches: &[Couche], p: &Value) -> Result<(Value, Vec<u8>), String> {
    let f = |k: &str, d: f64| p.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
    let nx = f("nx", 2.0).clamp(1.0, 30.0) as usize;
    let ny = f("ny", 2.0).clamp(1.0, 30.0) as usize;
    let vscore = p.get("separation").and_then(|v| v.as_str()) == Some("vscore");
    let ex = if vscore { f("ecartX", 0.0) } else { f("ecartX", 2.0) }.clamp(0.0, 50.0);
    let ey = if vscore { f("ecartY", 0.0) } else { f("ecartY", 2.0) }.clamp(0.0, 50.0);
    let bord = f("bord", 5.0).clamp(0.0, 50.0);
    let bords = p.get("bords").and_then(|v| v.as_str()).unwrap_or("haut-bas");
    let (bh, bv) = match bords {
        "tous" => (bord, bord),
        "gauche-droite" => (0.0, bord),
        "aucun" => (0.0, 0.0),
        _ => (bord, 0.0),
    };
    let mires = p.get("mires").and_then(|v| v.as_bool()).unwrap_or(true) && (bh > 0.0 || bv > 0.0);
    let trous_outil = p.get("trousOutil").and_then(|v| v.as_bool()).unwrap_or(true) && (bh >= 4.0 || bv >= 4.0);

    // Carte : contour s'il y en a un, sinon les couches de cuivre.
    let mut b = BBox::vide();
    if let Some(c) = couches.iter().find(|c| c.role.genre == Genre::Contour && !c.dessin.elements.is_empty()) {
        b = c.dessin.boite_axes();
    }
    if b.est_vide() {
        for c in couches.iter().filter(|c| c.role.genre == Genre::Cuivre) {
            b.union(&c.dessin.boite());
        }
    }
    if b.est_vide() || b.x1 - b.x0 < 0.5 || b.y1 - b.y0 < 0.5 {
        return Err("Dimensions de la carte introuvables.".into());
    }
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    let largeur = 2.0 * bv + nx as f64 * w + (nx - 1) as f64 * ex;
    let hauteur = 2.0 * bh + ny as f64 * h + (ny - 1) as f64 * ey;
    let mut decalages: Vec<(f64, f64)> = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            decalages.push((bv + i as f64 * (w + ex) - b.x0, bh + j as f64 * (h + ey) - b.y0));
        }
    }
    let cuivres = couches.iter().filter(|c| c.role.genre == Genre::Cuivre).count() as u32;

    // Éléments ajoutés : mires (cuivre et ouverture du vernis), cadre du panneau.
    let mut positions_mires: Vec<Pt> = Vec::new();
    if mires {
        if bh > 0.0 {
            positions_mires.extend([pt(5.0_f64.min(largeur / 4.0), bh / 2.0), pt(largeur - 5.0_f64.min(largeur / 4.0), bh / 2.0), pt(5.0_f64.min(largeur / 4.0), hauteur - bh / 2.0)]);
        } else {
            positions_mires.extend([pt(bv / 2.0, 5.0_f64.min(hauteur / 4.0)), pt(bv / 2.0, hauteur - 5.0_f64.min(hauteur / 4.0)), pt(largeur - bv / 2.0, 5.0_f64.min(hauteur / 4.0))]);
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
    let mut cadre = Dessin::default();
    let o = cadre.nouvel_objet(vec![("Fonction".into(), "Profile".into())]);
    let coins = [pt(0.0, 0.0), pt(largeur, 0.0), pt(largeur, hauteur), pt(0.0, hauteur)];
    for k in 0..4 {
        cadre.ajoute(Prim::Cap { a: coins[k], b: coins[(k + 1) % 4], r: 0.05 }, true, o);
    }

    // Trous non métallisés du panneau : outillage (coins) et perforations des languettes.
    let mut npth_panneau = Percages::default();
    if trous_outil {
        let m = if bh >= 4.0 { pt(3.5_f64.min(largeur / 8.0), bh / 2.0) } else { pt(bv / 2.0, 3.5_f64.min(hauteur / 8.0)) };
        let (mx, my) = (m.x, m.y);
        for c in [pt(mx, my), pt(largeur - mx, my), pt(mx, hauteur - my), pt(largeur - mx, hauteur - my)] {
            npth_panneau.trous.push((c, 3.0));
        }
    }
    // Séparation : rainures en V (lignes) ou languettes perforées au milieu de chaque côté qui fait face à un vide.
    let mut separation = Dessin::default();
    let o = separation.nouvel_objet(vec![("Fonction".into(), if vscore { "V-score".into() } else { "Rout".into() })]);
    if vscore {
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for i in 0..=nx {
            let x = bv + i as f64 * (w + ex) - if i == nx { ex } else { 0.0 };
            if (i > 0 || bv > 0.0) && (i < nx || bv > 0.0) {
                xs.push(x);
            }
            if ex > 0.0 && i > 0 && i < nx {
                xs.push(bv + i as f64 * (w + ex) - ex);
            }
        }
        for j in 0..=ny {
            let y = bh + j as f64 * (h + ey) - if j == ny { ey } else { 0.0 };
            if (j > 0 || bh > 0.0) && (j < ny || bh > 0.0) {
                ys.push(y);
            }
            if ey > 0.0 && j > 0 && j < ny {
                ys.push(bh + j as f64 * (h + ey) - ey);
            }
        }
        for x in xs {
            separation.ajoute(Prim::Cap { a: pt(x, 0.0), b: pt(x, hauteur), r: 0.1 }, true, o);
        }
        for y in ys {
            separation.ajoute(Prim::Cap { a: pt(0.0, y), b: pt(largeur, y), r: 0.1 }, true, o);
        }
    } else {
        // Fraisage autour de chaque carte (contour répété) ; languettes de 5 trous Ø 0,5 au pas de 0,8 mm.
        for &(dx, dy) in &decalages {
            let (x0, y0, x1, y1) = (b.x0 + dx, b.y0 + dy, b.x1 + dx, b.y1 + dy);
            let mut cotes: Vec<(Pt, Pt)> = Vec::new();
            if y0 > 0.01 {
                cotes.push((pt(x0, y0), pt(x1, y0)));
            }
            if y1 < hauteur - 0.01 {
                cotes.push((pt(x0, y1), pt(x1, y1)));
            }
            if x0 > 0.01 {
                cotes.push((pt(x0, y0), pt(x0, y1)));
            }
            if x1 < largeur - 0.01 {
                cotes.push((pt(x1, y0), pt(x1, y1)));
            }
            for (a, c) in cotes {
                let m = pt((a.x + c.x) / 2.0, (a.y + c.y) / 2.0);
                let l = a.dist(c).max(1e-9);
                let (ux, uy) = ((c.x - a.x) / l, (c.y - a.y) / l);
                for k in -2..=2 {
                    npth_panneau.trous.push((pt(m.x + ux * 0.8 * k as f64, m.y + uy * 0.8 * k as f64), 0.5));
                }
            }
        }
    }

    // Fichiers
    let mut fichiers: Vec<(String, Vec<u8>)> = Vec::new();
    let mut a_contour = false;
    let mut a_npth = false;
    for c in couches.iter().filter(|c| !c.dessin.elements.is_empty()) {
        match c.role.genre {
            Genre::Percage | Genre::Fraisage => {
                let libelle = format!("{} {}", c.role.libelle, c.nom).to_lowercase();
                let metallise = !(libelle.contains("non métallisé") || libelle.contains("npth") || libelle.contains("non-plated") || libelle.contains("non plated"));
                let mut pc = Percages::default();
                for &(dx, dy) in &decalages {
                    for e in c.dessin.elements.iter().filter(|e| e.sombre) {
                        if let Prim::Cap { a, b: q, r } = e.prim {
                            let (a, q) = (pt(a.x + dx, a.y + dy), pt(q.x + dx, q.y + dy));
                            if a.dist(q) < 1e-9 {
                                pc.trous.push((a, 2.0 * r));
                            } else {
                                pc.fraisages.push((a, q, 2.0 * r));
                            }
                        }
                    }
                }
                if !metallise && !a_npth {
                    pc.trous.extend(npth_panneau.trous.iter().copied());
                    a_npth = true;
                }
                fichiers.push((nom_fichier(c, "drl").replace(".gbr", ".drl"), excellon(&pc, metallise, cuivres).into_bytes()));
            }
            Genre::Contour => {
                if a_contour {
                    continue;
                }
                a_contour = true;
                let texte = gerber(&[(&cadre, vec![(0.0, 0.0)])], Some("Profile,NP"), "Contour du panneau");
                fichiers.push((nom_fichier(c, "gbr"), texte.into_bytes()));
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
                let texte = gerber(&sources, fonction.as_deref(), &c.nom);
                fichiers.push((nom_fichier(c, "gbr"), texte.into_bytes()));
            }
        }
    }
    if !a_contour {
        fichiers.push(("panneau-contour.gbr".into(), gerber(&[(&cadre, vec![(0.0, 0.0)])], Some("Profile,NP"), "Contour du panneau").into_bytes()));
    }
    if !a_npth && !npth_panneau.trous.is_empty() {
        fichiers.push(("panneau-NPTH.drl".into(), excellon(&npth_panneau, false, cuivres).into_bytes()));
    }
    if !separation.elements.is_empty() {
        let (nom, fonction) = if vscore { ("panneau-rainurage-V.gbr", "Vcut") } else { ("panneau-fraisage.gbr", "Other,Rout") };
        let mut d = separation.clone();
        if !vscore {
            // Le contour de chaque carte, à fraiser.
            if let Some(c) = couches.iter().find(|c| c.role.genre == Genre::Contour) {
                let o = d.nouvel_objet(vec![("Fonction".into(), "Rout".into())]);
                for &(dx, dy) in &decalages {
                    for e in &c.dessin.elements {
                        d.ajoute(e.prim.transformee(&geom::Transfo::translation(dx, dy)), true, o);
                    }
                }
            }
        }
        fichiers.push((nom.into(), gerber(&[(&d, vec![(0.0, 0.0)])], Some(fonction), if vscore { "Rainures en V" } else { "Fraisage des cartes" }).into_bytes()));
    }
    let noms: Vec<&str> = fichiers.iter().map(|f| f.0.as_str()).collect();
    let j = json!({"fichiers": noms, "largeur": largeur, "hauteur": hauteur, "cartes": nx * ny, "carte": [w, h]});
    Ok((j, zip(&fichiers)))
}
