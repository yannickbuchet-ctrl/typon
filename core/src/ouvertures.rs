//! Fichiers d'ouvertures séparés des anciens Gerber RS-274D (« roues », aperture lists) : Eagle (.whl), Altium et
//! Protel (.apr), tableaux génériques (D-code, forme, dimensions), et listes de commandes %AD…*% dans un fichier à part.
//!
//! Le RS-274D ne dit ni l'unité ni le format des coordonnées : ils sont déduits des coordonnées elles-mêmes (voir
//! `format_probable`).

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Forme {
    Cercle,
    Carre,
    Rectangle,
    Oblong,
    Octogone,
    Anneau,
    Thermique,
}

#[derive(Clone, Debug)]
pub struct Ouverture {
    pub forme: Forme,
    /// Dimensions en mm : largeur (ou diamètre) et hauteur (ou diamètre intérieur pour un anneau, une thermique).
    pub a: f64,
    pub b: f64,
}

pub struct Liste {
    pub ouvertures: BTreeMap<i32, Ouverture>,
    /// Unité retenue pour les dimensions, pour l'afficher : « pouces », « mils » ou « mm ».
    pub unite: &'static str,
}

/// Mot de forme -> forme.
fn forme(mot: &str) -> Option<Forme> {
    let m = mot.to_ascii_lowercase();
    Some(match m.as_str() {
        "c" | "circ" | "circle" | "circular" | "round" | "rounded" | "dot" | "rnd" | "r0" => Forme::Cercle,
        "s" | "sq" | "square" | "sqr" => Forme::Carre,
        "r" | "rect" | "rectangle" | "rectangular" | "rectang" | "rec" => Forme::Rectangle,
        "o" | "oval" | "oblong" | "obround" | "ellipse" | "ovl" | "obl" => Forme::Oblong,
        "oct" | "octagon" | "octagonal" => Forme::Octogone,
        "annulus" | "donut" | "ring" | "target" => Forme::Anneau,
        "thermal" | "relief" | "thr" => Forme::Thermique,
        _ => return None,
    })
}

/// Le texte ressemble-t-il à une liste d'ouvertures (et non à un Gerber ou un perçage) ?
pub fn reconnaitre(nom: &str, texte: &str) -> bool {
    let bas = nom.to_ascii_lowercase();
    let ext = bas.rsplit('.').next().unwrap_or("");
    let debut: String = texte.chars().take(20_000).collect();
    // Commandes %AD seules (sans tracé) : liste d'ouvertures au format RS-274X.
    if debut.contains("%AD") && !debut.contains("D01*") && !debut.contains("D03*") && !debut.contains("D02*") {
        return true;
    }
    if debut.contains('*') && (debut.contains("D01") || debut.contains("D03")) {
        return false;
    }
    let lignes = debut.lines().filter(|l| ligne(l).is_some()).count();
    lignes >= 2 || (lignes >= 1 && matches!(ext, "whl" | "apr" | "apt" | "app" | "ap" | "wheel" | "rep" | "aper"))
}

/// (D-code, forme, nombres) d'une ligne, si elle en décrit une.
fn ligne(l: &str) -> Option<(i32, Forme, Vec<f64>, bool)> {
    let l = l.trim();
    if l.is_empty() || l.starts_with(';') || l.starts_with('#') || l.starts_with("//") {
        return None;
    }
    // Séparateurs : espaces, tabulations, virgules, points-virgules, « x » entre deux nombres.
    let propre: String = l.chars().map(|c| if c == ',' || c == ';' || c == '\t' || c == '=' || c == ':' { ' ' } else { c }).collect();
    let mots: Vec<&str> = propre.split_whitespace().collect();
    let pos_d = mots.iter().position(|m| {
        let m = m.trim_start_matches('(');
        (m.starts_with('D') || m.starts_with('d')) && m.len() > 1 && m[1..].chars().all(|c| c.is_ascii_digit())
    })?;
    let code: i32 = mots[pos_d].trim_start_matches('(')[1..].parse().ok()?;
    if code < 10 {
        return None;
    }
    let mut f = None;
    let mut nombres = Vec::new();
    let mut mil = false;
    for m in &mots[pos_d + 1..] {
        let bas = m.to_ascii_lowercase();
        if bas == "mil" || bas == "mils" || bas == "th" {
            mil = true;
            continue;
        }
        // « 0.050x0.060 » ou « 10mil »
        for morceau in bas.split('x') {
            let chiffres = morceau.trim_end_matches("mils").trim_end_matches("mil").trim_end_matches("mm").trim_end_matches("in");
            if morceau.ends_with("mil") || morceau.ends_with("mils") {
                mil = true;
            }
            if let Ok(v) = chiffres.parse::<f64>() {
                nombres.push(v);
            } else if f.is_none() {
                f = forme(chiffres);
            }
        }
    }
    Some((code, f?, nombres, mil))
}

/// Lit une liste d'ouvertures.
pub fn lire(texte: &str) -> Option<Liste> {
    // Commandes %AD dans un fichier à part (%ADD10C,0.010*%), avec l'unité de %MO si elle y est (pouces sinon).
    if texte.contains("%AD") {
        let mm = texte.contains("%MOMM");
        let k = if mm { 1.0 } else { 25.4 };
        let mut ouvertures = BTreeMap::new();
        for bloc in texte.split('%') {
            let Some(r) = bloc.trim().strip_prefix("ADD") else { continue };
            let r = r.trim_end_matches('*');
            let fin = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
            let Ok(code) = r[..fin].parse::<i32>() else { continue };
            let (f, p) = r[fin..].split_once(',').unwrap_or((&r[fin..], ""));
            let n: Vec<f64> = p.split('X').filter_map(|v| v.trim().parse().ok()).collect();
            let a = n.first().copied().unwrap_or(0.0) * k;
            let b = n.get(1).copied().unwrap_or(n.first().copied().unwrap_or(0.0)) * k;
            let forme = match f {
                "C" => Forme::Cercle,
                "R" => Forme::Rectangle,
                "O" => Forme::Oblong,
                "P" => Forme::Octogone,
                _ => continue,
            };
            ouvertures.insert(code, Ouverture { forme, a, b });
        }
        return (!ouvertures.is_empty()).then_some(Liste { ouvertures, unite: if mm { "mm" } else { "pouces" } });
    }
    let bas = texte.to_ascii_lowercase();
    let lignes: Vec<_> = texte.lines().filter_map(ligne).collect();
    if lignes.is_empty() {
        return None;
    }
    // Unité : déclarée dans le fichier, ou déduite de la taille des valeurs (moins de 1 : pouces ; sinon mils).
    let mm = bas.contains("metric") || bas.contains("millim") || bas.split(|c: char| !c.is_alphanumeric()).any(|m| m == "mm");
    let mils_declares = lignes.iter().any(|l| l.3) || bas.contains("mils") || bas.contains("thou");
    let max = lignes.iter().flat_map(|l| l.2.iter().copied()).fold(0.0, f64::max);
    let (k, unite) = if mm {
        (1.0, "mm")
    } else if mils_declares || max >= 1.0 {
        (0.0254, "mils")
    } else {
        (25.4, "pouces")
    };
    let mut ouvertures = BTreeMap::new();
    for (code, f, n, _) in lignes {
        let a = n.first().copied().unwrap_or(0.0) * k;
        let b = n.get(1).copied().unwrap_or(n.first().copied().unwrap_or(0.0)) * k;
        ouvertures.entry(code).or_insert(Ouverture { forme: f, a, b });
    }
    Some(Liste { ouvertures, unite })
}

/// Format (chiffres entiers, décimales, zéros de tête omis, facteur vers le mm) le plus probable pour les coordonnées
/// d'un Gerber RS-274D : celui qui donne une carte de taille plausible (de 2 à 700 mm). Les zéros de tête présents
/// dans les coordonnées signalent l'omission des zéros de fin.
pub fn format_probable(texte: &str) -> (usize, usize, bool, f64) {
    let mut valeurs: Vec<&str> = Vec::new();
    let mut tete = 0usize;
    let mut total = 0usize;
    for m in texte.split(['*', '\n', '\r']) {
        let b = m.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'X' || b[i] == b'Y' {
                let d = i + 1;
                let mut f = d;
                while f < b.len() && (b[f].is_ascii_digit() || b[f] == b'-' || b[f] == b'+') {
                    f += 1;
                }
                if f > d {
                    let v = &m[d..f];
                    let chiffres = v.trim_start_matches(['-', '+']);
                    if chiffres.len() > 1 {
                        total += 1;
                        if chiffres.starts_with('0') {
                            tete += 1;
                        }
                    }
                    valeurs.push(v);
                }
                i = f;
            } else {
                i += 1;
            }
        }
        if valeurs.len() > 20_000 {
            break;
        }
    }
    let zeros_tete_omis = !(total > 0 && tete * 2 > total);
    let longueur_max = valeurs.iter().map(|v| v.trim_start_matches(['-', '+']).len()).max().unwrap_or(6);
    let candidats: [(usize, usize, f64); 6] = [(2, 4, 25.4), (2, 3, 25.4), (3, 3, 1.0), (2, 5, 25.4), (3, 4, 1.0), (4, 2, 1.0)];
    for (ent, dec, k) in candidats {
        if !zeros_tete_omis && longueur_max > ent + dec {
            continue;
        }
        let lire = |v: &str| -> f64 {
            let (signe, c) = if let Some(r) = v.strip_prefix('-') { (-1.0, r) } else { (1.0, v.trim_start_matches('+')) };
            let mut s = c.to_string();
            if !zeros_tete_omis {
                while s.len() < ent + dec {
                    s.push('0');
                }
            }
            signe * s.parse::<f64>().unwrap_or(0.0) / 10f64.powi(dec as i32) * k
        };
        let nums: Vec<f64> = valeurs.iter().map(|v| lire(v)).collect();
        if nums.is_empty() {
            return (ent, dec, zeros_tete_omis, k);
        }
        let (min, max) = nums.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        let etendue = max - min;
        if (2.0..=700.0).contains(&etendue) {
            return (ent, dec, zeros_tete_omis, k);
        }
    }
    (2, 4, zeros_tete_omis, 25.4)
}
