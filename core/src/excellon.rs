//! Lecture des fichiers de perçage Excellon (et XNC, son sous-ensemble normalisé par Ucamco) : outils, trous, rainures
//! (G85), fraisages (G00/G01/G02/G03 avec M15/M16), répétitions (R), zéros omis, unités.

use crate::geom::*;
use std::collections::BTreeMap;

pub struct Outil {
    pub numero: u32,
    pub diametre: f64,
    pub trous: usize,
    pub fraisages: usize,
}

pub struct Excellon {
    pub dessin: Dessin,
    pub outils: Vec<Outil>,
    pub unite: &'static str,
    pub format: String,
    /// Attributs X2 des fichiers XNC ou KiCad (« ; #@! TF.FileFunction,Plated,1,2,PTH »).
    pub attributs: Vec<(String, Vec<String>)>,
    pub commentaires: Vec<String>,
    pub alertes: Vec<String>,
}

/// Le texte ressemble-t-il à un fichier Excellon ?
pub fn reconnaitre(texte: &str) -> bool {
    let debut: String = texte.chars().take(4000).collect();
    let lignes: Vec<&str> = debut.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lignes.iter().any(|l| *l == "M48") {
        return true;
    }
    // Fichier sans en-tête : outils « T1C0.8 » ou suite de « T01 » puis « X…Y… », sans commande Gerber.
    let gerber = debut.contains("%FS") || debut.contains("%MO") || debut.contains("D01*") || debut.contains("D03*");
    !gerber && lignes.iter().any(|l| l.starts_with('T') && l[1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
        && lignes.iter().any(|l| l.starts_with('X') || l.starts_with('Y'))
}

pub fn lire(texte: &str) -> Excellon {
    let mut res = Excellon { dessin: Dessin::default(), outils: Vec::new(), unite: "pouce", format: String::new(), attributs: Vec::new(), commentaires: Vec::new(), alertes: Vec::new() };
    let mut metrique = false;
    let mut zeros_tete_gardes = true; // LZ : zéros de tête présents, zéros de fin omis
    let mut entiers: Option<usize> = None;
    let mut decimales: Option<usize> = None;
    let mut diametres: BTreeMap<u32, f64> = BTreeMap::new();
    let mut comptes: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut outil: u32 = 0;
    let mut x = 0.0;
    let mut y = 0.0;
    let mut incremental = false;
    let mut en_tete = false;
    let mut fraisage = false; // mode G00/G01… actif
    let mut outil_baisse = false;
    let mut mode_g = 5; // 5 : perçage, 0 : déplacement, 1 : ligne, 2/3 : arcs
    let mut unite_vue = false;
    let mut objets_outil: BTreeMap<u32, u32> = BTreeMap::new();

    // Les zéros et l'unité peuvent être déclarés après les premiers outils : on lit l'en-tête d'abord.
    for ligne in texte.lines() {
        let l = ligne.trim();
        let u = l.to_ascii_uppercase();
        if u.starts_with("METRIC") || u == "M71" {
            metrique = true;
            unite_vue = true;
        } else if u.starts_with("INCH") || u == "M72" {
            metrique = false;
            unite_vue = true;
        }
        if u.starts_with("METRIC") || u.starts_with("INCH") {
            if u.contains(",TZ") {
                zeros_tete_gardes = false;
            } else if u.contains(",LZ") {
                zeros_tete_gardes = true;
            }
            // « METRIC,TZ,000.000 » : format explicite
            if let Some(f) = u.split(',').find(|p| p.contains('.') && p.chars().all(|c| c == '0' || c == '.')) {
                let mut s = f.split('.');
                entiers = Some(s.next().unwrap_or("").len());
                decimales = Some(s.next().unwrap_or("").len());
            }
        }
        if let Some(f) = l.strip_prefix(";FILE_FORMAT=") {
            let mut s = f.split(':');
            entiers = s.next().and_then(|v| v.trim().parse().ok());
            decimales = s.next().and_then(|v| v.trim().parse().ok());
        }
        if u == "%" || u == "M95" {
            break;
        }
    }
    let (ent, dec) = (entiers.unwrap_or(if metrique { 3 } else { 2 }), decimales.unwrap_or(if metrique { 3 } else { 4 }));
    let facteur = if metrique { 1.0 } else { 25.4 };
    res.unite = if metrique { "mm" } else { "pouce" };
    res.format = format!("{ent}.{dec}, {}", if zeros_tete_gardes { "zéros de tête présents" } else { "zéros de fin présents" });
    if !unite_vue {
        res.alertes.push("Unité non indiquée : pouces supposés (valeur par défaut d'Excellon).".into());
    }
    // Coordonnées à point décimal : le format n'importe pas.
    let avec_point = texte.lines().any(|l| { let l = l.trim_start(); (l.starts_with('X') || l.starts_with('Y')) && l.contains('.') });
    if entiers.is_none() && !avec_point && texte.lines().any(|l| l.trim_start().starts_with('X') || l.trim_start().starts_with('Y')) {
        res.alertes.push(format!("Format des coordonnées non indiqué : {ent}.{dec} supposé. Vérifiez les dimensions (une erreur d'échelle ×10 est possible)."));
    }

    let coord = |t: &str| -> f64 {
        let v = if t.contains('.') {
            t.parse::<f64>().unwrap_or(0.0)
        } else {
            let (signe, ch) = match t.as_bytes().first() {
                Some(b'-') => (-1.0, &t[1..]),
                Some(b'+') => (1.0, &t[1..]),
                _ => (1.0, t),
            };
            let n = if zeros_tete_gardes {
                // Zéros de fin omis : on complète à droite jusqu'à ent + dec chiffres.
                let mut s = ch.to_string();
                while s.len() < ent + dec {
                    s.push('0');
                }
                s.parse::<f64>().unwrap_or(0.0) / 10f64.powi(dec as i32)
            } else {
                ch.parse::<f64>().unwrap_or(0.0) / 10f64.powi(dec as i32)
            };
            signe * n
        };
        v * facteur
    };

    let mut lignes = texte.lines().peekable();
    let mut dessin = Dessin::default();
    let mut premier_point_fraisage: Option<Pt> = None;
    let _ = &mut premier_point_fraisage;
    while let Some(ligne) = lignes.next() {
        let l = ligne.trim();
        if l.is_empty() {
            continue;
        }
        if let Some(c) = l.strip_prefix(';') {
            let c = c.trim();
            if let Some(a) = c.strip_prefix("#@!").map(|s| s.trim()) {
                if let Some(tf) = a.strip_prefix("TF.") {
                    let mut p = tf.split(',');
                    let nom = p.next().unwrap_or("").to_string();
                    res.attributs.push((nom, p.map(|s| s.to_string()).collect()));
                }
            } else if res.commentaires.len() < 30 && !c.is_empty() {
                res.commentaires.push(c.to_string());
            }
            continue;
        }
        let u = l.to_ascii_uppercase();
        if u == "M48" {
            en_tete = true;
            continue;
        }
        if u == "%" || u == "M95" {
            en_tete = false;
            continue;
        }
        if u == "M30" || u == "M00" {
            break;
        }
        if u.starts_with("METRIC") || u.starts_with("INCH") || u == "M71" || u == "M72" || u.starts_with("FMAT") || u.starts_with("VER") || u.starts_with("ICI") {
            continue;
        }
        if u == "G90" {
            incremental = false;
            continue;
        }
        if u == "G91" {
            incremental = true;
            continue;
        }
        if u.starts_with('T') && u[1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            // T01C0.800F…S… (définition) ou T01 (sélection)
            let corps = &u[1..];
            let fin = corps.find(|c: char| !c.is_ascii_digit()).unwrap_or(corps.len());
            let n: u32 = corps[..fin].parse().unwrap_or(0);
            if let Some(p) = corps.find('C') {
                let rest = &corps[p + 1..];
                let f = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
                let d: f64 = rest[..f].parse().unwrap_or(0.0);
                diametres.insert(n, d * facteur);
                if !en_tete {
                    outil = n;
                }
            } else {
                outil = n;
                if en_tete {
                    continue;
                }
            }
            if n == 0 {
                outil = 0;
            }
            continue;
        }
        if en_tete {
            continue;
        }
        // Fraisage : modes G00/G01/G02/G03, plongée M15, remontée M16/M17.
        if u.starts_with("M15") {
            outil_baisse = true;
            continue;
        }
        if u.starts_with("M16") || u.starts_with("M17") {
            outil_baisse = false;
            continue;
        }
        if u.starts_with("G05") || u.starts_with("G81") {
            fraisage = false;
            mode_g = 5;
            continue;
        }
        let mut corps = u.as_str();
        let mut g: Option<u32> = None;
        if corps.starts_with('G') {
            let f = corps[1..].find(|c: char| !c.is_ascii_digit()).map(|p| p + 1).unwrap_or(corps.len());
            g = corps[1..f].parse().ok();
            corps = &corps[f..];
        }
        let diametre = *diametres.get(&outil).unwrap_or(&0.0);
        let info_trou: crate::geom::Info = vec![
            ("Objet".into(), if fraisage && mode_g != 5 { "Fraisage".into() } else { "Perçage".into() }),
            ("Outil".into(), format!("T{outil:02}")),
            ("Diamètre".into(), format!("{} mm", crate::gerber::nombre(diametre))),
        ];
        let objet = dessin.nouvel_objet(info_trou);
        let _ = &mut objets_outil;
        if let Some(gc) = g {
            if gc <= 3 {
                mode_g = gc;
                fraisage = true;
            }
        }
        // Répétition : R<n>X<dx>Y<dy>
        if corps.starts_with('R') {
            let (n, dx, dy) = lire_repetition(corps, &coord);
            for _ in 0..n {
                x += dx;
                y += dy;
                dessin.ajoute(Prim::Cap { a: pt(x, y), b: pt(x, y), r: diametre / 2.0 }, true, objet);
                comptes.entry(outil).or_default().0 += 1;
            }
            continue;
        }
        // Rainure : X…Y…G85X…Y…
        if let Some(p) = corps.find("G85") {
            let (a, _) = lire_xy(&corps[..p], &coord, x, y, incremental);
            let (b, _) = lire_xy(&corps[p + 3..], &coord, a.x, a.y, incremental);
            dessin.ajoute(Prim::Cap { a, b, r: diametre / 2.0 }, true, objet);
            comptes.entry(outil).or_default().1 += 1;
            x = b.x;
            y = b.y;
            continue;
        }
        if corps.starts_with('X') || corps.starts_with('Y') || corps.starts_with('A') || corps.starts_with('I') {
            let debut = pt(x, y);
            let (p, extra) = lire_xy(corps, &coord, x, y, incremental);
            x = p.x;
            y = p.y;
            if fraisage && mode_g != 5 {
                // G00 : déplacement outil levé. G01/G02/G03 : fraisage (dessiné même sans M15, que beaucoup de
                // logiciels omettent).
                if mode_g != 0 {
                    dessiner_fraisage(&mut dessin, debut, p, mode_g, &extra, diametre, objet);
                    comptes.entry(outil).or_default().1 += 1;
                }
                let _ = outil_baisse;
            } else {
                dessin.ajoute(Prim::Cap { a: p, b: p, r: diametre / 2.0 }, true, objet);
                comptes.entry(outil).or_default().0 += 1;
            }
        }
    }
    for (n, d) in &diametres {
        let (t, f) = comptes.get(n).copied().unwrap_or((0, 0));
        res.outils.push(Outil { numero: *n, diametre: *d, trous: t, fraisages: f });
    }
    res.dessin = dessin;
    res
}

struct Extra {
    a: Option<f64>,
    i: Option<f64>,
    j: Option<f64>,
}

fn lire_xy(t: &str, coord: &dyn Fn(&str) -> f64, x0: f64, y0: f64, incremental: bool) -> (Pt, Extra) {
    let mut x = x0;
    let mut y = y0;
    let mut extra = Extra { a: None, i: None, j: None };
    let b = t.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() {
            let d = i + 1;
            let mut f = d;
            while f < b.len() && (b[f].is_ascii_digit() || b[f] == b'.' || b[f] == b'-' || b[f] == b'+') {
                f += 1;
            }
            let v = &t[d..f];
            match c {
                'X' => x = if incremental { x0 + coord(v) } else { coord(v) },
                'Y' => y = if incremental { y0 + coord(v) } else { coord(v) },
                'A' => extra.a = Some(coord(v)),
                'I' => extra.i = Some(coord(v)),
                'J' => extra.j = Some(coord(v)),
                _ => {}
            }
            i = f.max(i + 1);
        } else {
            i += 1;
        }
    }
    (pt(x, y), extra)
}

fn lire_repetition(t: &str, coord: &dyn Fn(&str) -> f64) -> (usize, f64, f64) {
    let f = t[1..].find(|c: char| !c.is_ascii_digit()).map(|p| p + 1).unwrap_or(t.len());
    let n: usize = t[1..f].parse().unwrap_or(0);
    let (p, _) = lire_xy(&t[f..], coord, 0.0, 0.0, false);
    (n.min(100_000), p.x, p.y)
}

fn dessiner_fraisage(dessin: &mut Dessin, a: Pt, b: Pt, mode: u32, extra: &Extra, diametre: f64, objet: u32) {
    let r = diametre / 2.0;
    if mode == 2 || mode == 3 {
        let horaire = mode == 2;
        let centre = if let (Some(i), Some(j)) = (extra.i, extra.j) {
            pt(a.x + i, a.y + j)
        } else if let Some(rayon) = extra.a {
            centre_par_rayon(a, b, rayon, horaire)
        } else {
            pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
        };
        let pts = arc(a, b, centre, horaire);
        for w in pts.windows(2) {
            dessin.ajoute(Prim::Cap { a: w[0], b: w[1], r }, true, objet);
        }
    } else {
        dessin.ajoute(Prim::Cap { a, b, r }, true, objet);
    }
}

fn centre_par_rayon(a: Pt, b: Pt, rayon: f64, horaire: bool) -> Pt {
    let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let d = a.dist(b);
    let h = (rayon * rayon - d * d / 4.0).max(0.0).sqrt();
    let (ux, uy) = ((b.x - a.x) / d.max(1e-12), (b.y - a.y) / d.max(1e-12));
    // Arc de moins d'un demi-tour : centre à droite du trajet en sens horaire, à gauche sinon.
    let s = if horaire { -1.0 } else { 1.0 };
    pt(m.x - s * uy * h, m.y + s * ux * h)
}
