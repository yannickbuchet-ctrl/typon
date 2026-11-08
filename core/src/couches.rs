//! Reconnaissance du rôle d'une couche : d'après l'attribut X2 .FileFunction, sinon d'après le nom du fichier
//! (conventions KiCad, Altium/Protel, Eagle, OrCAD, LibrePCB…). Donne aussi un nom lisible, une couleur et un rang
//! dans l'empilage (du dessus vers le dessous).

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Genre {
    Cuivre,
    Masque,
    Serigraphie,
    Pate,
    Contour,
    Percage,
    Fraisage,
    Composants,
    Mecanique,
    Plan,
    Netlist,
    Job,
    Autre,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Cote {
    Dessus,
    Dessous,
    Interne,
    Aucun,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub genre: Genre,
    pub cote: Cote,
    /// Numéro de couche cuivre (1 = dessus), si connu.
    pub numero: Option<u32>,
    pub libelle: String,
    pub couleur: &'static str,
    /// Rang d'affichage : les couches de rang élevé sont dessinées au-dessus.
    pub rang: i32,
    /// Origine de la reconnaissance : « X2 », « nom de fichier » ou « ODB++ ».
    pub source: &'static str,
}

pub fn couleur(genre: Genre, cote: Cote, numero: Option<u32>) -> &'static str {
    match (genre, cote) {
        (Genre::Cuivre, Cote::Dessus) => "#e8a33d",
        (Genre::Cuivre, Cote::Dessous) => "#3d8ee8",
        (Genre::Cuivre, _) => ["#d9534f", "#5cb85c", "#9b59b6", "#16a085", "#e67e22", "#c0392b", "#2980b9", "#8e44ad"][(numero.unwrap_or(2) as usize) % 8],
        (Genre::Masque, Cote::Dessous) => "#2e7d6b",
        (Genre::Masque, _) => "#3fae8a",
        (Genre::Serigraphie, _) => "#f2f2f2",
        (Genre::Pate, _) => "#9aa4b1",
        (Genre::Contour, _) => "#f5d76e",
        (Genre::Percage, _) | (Genre::Fraisage, _) => "#ff6b9d",
        (Genre::Composants, _) => "#b388ff",
        (Genre::Plan, _) => "#7f8c8d",
        _ => "#a0aec0",
    }
}

fn libelle_cote(cote: Cote) -> &'static str {
    match cote {
        Cote::Dessus => "dessus",
        Cote::Dessous => "dessous",
        Cote::Interne => "interne",
        Cote::Aucun => "",
    }
}

pub fn nommer(genre: Genre, cote: Cote, numero: Option<u32>) -> String {
    let c = libelle_cote(cote);
    let base = match genre {
        Genre::Cuivre => match (cote, numero) {
            (Cote::Interne, Some(n)) => return format!("Cuivre interne {n}"),
            _ => "Cuivre",
        },
        Genre::Masque => "Masque de soudure",
        Genre::Serigraphie => "Sérigraphie",
        Genre::Pate => "Pâte à braser",
        Genre::Contour => return "Contour de la carte".into(),
        Genre::Percage => "Perçage",
        Genre::Fraisage => "Fraisage",
        Genre::Composants => "Composants",
        Genre::Mecanique => "Mécanique",
        Genre::Plan => "Plan de perçage",
        Genre::Netlist => return "Netlist".into(),
        Genre::Job => return "Fichier de job".into(),
        Genre::Autre => return "Autre".into(),
    };
    if c.is_empty() { base.to_string() } else { format!("{base} {c}") }
}

fn rang(genre: Genre, cote: Cote, numero: Option<u32>) -> i32 {
    // Dessinées de bas en haut : dessous d'abord, puis internes, puis dessus ; perçages et contour par-dessus.
    let base = match cote {
        Cote::Dessous => 0,
        Cote::Interne => 100,
        Cote::Dessus | Cote::Aucun => 200,
    };
    let ordre = match (genre, cote) {
        (Genre::Cuivre, Cote::Interne) => -(numero.unwrap_or(2) as i32),
        (Genre::Cuivre, _) => 10,
        (Genre::Masque, _) => 20,
        (Genre::Pate, _) => 25,
        (Genre::Serigraphie, _) => 30,
        (Genre::Composants, _) => 40,
        (Genre::Mecanique, _) => 45,
        _ => 0,
    };
    let ordre = if cote == Cote::Dessous { 50 - ordre } else { ordre };
    match genre {
        Genre::Percage | Genre::Fraisage => 300,
        Genre::Contour => 320,
        Genre::Plan | Genre::Autre => 280,
        _ => base + ordre,
    }
}

fn role(genre: Genre, cote: Cote, numero: Option<u32>, source: &'static str) -> Role {
    Role { genre, cote, numero, libelle: nommer(genre, cote, numero), couleur: couleur(genre, cote, numero), rang: rang(genre, cote, numero), source }
}

/// D'après l'attribut .FileFunction (valeurs déjà séparées).
pub fn depuis_x2(f: &[String]) -> Option<Role> {
    let g = |i: usize| f.get(i).map(|s| s.as_str()).unwrap_or("");
    let cote = |s: &str| match s {
        "Top" => Cote::Dessus,
        "Bot" => Cote::Dessous,
        "Inr" => Cote::Interne,
        _ => Cote::Aucun,
    };
    let r = match g(0) {
        "Copper" => {
            let n = g(1).trim_start_matches('L').parse().ok();
            role(Genre::Cuivre, cote(g(2)), n, "X2")
        }
        "Soldermask" => role(Genre::Masque, cote(g(1)), None, "X2"),
        "Legend" => role(Genre::Serigraphie, cote(g(1)), None, "X2"),
        "Paste" => role(Genre::Pate, cote(g(1)), None, "X2"),
        "Profile" => role(Genre::Contour, Cote::Aucun, None, "X2"),
        "Plated" | "NonPlated" => {
            let fraisage = f.iter().any(|v| v == "Rout");
            let mixte = f.iter().any(|v| v == "Mixed");
            let mut r = role(if fraisage { Genre::Fraisage } else { Genre::Percage }, Cote::Aucun, None, "X2");
            let (a, b) = (g(1), g(2));
            let genre = if g(0) == "Plated" { "métallisé" } else { "non métallisé" };
            r.libelle = format!("{} {genre} {a}–{b}{}", if fraisage { "Fraisage" } else { "Perçage" }, if mixte { " (avec fraisages)" } else { "" });
            r
        }
        "Component" => role(Genre::Composants, cote(g(2)), None, "X2"),
        "Drillmap" | "FabricationDrawing" | "AssemblyDrawing" | "ArrayDrawing" => {
            let mut r = role(Genre::Plan, cote(g(1)), None, "X2");
            r.libelle = match g(0) {
                "Drillmap" => "Plan de perçage".into(),
                "FabricationDrawing" => "Plan de fabrication".into(),
                "AssemblyDrawing" => format!("Plan d'assemblage {}", libelle_cote(cote(g(1)))).trim().to_string(),
                _ => "Plan du panneau".into(),
            };
            r
        }
        "Glue" => {
            let mut r = role(Genre::Mecanique, cote(g(1)), None, "X2");
            r.libelle = format!("Colle {}", libelle_cote(cote(g(1))));
            r
        }
        "Carbonmask" | "Goldmask" | "Silvermask" | "Tinmask" | "Peelablemask" | "Heatsinkmask" => {
            let mut r = role(Genre::Mecanique, cote(g(1)), None, "X2");
            r.libelle = format!("{} {}", g(0), libelle_cote(cote(g(1))));
            r
        }
        "Keep-out" | "Pads" | "Viafill" | "Vcut" | "Vcutmap" | "Depthrout" | "Other" | "Plated-Drill" => {
            let mut r = role(Genre::Autre, cote(g(1)), None, "X2");
            r.libelle = format!("{} {}", g(0), g(1)).trim().to_string();
            r
        }
        "" => return None,
        autre => {
            let mut r = role(Genre::Autre, Cote::Aucun, None, "X2");
            r.libelle = autre.to_string();
            r
        }
    };
    Some(r)
}

/// D'après le nom du fichier (sans le dossier).
pub fn depuis_nom(chemin: &str) -> Role {
    let nom = chemin.rsplit(['/', '\\']).next().unwrap_or(chemin);
    let bas = nom.to_ascii_lowercase();
    let ext = bas.rsplit('.').next().unwrap_or("");
    let s = "nom de fichier";
    let r = |g, c, n| role(g, c, n, s);
    // Extensions Altium / Protel
    match ext {
        "gtl" => return r(Genre::Cuivre, Cote::Dessus, Some(1)),
        "gbl" => return r(Genre::Cuivre, Cote::Dessous, None),
        "gts" => return r(Genre::Masque, Cote::Dessus, None),
        "gbs" => return r(Genre::Masque, Cote::Dessous, None),
        "gto" => return r(Genre::Serigraphie, Cote::Dessus, None),
        "gbo" => return r(Genre::Serigraphie, Cote::Dessous, None),
        "gtp" => return r(Genre::Pate, Cote::Dessus, None),
        "gbp" => return r(Genre::Pate, Cote::Dessous, None),
        "gko" | "gm1" | "gml" | "gm" | "outline" | "oln" => return r(Genre::Contour, Cote::Aucun, None),
        "gd1" | "gg1" => return r(Genre::Plan, Cote::Aucun, None),
        "drl" | "xln" | "exc" | "drd" | "xnc" | "nc" | "ncd" => return r(Genre::Percage, Cote::Aucun, None),
        // Eagle
        "cmp" => return r(Genre::Cuivre, Cote::Dessus, Some(1)),
        "sol" => return r(Genre::Cuivre, Cote::Dessous, None),
        "stc" => return r(Genre::Masque, Cote::Dessus, None),
        "sts" => return r(Genre::Masque, Cote::Dessous, None),
        "plc" => return r(Genre::Serigraphie, Cote::Dessus, None),
        "pls" => return r(Genre::Serigraphie, Cote::Dessous, None),
        "crc" => return r(Genre::Pate, Cote::Dessus, None),
        "crs" => return r(Genre::Pate, Cote::Dessous, None),
        "dim" | "mil" => return r(Genre::Contour, Cote::Aucun, None),
        "ipc" | "net" if bas.contains("ipc") || ext == "ipc" => return r(Genre::Netlist, Cote::Aucun, None),
        "gbrjob" => return r(Genre::Job, Cote::Aucun, None),
        _ => {}
    }
    // Couches internes Altium : .G1 … .G30, plans .GP1 …
    if let Some(n) = ext.strip_prefix('g').and_then(|v| v.parse::<u32>().ok()) {
        return r(Genre::Cuivre, Cote::Interne, Some(n + 1));
    }
    if let Some(n) = ext.strip_prefix("gp").and_then(|v| v.parse::<u32>().ok()) {
        return r(Genre::Cuivre, Cote::Interne, Some(n + 1));
    }
    // Noms KiCad, LibrePCB et génériques (dans le nom lui-même)
    let a = |motifs: &[&str]| motifs.iter().any(|m| bas.contains(m));
    let dessus = a(&["f_", "f.", "-f_", "top", "_t.", "front", "-f."]);
    let dessous = a(&["b_", "b.", "-b_", "bot", "bottom", "back", "-b."]);
    let cote = if dessous && !a(&["top"]) { Cote::Dessous } else if dessus { Cote::Dessus } else { Cote::Aucun };
    if a(&["edge_cuts", "edge.cuts", "outline", "profile", "contour", "board_outline", "boardoutline"]) {
        return r(Genre::Contour, Cote::Aucun, None);
    }
    if a(&["npth", "pth", "drill", "drl", "perc"]) && !a(&["drill_map", "drillmap", "map"]) {
        return r(Genre::Percage, Cote::Aucun, None);
    }
    if a(&["mask", "soldermask", "solder_mask", "masque"]) {
        return r(Genre::Masque, cote, None);
    }
    if a(&["paste", "cream", "pate"]) {
        return r(Genre::Pate, cote, None);
    }
    if a(&["silk", "legend", "overlay", "serigraph"]) {
        return r(Genre::Serigraphie, cote, None);
    }
    if a(&["pnp", "pos", "component", "assembly"]) {
        return r(Genre::Composants, cote, None);
    }
    if let Some(n) = numero_interne(&bas) {
        return r(Genre::Cuivre, Cote::Interne, Some(n));
    }
    if a(&["cu", "copper", "cuivre", "signal", "layer"]) {
        return r(Genre::Cuivre, if cote == Cote::Aucun { Cote::Dessus } else { cote }, if cote == Cote::Dessous { None } else { Some(1) });
    }
    r(Genre::Autre, cote, None)
}

fn numero_interne(bas: &str) -> Option<u32> {
    for motif in ["in", "inner", "l"] {
        for (i, _) in bas.match_indices(motif) {
            let suite = &bas[i + motif.len()..];
            let chiffres: String = suite.chars().take_while(|c| c.is_ascii_digit()).collect();
            let apres = suite.chars().nth(chiffres.len());
            if !chiffres.is_empty() && apres.map(|c| c == '_' || c == '.' || c == '-' || c == '$').unwrap_or(true) {
                let n: u32 = chiffres.parse().ok()?;
                // KiCad numérote In1 pour la deuxième couche
                return Some(if motif == "in" || motif == "inner" { n + 1 } else { n });
            }
        }
    }
    None
}
