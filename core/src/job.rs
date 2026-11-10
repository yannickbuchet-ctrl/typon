//! Fichier de job Gerber (.gbrjob, JSON) : informations générales de fabrication, empilage, rôle de chaque fichier.

use serde_json::Value;

pub struct Job {
    /// Rubriques lisibles : (titre, [(libellé, valeur)]).
    pub rubriques: Vec<(String, Vec<(String, String)>)>,
    /// Fichiers décrits : chemin -> .FileFunction (valeurs séparées).
    pub fichiers: Vec<(String, Vec<String>)>,
    /// Empilage : (couche, type, épaisseur en mm, matériau)
    pub empilage: Vec<(String, String, Option<f64>, String)>,
    pub epaisseur: Option<f64>,
    pub finition: Option<String>,
}

pub fn lire(texte: &str) -> Result<Job, String> {
    let v: Value = serde_json::from_str(texte).map_err(|e| format!("Fichier de job illisible : {e}"))?;
    let mut rubriques = Vec::new();
    let texte_de = |v: &Value| -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string().replace('.', ","),
            Value::Bool(b) => if *b { "oui".into() } else { "non".into() },
            Value::Array(a) => a.iter().map(|x| match x { Value::String(s) => s.clone(), autre => autre.to_string() }).collect::<Vec<_>>().join(", "),
            Value::Object(o) => o.iter().map(|(k, v)| format!("{k} : {}", match v { Value::String(s) => s.clone(), autre => autre.to_string() })).collect::<Vec<_>>().join(" ; "),
            Value::Null => String::new(),
        }
    };
    for (cle, titre) in [("Header", "En-tête"), ("GeneralSpecs", "Caractéristiques générales"), ("DesignRules", "Règles de conception")] {
        if let Some(o) = v.get(cle) {
            let mut lignes = Vec::new();
            aplatir(o, "", &mut lignes, &texte_de);
            if !lignes.is_empty() {
                rubriques.push((titre.to_string(), lignes.into_iter().map(|(k, v)| (libelle(&k), v)).collect()));
            }
        }
    }
    let mut fichiers = Vec::new();
    if let Some(Value::Array(a)) = v.get("FilesAttributes") {
        for f in a {
            let chemin = f.get("Path").and_then(|p| p.as_str()).unwrap_or("").to_string();
            let fonction = f.get("FileFunction").and_then(|p| p.as_str()).unwrap_or("").split(',').map(|s| s.to_string()).collect();
            fichiers.push((chemin, fonction));
        }
    }
    let mut empilage = Vec::new();
    if let Some(Value::Array(a)) = v.get("MaterialStackup") {
        for c in a {
            empilage.push((
                c.get("Name").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                c.get("Type").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                c.get("Thickness").and_then(|p| p.as_f64()),
                c.get("Material").and_then(|p| p.as_str()).unwrap_or("").to_string(),
            ));
        }
    }
    let epaisseur = v.get("GeneralSpecs").and_then(|g| g.get("BoardThickness")).and_then(|t| t.as_f64()).filter(|e| *e > 0.05 && *e < 20.0);
    let finition = v.get("GeneralSpecs").and_then(|g| g.get("Finish")).and_then(|t| t.as_str()).map(|s| s.to_string());
    Ok(Job { rubriques, fichiers, empilage, epaisseur, finition })
}

fn aplatir(v: &Value, prefixe: &str, sortie: &mut Vec<(String, String)>, texte_de: &dyn Fn(&Value) -> String) {
    match v {
        Value::Array(a) if a.iter().all(|x| x.is_object()) => {
            // Liste d'objets (règles de conception par groupe de couches) : un préfixe par élément.
            for (i, x) in a.iter().enumerate() {
                let nom = x.get("Layers").and_then(|l| l.as_str()).map(|l| match l {
                    "Outer" => "Couches externes".to_string(),
                    "Inner" => "Couches internes".to_string(),
                    "All" => "Toutes les couches".to_string(),
                    autre => autre.to_string(),
                }).unwrap_or_else(|| format!("{}", i + 1));
                let p = if prefixe.is_empty() { nom } else { format!("{prefixe} · {nom}") };
                if let Value::Object(o) = x {
                    for (k, y) in o {
                        if k == "Layers" {
                            continue;
                        }
                        let v = match y {
                            Value::Number(n) => format!("{} mm", n.to_string().replace('.', ",")),
                            autre => texte_de(autre),
                        };
                        sortie.push((format!("{p} · {k}"), v));
                    }
                }
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                let nom = if prefixe.is_empty() { k.clone() } else { format!("{prefixe} · {k}") };
                match x {
                    Value::Object(_) => aplatir(x, &nom, sortie, texte_de),
                    Value::Array(a) if a.iter().all(|y| y.is_object()) && !a.is_empty() => aplatir(x, &nom, sortie, texte_de),
                    Value::String(s) if k == "CreationDate" && s.len() >= 16 && s.as_bytes()[4] == b'-' => {
                        sortie.push((nom, format!("{}/{}/{} {}", &s[8..10], &s[5..7], &s[0..4], &s[11..16])))
                    }
                    Value::Number(n) if k.contains("Width") || k.contains("To") || k == "BoardThickness" || k == "X" || k == "Y" => {
                        sortie.push((nom, format!("{} mm", n.to_string().replace('.', ","))))
                    }
                    _ => sortie.push((nom, texte_de(x))),
                }
            }
        }
        autre => sortie.push((prefixe.to_string(), texte_de(autre))),
    }
}

fn libelle(k: &str) -> String {
    let traduire = |p: &str| -> String {
        match p {
            "GenerationSoftware" => "Logiciel".into(),
            "Vendor" => "Éditeur".into(),
            "Application" => "Application".into(),
            "Version" => "Version".into(),
            "CreationDate" => "Date de création".into(),
            "ProjectId" => "Projet".into(),
            "Name" => "Nom".into(),
            "GUID" => "Identifiant".into(),
            "Revision" => "Révision".into(),
            "Size" => "Taille".into(),
            "X" => "largeur".into(),
            "Y" => "hauteur".into(),
            "LayerNumber" => "Nombre de couches".into(),
            "BoardThickness" => "Épaisseur".into(),
            "Finish" => "Finition".into(),
            "ImpedanceControlled" => "Impédance contrôlée".into(),
            "Legend" => "Sérigraphie".into(),
            "SolderMask" => "Masque de soudure".into(),
            "Color" => "couleur".into(),
            "Top" => "dessus".into(),
            "Bottom" => "dessous".into(),
            "Layers" => "Couches".into(),
            "PadToPad" => "Écart pastille-pastille".into(),
            "PadToTrack" => "Écart pastille-piste".into(),
            "TrackToTrack" => "Écart piste-piste".into(),
            "MinLineWidth" => "Largeur de piste minimale".into(),
            "TrackToRegion" => "Écart piste-zone".into(),
            "RegionToRegion" => "Écart zone-zone".into(),
            "BoardOutline" => "Contour".into(),
            "Polarity" => "Polarité".into(),
            "IPC-6012Class" | "IPCClass" => "Classe IPC".into(),
            autre => autre.to_string(),
        }
    };
    k.split(" · ").map(traduire).collect::<Vec<_>>().join(" · ")
}
